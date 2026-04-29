use anyhow::{Context, Result, bail};
use futures::StreamExt;
use serde_json::{Map, Value, json};
use tokio::sync::mpsc;

use crate::{
    ai::{AgentStep, LlmProvider, Message, ToolCall},
    config::ResolvedProvider,
    events::AgentEvent,
    term::{
        format_api_error_summary, format_google_error_detail_line,
        user_message_google_genai_http_error,
    },
    tools::ToolDef,
};

pub struct GoogleGenaiProvider {
    client: reqwest::Client,
    config: ResolvedProvider,
}

impl GoogleGenaiProvider {
    pub fn new(config: ResolvedProvider) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for GoogleGenaiProvider {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        events: Option<mpsc::Sender<AgentEvent>>,
    ) -> Result<AgentStep> {
        let mut request = self.client.post(format!(
            "{}/v1beta/models/{}:streamGenerateContent?alt=sse",
            self.config.base_url.trim_end_matches('/'),
            self.config.model
        ));
        if let Some(api_key) = self.config.auth.api_key() {
            request = request.header("x-goog-api-key", api_key);
        }
        if let Some(token) = self.config.auth.bearer_token() {
            request = request.bearer_auth(token);
        }
        for (name, value) in &self.config.extra_headers {
            request = request.header(name, value);
        }

        let payload = json!({
            "systemInstruction": system_instruction(messages),
            "contents": messages.iter().filter(|message| message.role != "system").map(serialize_message).collect::<Vec<_>>(),
            "tools": [
                {
                    "functionDeclarations": tools.iter().map(serialize_tool).collect::<Vec<_>>()
                }
            ]
        });

        let response = request
            .json(&payload)
            .send()
            .await
            .context("failed to call Google GenAI API")?;
        parse_google_stream_response(response, "Google GenAI", Some(&self.config.model), events)
            .await
    }
}

