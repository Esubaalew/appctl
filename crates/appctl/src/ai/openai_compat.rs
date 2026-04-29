use anyhow::{Context, Result, bail};
use futures::StreamExt;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::{
    ai::{AgentStep, LlmProvider, Message, ToolCall},
    config::ResolvedProvider,
    events::AgentEvent,
    term::format_api_error_summary,
    tools::ToolDef,
};

pub struct OpenAiCompatProvider {
    client: reqwest::Client,
    config: ResolvedProvider,
}

impl OpenAiCompatProvider {
    pub fn new(config: ResolvedProvider) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }

    fn endpoint_description(&self) -> String {
        format!(
            r#""{}" at {}"#,
            self.config.name,
            self.config.base_url.trim_end_matches('/')
        )
    }
}

/// Short hint when the base URL looks local (Ollama, LM Studio, vLLM, etc.).
fn local_model_server_hint(config: &ResolvedProvider) -> &'static str {
    let u = config.base_url.to_lowercase();
    if u.contains("127.0.0.1") || u.contains("localhost") {
        " Is the local model server running (e.g. `ollama serve`)? The DB connection only provides tools; chat still needs that HTTP endpoint."
    } else {
        " Database tools are ready separately; this step only talks to the model’s HTTP API."
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiCompatProvider {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        events: Option<mpsc::Sender<AgentEvent>>,
    ) -> Result<AgentStep> {
        let mut request = self.client.post(format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        ));
        if let Some(api_key) = self.config.auth.api_key() {
            request = request.bearer_auth(api_key);
        }
        if let Some(token) = self.config.auth.bearer_token() {
            request = request.bearer_auth(token);
        }
        for (name, value) in &self.config.extra_headers {
            request = request.header(name, value);
        }

        let payload = json!({
            "model": self.config.model,
            "messages": messages.iter().map(serialize_message).collect::<Vec<_>>(),
            "tools": tools.iter().map(|tool| json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema
                }
            })).collect::<Vec<_>>(),
            "tool_choice": "auto",
            "stream": true
        });

        let endpoint = self.endpoint_description();
        let response = request.json(&payload).send().await.map_err(|e| {
            anyhow::anyhow!(
                "Could not reach the model HTTP endpoint ({}): {}{}",
                endpoint,
                e,
                local_model_server_hint(&self.config)
            )
        })?;
        parse_openai_stream_response(response, &endpoint, events).await
    }
}

#[derive(Debug, Default)]
struct PendingToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

pub(crate) async fn parse_openai_stream_response(
    response: reqwest::Response,
    endpoint: &str,
    events: Option<mpsc::Sender<AgentEvent>>,
) -> Result<AgentStep> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.with_context(|| {
            format!("Failed to read response body from the model at {endpoint}.")
        })?;
        bail!(
            "Model HTTP API returned status {} ({}). {}",
            status,
            endpoint,
            format_api_error_summary(&body)
        );
    }

    let mut content = String::new();
    let mut calls = Vec::<PendingToolCall>::new();
    let mut buffer = String::new();
    let mut raw_body = String::new();
    let mut saw_sse_event = false;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.with_context(|| format!("Failed to read stream from {endpoint}."))?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);
        raw_body.push_str(&text);
        while let Some((event, rest)) = take_sse_event(&buffer) {
            buffer = rest;
            if process_openai_sse_event(&event, &mut content, &mut calls, &events, endpoint).await?
            {
                saw_sse_event = true;
            }
        }
    }

    if !buffer.trim().is_empty()
        && process_openai_sse_event(&buffer, &mut content, &mut calls, &events, endpoint).await?
    {
        saw_sse_event = true;
    }

    if !saw_sse_event {
        let response: Value = serde_json::from_str(raw_body.trim()).with_context(|| {
            format!(
                "Could not parse JSON or streaming data from the model at {}: {}",
                endpoint,
                format_api_error_summary(&raw_body)
            )
        })?;
        return parse_openai_message_response(&response, endpoint);
    }

    let calls = finish_tool_calls(calls);
    if !calls.is_empty() {
        Ok(AgentStep::ToolCalls { calls })
    } else if content.is_empty() {
        Ok(AgentStep::Stop)
    } else {
        Ok(AgentStep::Message { content })
    }
}

fn take_sse_event(buffer: &str) -> Option<(String, String)> {
    let lf = buffer.find("\n\n");
    let crlf = buffer.find("\r\n\r\n");
    let (idx, sep_len) = match (lf, crlf) {
        (Some(a), Some(b)) if b < a => (b, 4),
        (Some(a), _) => (a, 2),
        (_, Some(b)) => (b, 4),
        _ => return None,
    };
    Some((
        buffer[..idx].to_string(),
        buffer[idx + sep_len..].to_string(),
    ))
}

