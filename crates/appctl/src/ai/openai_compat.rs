use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::{
    ai::{AgentStep, LlmProvider, Message, ToolCall},
    config::ResolvedProvider,
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
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiCompatProvider {
    async fn chat(&self, messages: &[Message], tools: &[ToolDef]) -> Result<AgentStep> {
        let mut request = self.client.post(format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        ));
        if let Some(api_key) = &self.config.api_key {
            request = request.bearer_auth(api_key);
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
            "tool_choice": "auto"
        });

        let response = request
            .json(&payload)
            .send()
            .await
            .context("failed to call OpenAI-compatible API")?;
        let status = response.status();
        let body = response
            .text()
            .await
            .context("failed to read OpenAI-compatible response body")?;
        if !status.is_success() {
            bail!(
                "OpenAI-compatible API returned {}: {}",
                status,
                summarize_body(&body)
            );
        }
        let response: Value = serde_json::from_str(&body).with_context(|| {
            format!(
                "failed to parse OpenAI-compatible response as JSON: {}",
                summarize_body(&body)
            )
        })?;

        let message = response
            .pointer("/choices/0/message")
            .context("OpenAI-compatible response missing choices[0].message")?;

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
}

fn summarize_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "<empty body>".to_string();
    }

    let mut compact = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.truncate(280);
    compact
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
