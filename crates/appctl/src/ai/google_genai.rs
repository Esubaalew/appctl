use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};

use crate::{
    ai::{AgentStep, LlmProvider, Message, ToolCall},
    config::ResolvedProvider,
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
    async fn chat(&self, messages: &[Message], tools: &[ToolDef]) -> Result<AgentStep> {
        let mut request = self.client.post(format!(
            "{}/v1beta/models/{}:generateContent",
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
        let status = response.status();
        let body = response
            .text()
            .await
            .context("failed to read Google GenAI response body")?;
        if !status.is_success() {
            bail!(
                "{}",
                user_message_google_genai_http_error(status.as_u16(), &body, &self.config.model)
            );
        }
        let response: Value = serde_json::from_str(&body).with_context(|| {
            format!(
                "failed to parse Google GenAI response as JSON: {}",
                format_api_error_summary(&body)
            )
        })?;

        let Some(candidate) = response
            .pointer("/candidates/0/content/parts")
            .and_then(Value::as_array)
        else {
            let feedback = response
                .get("promptFeedback")
                .map(|value| format_google_error_detail_line(&value.to_string()))
                .unwrap_or_else(|| format_google_error_detail_line(&body));
            bail!(
                "appctl could not read a normal model reply (blocked, safety, or empty output). {}",
                feedback
            );
        };

        let mut tool_calls = Vec::new();
        let mut text = String::new();
        for part in candidate {
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
            let response = serde_json::from_str::<Value>(&message.content)
                .unwrap_or_else(|_| Value::String(message.content.clone()));
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