async fn process_openai_sse_event(
    event: &str,
    content: &mut String,
    calls: &mut Vec<PendingToolCall>,
    events: &Option<mpsc::Sender<AgentEvent>>,
    endpoint: &str,
) -> Result<bool> {
    let data = event
        .lines()
        .filter_map(|line| line.trim_end_matches('\r').strip_prefix("data:"))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n");
    if data.is_empty() {
        return Ok(false);
    }
    if data == "[DONE]" {
        return Ok(true);
    }
    let chunk: Value = serde_json::from_str(&data).with_context(|| {
        format!(
            "Could not parse streaming JSON from the model at {}: {}",
            endpoint,
            format_api_error_summary(&data)
        )
    })?;
    for choice in chunk
        .get("choices")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(delta) = choice.get("delta").or_else(|| choice.get("message")) {
            if let Some(text) = delta.get("content").and_then(Value::as_str)
                && !text.is_empty()
            {
                content.push_str(text);
                send_delta(events, text).await;
            }
            if let Some(tool_deltas) = delta.get("tool_calls").and_then(Value::as_array) {
                merge_tool_deltas(calls, tool_deltas);
            }
        }
    }
    Ok(true)
}

async fn send_delta(events: &Option<mpsc::Sender<AgentEvent>>, text: &str) {
    if let Some(tx) = events {
        let _ = tx
            .send(AgentEvent::AssistantDelta {
                text: text.to_string(),
            })
            .await;
    }
}

fn merge_tool_deltas(calls: &mut Vec<PendingToolCall>, tool_deltas: &[Value]) {
    for delta in tool_deltas {
        let index = delta.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
        if calls.len() <= index {
            calls.resize_with(index + 1, PendingToolCall::default);
        }
        let call = &mut calls[index];
        if let Some(id) = delta.get("id").and_then(Value::as_str)
            && !id.is_empty()
        {
            call.id = Some(id.to_string());
        }
        if let Some(name) = delta.pointer("/function/name").and_then(Value::as_str)
            && !name.is_empty()
        {
            call.name = Some(name.to_string());
        }
        if let Some(arguments) = delta.pointer("/function/arguments").and_then(Value::as_str) {
            call.arguments.push_str(arguments);
        }
    }
}

fn finish_tool_calls(calls: Vec<PendingToolCall>) -> Vec<ToolCall> {
    calls
        .into_iter()
        .filter_map(|call| {
            let name = call.name.unwrap_or_default();
            if name.is_empty() {
                return None;
            }
            Some(ToolCall {
                id: call.id.unwrap_or_else(|| "tool".to_string()),
                name,
                arguments: serde_json::from_str(&call.arguments)
                    .unwrap_or(Value::Object(serde_json::Map::new())),
            })
        })
        .collect()
}

fn parse_openai_message_response(response: &Value, endpoint: &str) -> Result<AgentStep> {
    let message = response.pointer("/choices/0/message").with_context(|| {
        format!(
            "The model at {endpoint} returned no assistant message (empty or unexpected layout)."
        )
    })?;

    if let Some(tool_calls) = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .filter(|calls| !calls.is_empty())
    {
        let calls = tool_calls
            .iter()
            .map(|call| ToolCall {
                id: call
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("tool")
                    .to_string(),
                name: call
                    .pointer("/function/name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                arguments: call
                    .pointer("/function/arguments")
                    .and_then(Value::as_str)
                    .and_then(|raw| serde_json::from_str(raw).ok())
                    .unwrap_or(Value::Object(serde_json::Map::new())),
            })
            .collect();
        return Ok(AgentStep::ToolCalls { calls });
    }

    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if content.is_empty() {
        Ok(AgentStep::Stop)
    } else {
        Ok(AgentStep::Message { content })
    }
}

fn serialize_message(message: &Message) -> Value {
    match message.role.as_str() {
        "assistant" if !message.tool_calls.is_empty() => json!({
            "role": "assistant",
            "content": if message.content.is_empty() { Value::Null } else { Value::String(message.content.clone()) },
            "tool_calls": message.tool_calls.iter().map(|call| json!({
                "id": call.id,
                "type": "function",
                "function": {
                    "name": call.name,
                    "arguments": serde_json::to_string(&call.arguments).unwrap_or_else(|_| "{}".to_string())
                }
            })).collect::<Vec<_>>()
        }),
        "tool" => json!({
            "role": "tool",
            "tool_call_id": message.tool_call_id,
            "content": message.content
        }),
        _ => json!({
            "role": message.role,
            "content": if message.content.is_empty() { Value::Null } else { Value::String(message.content.clone()) }
        }),
    }
}
