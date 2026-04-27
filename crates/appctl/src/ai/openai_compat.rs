use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::{
    ai::{AgentStep, LlmProvider, Message, ToolCall},
    config::ResolvedProvider,
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
    async fn chat(&self, messages: &[Message], tools: &[ToolDef]) -> Result<AgentStep> {
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
            "tool_choice": "auto"
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
        let status = response.status();
        let body = response.text().await.with_context(|| {
            format!(
                "Failed to read response body from the model at {}.",
                endpoint
            )
        })?;
        if !status.is_success() {
            bail!(
                "Model HTTP API returned status {} ({}). {}",
                status,
                endpoint,
                format_api_error_summary(&body)
            );
        }
        let response: Value = serde_json::from_str(&body).with_context(|| {
            format!(
                "Could not parse JSON from the model at {}: {}",
                endpoint,
                format_api_error_summary(&body)
            )
        })?;

        let message = response.pointer("/choices/0/message").with_context(|| {
            format!(
                "The model at {} returned no assistant message (empty or unexpected layout).",
                endpoint
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