pub(crate) async fn parse_google_stream_response(
    response: reqwest::Response,
    provider_label: &str,
    model: Option<&str>,
    events: Option<mpsc::Sender<AgentEvent>>,
) -> Result<AgentStep> {
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .with_context(|| format!("failed to read {provider_label} response body"))?;
        if let Some(model) = model {
            bail!(
                "{}",
                user_message_google_genai_http_error(status.as_u16(), &body, model)
            );
        }
        bail!(
            "{} returned {}: {}",
            provider_label,
            status,
            format_api_error_summary(&body)
        );
    }

    let mut buffer = String::new();
    let mut raw_body = String::new();
    let mut saw_sse_event = false;
    let mut text = String::new();
    let mut thought = String::new();
    let mut tool_calls = Vec::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.with_context(|| format!("failed to read {provider_label} stream"))?;
        let chunk = String::from_utf8_lossy(&chunk);
        buffer.push_str(&chunk);
        raw_body.push_str(&chunk);
        while let Some((event, rest)) = take_sse_event(&buffer) {
            buffer = rest;
            if process_google_sse_event(&event, &mut text, &mut thought, &mut tool_calls, &events)
                .await?
            {
                saw_sse_event = true;
            }
        }
    }

    if !buffer.trim().is_empty()
        && process_google_sse_event(&buffer, &mut text, &mut thought, &mut tool_calls, &events)
            .await?
    {
        saw_sse_event = true;
    }

    if !thought.is_empty()
        && let Some(tx) = &events
    {
        let _ = tx
            .send(AgentEvent::AssistantThought { text: thought })
            .await;
    }

    if !saw_sse_event {
        let response: Value = serde_json::from_str(raw_body.trim()).with_context(|| {
            format!(
                "failed to parse {provider_label} response as JSON or streaming data: {}",
                format_api_error_summary(&raw_body)
            )
        })?;
        return parse_google_response(&response, &raw_body);
    }

    if !tool_calls.is_empty() {
        Ok(AgentStep::ToolCalls { calls: tool_calls })
    } else if text.is_empty() {
        Ok(AgentStep::Stop)
    } else {
        Ok(AgentStep::Message { content: text })
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

async fn process_google_sse_event(
    event: &str,
    text: &mut String,
    thought: &mut String,
    tool_calls: &mut Vec<ToolCall>,
    events: &Option<mpsc::Sender<AgentEvent>>,
) -> Result<bool> {
    let data = event
        .lines()
        .filter_map(|line| line.trim_end_matches('\r').strip_prefix("data:"))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n");
    if data.is_empty() || data == "[DONE]" {
        return Ok(!data.is_empty());
    }
    let response: Value = serde_json::from_str(&data).with_context(|| {
        format!(
            "failed to parse Google streaming JSON: {}",
            format_api_error_summary(&data)
        )
    })?;
    collect_google_parts(&response, text, thought, tool_calls, events).await?;
    Ok(true)
}

async fn collect_google_parts(
    response: &Value,
    text: &mut String,
    thought: &mut String,
    tool_calls: &mut Vec<ToolCall>,
    events: &Option<mpsc::Sender<AgentEvent>>,
) -> Result<()> {
    let Some(parts) = response
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
    else {
        return Ok(());
    };
    for part in parts {
        let is_thought = part
            .get("thought")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if let Some(chunk) = part.get("text").and_then(Value::as_str)
            && !chunk.is_empty()
        {
            if is_thought {
                thought.push_str(chunk);
                if let Some(tx) = events {
                    let _ = tx
                        .send(AgentEvent::AssistantThoughtDelta {
                            text: chunk.to_string(),
                        })
                        .await;
                }
            } else {
                text.push_str(chunk);
                if let Some(tx) = events {
                    let _ = tx
                        .send(AgentEvent::AssistantDelta {
                            text: chunk.to_string(),
                        })
                        .await;
                }
            }
        }
        if let Some(call) = part.get("functionCall").and_then(Value::as_object) {
            tool_calls.push(ToolCall {
                id: call
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("tool")
                    .to_string(),
                name: call
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                arguments: call
                    .get("args")
                    .cloned()
                    .unwrap_or(Value::Object(Map::new())),
            });
        }
    }
    Ok(())
}

fn parse_google_response(response: &Value, body: &str) -> Result<AgentStep> {
    let Some(candidate) = response
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
    else {
        let feedback = response
            .get("promptFeedback")
            .map(|value| format_google_error_detail_line(&value.to_string()))
            .unwrap_or_else(|| format_google_error_detail_line(body));
        bail!(
            "appctl could not read a normal model reply (blocked, safety, or empty output). {}",
            feedback
        );
    };

    let mut tool_calls = Vec::new();
    let mut text = String::new();
    for part in candidate {
        if part
            .get("thought")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        if let Some(chunk) = part.get("text").and_then(Value::as_str) {
            text.push_str(chunk);
        }
        if let Some(call) = part.get("functionCall").and_then(Value::as_object) {
            tool_calls.push(ToolCall {
                id: call
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("tool")
                    .to_string(),
                name: call
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                arguments: call
                    .get("args")
                    .cloned()
                    .unwrap_or(Value::Object(Map::new())),
            });
        }
    }

    if !tool_calls.is_empty() {
        Ok(AgentStep::ToolCalls { calls: tool_calls })
    } else if text.is_empty() {
        Ok(AgentStep::Stop)
    } else {
        Ok(AgentStep::Message { content: text })
    }
}

fn system_instruction(messages: &[Message]) -> Value {
    match messages.iter().find(|message| message.role == "system") {
        Some(message) => json!({
            "parts": [{ "text": message.content }]
        }),
        None => Value::Null,
    }
}

fn serialize_message(message: &Message) -> Value {
    match message.role.as_str() {
        "assistant" if !message.tool_calls.is_empty() => json!({
            "role": "model",
            "parts": message.tool_calls.iter().map(|call| json!({
                "functionCall": {
                    "id": call.id,
                    "name": call.name,
                    "args": call.arguments,
                }
            })).collect::<Vec<_>>()
        }),
        "assistant" => json!({
            "role": "model",
            "parts": [{ "text": message.content }]
        }),
        "tool" => {
            let response = function_response_payload(&message.content);
            json!({
                "role": "user",
                "parts": [{
                    "functionResponse": {
                        "id": message.tool_call_id,
                        "name": message.tool_name.clone().unwrap_or_default(),
                        "response": response,
                    }
                }]
            })
        }
        _ => json!({
            "role": "user",
            "parts": [{ "text": message.content }]
        }),
    }
}

fn function_response_payload(content: &str) -> Value {
    match serde_json::from_str::<Value>(content) {
        Ok(Value::Object(map)) => Value::Object(map),
        Ok(other) => json!({ "value": other }),
        Err(_) => json!({ "content": content }),
    }
}

fn serialize_tool(tool: &ToolDef) -> Value {
    json!({
        "name": tool.name,
        "description": tool.description,
        "parameters": sanitize_genai_schema(tool.input_schema.clone()),
    })
}

fn sanitize_genai_schema(value: Value) -> Value {
    match value {
        Value::Object(mut map) => {
            map.remove("additionalProperties");
            let keys = map.keys().cloned().collect::<Vec<_>>();
            for key in keys {
                if let Some(child) = map.remove(&key) {
                    map.insert(key, sanitize_genai_schema(child));
                }
            }
            Value::Object(map)
        }
        Value::Array(values) => {
            Value::Array(values.into_iter().map(sanitize_genai_schema).collect())
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::function_response_payload;
    use serde_json::{Value, json};

    #[test]
    fn wraps_non_json_tool_content_as_struct_object() {
        let payload = function_response_payload(
            "appctl tool summary: HTTP 200\n tool result JSON: {\"ok\":true}",
        );

        assert_eq!(
            payload,
            json!({
                "content": "appctl tool summary: HTTP 200\n tool result JSON: {\"ok\":true}"
            })
        );
        assert!(matches!(payload, Value::Object(_)));
    }

    #[test]
    fn keeps_json_object_tool_content_as_struct_object() {
        let payload = function_response_payload("{\"ok\":true,\"status\":200}");

        assert_eq!(payload, json!({"ok": true, "status": 200}));
    }
}
