use anyhow::{Context, Result};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::{
    ai::{AgentStep, LlmProvider, Message, openai_compat::parse_openai_stream_response},
    config::ResolvedProvider,
    events::AgentEvent,
    tools::ToolDef,
};

pub struct AzureOpenAiProvider {
    client: reqwest::Client,
    config: ResolvedProvider,
}

impl AzureOpenAiProvider {
    pub fn new(config: ResolvedProvider) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for AzureOpenAiProvider {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        events: Option<mpsc::Sender<AgentEvent>>,
    ) -> Result<AgentStep> {
        let mut request = self.client.post(format!(
            "{}/openai/deployments/{}/chat/completions?api-version=2024-10-21",
            self.config.base_url.trim_end_matches('/'),
            self.config.model
        ));
        if let Some(api_key) = self.config.auth.api_key() {
            request = request.header("api-key", api_key);
        }
        if let Some(token) = self.config.auth.bearer_token() {
            request = request.bearer_auth(token);
        }
        for (name, value) in &self.config.extra_headers {
            request = request.header(name, value);
        }

        let payload = json!({
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

        let response = request
            .json(&payload)
            .send()
            .await
            .context("failed to call Azure OpenAI")?;
        parse_openai_stream_response(response, "Azure OpenAI", events).await
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
