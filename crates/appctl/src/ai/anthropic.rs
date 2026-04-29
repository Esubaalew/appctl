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

pub struct AnthropicProvider {
    client: reqwest::Client,
    config: ResolvedProvider,
}

impl AnthropicProvider {
    pub fn new(config: ResolvedProvider) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for AnthropicProvider {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        events: Option<mpsc::Sender<AgentEvent>>,
    ) -> Result<AgentStep> {
        let mut request = self
            .client
            .post(format!(
                "{}/v1/messages",
                self.config.base_url.trim_end_matches('/')
            ))
            .header("anthropic-version", "2023-06-01");
        if let Some(api_key) = self.config.auth.api_key() {
            request = request.header("x-api-key", api_key);
        }
        if let Some(token) = self.config.auth.bearer_token() {
            request = request.bearer_auth(token);
        }
        for (name, value) in &self.config.extra_headers {
            request = request.header(name, value);
        }

        let payload = json!({
            "model": self.config.model,
            "max_tokens": 2048,
            "system": messages
                .iter()
                .find(|message| message.role == "system")
                .map(|message| message.content.clone())
                .unwrap_or_default(),
            "messages": messages
                .iter()
                .filter(|message| message.role != "system")
                .map(|message| json!({
                    "role": if message.role == "tool" { "user".to_string() } else { message.role.clone() },
                    "content": message.content
                }))
                .collect::<Vec<_>>(),
            "tools": tools.iter().map(|tool| json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.input_schema
            })).collect::<Vec<_>>(),
            "stream": true
        });

        let response = request
            .json(&payload)
            .send()
            .await
            .context("failed to call Anthropic API")?;
        parse_anthropic_stream_response(response, events).await
    }
}

#[derive(Debug, Default)]
struct AnthropicToolBlock {
    id: Option<String>,
    name: Option<String>,
    input: Option<Value>,
    input_json: String,
}

async fn parse_anthropic_stream_response(
    response: reqwest::Response,
    events: Option<mpsc::Sender<AgentEvent>>,
) -> Result<AgentStep> {
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .context("failed to read Anthropic response body")?;
        bail!(
            "Anthropic API returned {}: {}",
            status,
            format_api_error_summary(&body)
        );
    }

    let mut buffer = String::new();
    let mut raw_body = String::new();
    let mut saw_sse_event = false;
    let mut text = String::new();
    let mut thought = String::new();
    let mut tool_blocks = Vec::<AnthropicToolBlock>::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("failed to read Anthropic stream")?;
        let chunk = String::from_utf8_lossy(&chunk);
        buffer.push_str(&chunk);
        raw_body.push_str(&chunk);
        while let Some((event, rest)) = take_sse_event(&buffer) {
            buffer = rest;
            if process_anthropic_sse_event(
                &event,
                &mut text,
                &mut thought,
                &mut tool_blocks,
                &events,
            )
            .await?
            {
                saw_sse_event = true;
            }
        }
    }

    if !buffer.trim().is_empty()
        && process_anthropic_sse_event(&buffer, &mut text, &mut thought, &mut tool_blocks, &events)
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
                "failed to parse Anthropic response as JSON or streaming data: {}",
                format_api_error_summary(&raw_body)
            )
        })?;
        return parse_anthropic_response(&response);
    }

    let calls = tool_blocks
        .into_iter()
        .filter_map(|block| {
            let name = block.name.unwrap_or_default();
            if name.is_empty() {
                return None;
            }
            let arguments = if block.input_json.trim().is_empty() {
                block.input.unwrap_or(Value::Null)
            } else {
                serde_json::from_str(&block.input_json).unwrap_or(Value::Null)
            };
            Some(ToolCall {
                id: block.id.unwrap_or_else(|| "tool".to_string()),
                name,
                arguments,
            })
        })
        .collect::<Vec<_>>();

    if !calls.is_empty() {
        Ok(AgentStep::ToolCalls { calls })
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

async fn process_anthropic_sse_event(
    event: &str,
    text: &mut String,
    thought: &mut String,
    tool_blocks: &mut Vec<AnthropicToolBlock>,
    events: &Option<mpsc::Sender<AgentEvent>>,
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

    let chunk: Value = serde_json::from_str(&data).with_context(|| {
        format!(
            "failed to parse Anthropic streaming JSON: {}",
            format_api_error_summary(&data)
        )
    })?;
    match chunk
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "content_block_start"
            if chunk
                .pointer("/content_block/type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                == "tool_use" =>
        {
            let index = chunk.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            if tool_blocks.len() <= index {
                tool_blocks.resize_with(index + 1, AnthropicToolBlock::default);
            }
            let block = &mut tool_blocks[index];
            block.id = chunk
                .pointer("/content_block/id")
                .and_then(Value::as_str)
                .map(str::to_string);
            block.name = chunk
                .pointer("/content_block/name")
                .and_then(Value::as_str)
                .map(str::to_string);
            block.input = chunk.pointer("/content_block/input").cloned();
        }
        "content_block_delta" => {
            if let Some(delta) = chunk.get("delta") {
                match delta
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                {
                    "text_delta" => {
                        if let Some(chunk_text) = delta.get("text").and_then(Value::as_str)
                            && !chunk_text.is_empty()
                        {
                            text.push_str(chunk_text);
                            if let Some(tx) = events {
                                let _ = tx
                                    .send(AgentEvent::AssistantDelta {
                                        text: chunk_text.to_string(),
                                    })
                                    .await;
                            }
                        }
                    }
                    "thinking_delta" => {
                        if let Some(chunk_text) = delta.get("thinking").and_then(Value::as_str)
                            && !chunk_text.is_empty()
                        {
                            thought.push_str(chunk_text);
                            if let Some(tx) = events {
                                let _ = tx
                                    .send(AgentEvent::AssistantThoughtDelta {
                                        text: chunk_text.to_string(),
                                    })
                                    .await;
                            }
                        }
                    }
                    "input_json_delta" => {
                        let index =
                            chunk.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                        if tool_blocks.len() <= index {
                            tool_blocks.resize_with(index + 1, AnthropicToolBlock::default);
                        }
                        if let Some(partial) = delta.get("partial_json").and_then(Value::as_str) {
                            tool_blocks[index].input_json.push_str(partial);
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    Ok(true)
}

fn parse_anthropic_response(response: &Value) -> Result<AgentStep> {
    let Some(content) = response.get("content").and_then(Value::as_array) else {
        return Ok(AgentStep::Stop);
    };

    let mut tool_calls = Vec::new();
    let mut text = String::new();
    for block in content {
        match block
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "text" => {
                text.push_str(
                    block
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                );
            }
            "thinking" | "redacted_thinking" => {}
            "tool_use" => {
                tool_calls.push(ToolCall {
                    id: block
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("tool")
                        .to_string(),
                    name: block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    arguments: block.get("input").cloned().unwrap_or(Value::Null),
                });
            }
            _ => {}
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
