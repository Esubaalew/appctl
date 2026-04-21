use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::{
    ai::{AgentStep, LlmProvider, Message, ToolCall},
    config::ResolvedProvider,
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
    async fn chat(&self, messages: &[Message], tools: &[ToolDef]) -> Result<AgentStep> {
        let mut request = self
            .client
            .post(format!(
                "{}/v1/messages",
                self.config.base_url.trim_end_matches('/')
            ))
            .header("anthropic-version", "2023-06-01");
        if let Some(api_key) = &self.config.api_key {
            request = request.header("x-api-key", api_key);
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
            })).collect::<Vec<_>>()
        });

        let response = request
            .json(&payload)
            .send()
            .await
            .context("failed to call Anthropic API")?;
        let status = response.status();
        let body = response
            .text()
            .await
            .context("failed to read Anthropic response body")?;
        if !status.is_success() {
            bail!(
                "Anthropic API returned {}: {}",
                status,
                summarize_body(&body)
            );
        }
        let response: Value = serde_json::from_str(&body).with_context(|| {
            format!(
                "failed to parse Anthropic response as JSON: {}",
                summarize_body(&body)
            )
        })?;

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
