use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::sleep;
use url::Url;

use crate::{
    ai::{AgentStep, LlmProvider, Message, google_genai::parse_google_stream_response},
    config::ResolvedProvider,
    events::AgentEvent,
    term::format_api_error_summary,
    tools::ToolDef,
};

pub struct VertexProvider {
    client: reqwest::Client,
    config: ResolvedProvider,
}

impl VertexProvider {
    pub fn new(config: ResolvedProvider) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }

    fn region(&self) -> String {
        if let Some(region) = self.config.extra_headers.get("x-appctl-vertex-region") {
            return region.clone();
        }

        Url::parse(&self.config.base_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            .and_then(|host| host.split('.').next().map(str::to_string))
            .filter(|part| !part.is_empty())
            .unwrap_or_else(|| "us-central1".to_string())
    }

    fn project_id(&self) -> Result<String> {
        self.config.auth_status.project_id.clone().context(
            "Vertex requires a Google Cloud project. Run `appctl auth provider login vertex`.",
        )
    }

    fn build_request(
        &self,
        access_token: &str,
        project_id: &str,
        region: &str,
        payload: &Value,
    ) -> reqwest::RequestBuilder {
        let mut request = self.client.post(format!(
            "{}/v1/projects/{}/locations/{}/publishers/google/models/{}:streamGenerateContent?alt=sse",
            self.config.base_url.trim_end_matches('/'),
            project_id,
            region,
            self.config.model
        ));
        request = request.bearer_auth(access_token);
        for (name, value) in &self.config.extra_headers {
            if name != "x-appctl-vertex-region" {
                request = request.header(name, value);
            }
        }
        request.json(payload)
    }
}

#[async_trait::async_trait]
impl LlmProvider for VertexProvider {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        events: Option<mpsc::Sender<AgentEvent>>,
    ) -> Result<AgentStep> {
        let access_token = self
            .config
            .auth
            .bearer_token()
            .context("Vertex requires Google ADC or another bearer token source")?;
        let region = self.region();
        let project_id = self.project_id()?;

        let payload = json!({
            "systemInstruction": system_instruction(messages),
            "contents": messages.iter().filter(|message| message.role != "system").map(serialize_message).collect::<Vec<_>>(),
            "tools": [
                {
                    "functionDeclarations": tools.iter().map(serialize_tool).collect::<Vec<_>>()
                }
            ]
        });

        let response =
            send_stream_with_backoff(self, access_token, &project_id, &region, &payload).await?;
        parse_google_stream_response(response, "Vertex AI", None, events).await
    }
}

fn parse_retry_after_seconds(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
}

async fn send_stream_with_backoff(
    provider: &VertexProvider,
    access_token: &str,
    project_id: &str,
    region: &str,
    payload: &Value,
) -> Result<reqwest::Response> {
    let max_retries = 3usize;
    let mut last_summary = String::new();

    for attempt in 0..=max_retries {
        let response = provider
            .build_request(access_token, project_id, region, payload)
            .send()
            .await
            .context("failed to call Vertex AI")?;
        let status = response.status();
        let headers = response.headers().clone();
        if status.is_success() {
            return Ok(response);
        }
        let body = response
            .text()
            .await
            .context("failed to read Vertex AI response body")?;

        let summary = format_api_error_summary(&body);
        if status.as_u16() != 429 {
            bail!("Vertex AI returned {}: {}", status, summary);
        }

        last_summary = summary;
        if attempt == max_retries {
            bail!(
                "Vertex AI returned 429 Too Many Requests after {} retries: {}. Try again shortly, switch to a lower-quota model like `gemini-1.5-flash`, or configure another provider for this app dir.",
                max_retries,
                last_summary
            );
        }

        let wait_secs = parse_retry_after_seconds(&headers).unwrap_or(1u64 << (attempt + 1));
        sleep(Duration::from_secs(wait_secs)).await;
    }

    bail!("Vertex AI rate-limited the request: {}", last_summary)
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
        "parameters": tool.input_schema,
    })
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
