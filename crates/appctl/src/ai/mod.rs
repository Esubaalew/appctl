use std::time::Instant;

use anyhow::{Result, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::{
    config::{AppConfig, ConfigPaths, ProviderKind, ResolvedProvider},
    events::{AgentEvent, ToolStatus},
    executor::{ExecutionContext, ExecutionRequest, Executor},
    history::HistoryStore,
    tools::ToolDef,
};

pub mod anthropic;
pub mod google_genai;
pub mod openai_compat;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentStep {
    Message { content: String },
    ToolCalls { calls: Vec<ToolCall> },
    Stop,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, messages: &[Message], tools: &[ToolDef]) -> Result<AgentStep>;
}

pub fn provider_from_config(resolved: ResolvedProvider) -> Box<dyn LlmProvider> {
    match resolved.kind {
        ProviderKind::Anthropic => Box::new(anthropic::AnthropicProvider::new(resolved)),
        ProviderKind::OpenAiCompatible => {
            Box::new(openai_compat::OpenAiCompatProvider::new(resolved))
        }
        ProviderKind::GoogleGenai => Box::new(google_genai::GoogleGenaiProvider::new(resolved)),
    }
}

async fn send_agent_event(tx: &Option<mpsc::Sender<AgentEvent>>, ev: AgentEvent) {
    if let Some(t) = tx {
        let _ = t.send(ev).await;
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run_agent(
    paths: &ConfigPaths,
    config: &AppConfig,
    provider_name: Option<&str>,
    model_override: Option<&str>,
    prompt: &str,
    tools: &[ToolDef],
    schema: &crate::schema::Schema,
    exec_context: ExecutionContext,
    events: Option<mpsc::Sender<AgentEvent>>,
) -> Result<Value> {
    send_agent_event(
        &events,
        AgentEvent::UserPrompt {
            text: prompt.to_string(),
        },
    )
    .await;

    let provider =
        provider_from_config(config.resolve_provider_with_paths(Some(paths), provider_name, model_override)?);
    let executor = Executor::new(paths)?;
    let history = HistoryStore::open(paths)?;
    let mut messages = vec![
        Message {
            role: "system".to_string(),
            content: system_prompt(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            tool_name: None,
        },
        Message {
            role: "user".to_string(),
            content: prompt.to_string(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            tool_name: None,
        },
    ];

    let mut final_response = Value::Null;

    let loop_result: Result<()> = 'agent: {
        for _ in 0..config.behavior.max_iterations {
            match provider.chat(&messages, tools).await? {
                AgentStep::Message { content } => {
                    final_response = Value::String(content.clone());
                    send_agent_event(
                        &events,
                        AgentEvent::AssistantMessage {
                            text: content.clone(),
                        },
                    )
                    .await;
                    messages.push(Message {
                        role: "assistant".to_string(),
                        content,
                        tool_calls: Vec::new(),
                        tool_call_id: None,
                        tool_name: None,
                    });
                }
                AgentStep::ToolCalls { calls } => {
                    messages.push(Message {
                        role: "assistant".to_string(),
                        content: String::new(),
                        tool_calls: calls.clone(),
                        tool_call_id: None,
                        tool_name: None,
                    });

                    for call in calls {
                        send_agent_event(
                            &events,
                            AgentEvent::ToolCall {
                                id: call.id.clone(),
                                name: call.name.clone(),
                                arguments: call.arguments.clone(),
                            },
                        )
                        .await;

                        let request =
                            ExecutionRequest::new(call.name.clone(), call.arguments.clone());
                        let start = Instant::now();
                        match executor
                            .execute(schema, exec_context.clone(), request.clone())
                            .await
                        {
                            Ok(result) => {
                                let duration_ms = start.elapsed().as_millis() as u64;
                                history.log(&exec_context.session_id, &request, &result, "ok")?;
                                send_agent_event(
                                    &events,
                                    AgentEvent::ToolResult {
                                        id: call.id.clone(),
                                        result: result.output.clone(),
                                        status: ToolStatus::Ok,
                                        duration_ms,
                                    },
                                )
                                .await;
                                messages.push(Message {
                                    role: "tool".to_string(),
                                    content: serde_json::to_string(&result.output)
                                        .map_err(|e| anyhow::anyhow!(e))?,
                                    tool_calls: Vec::new(),
                                    tool_call_id: Some(call.id),
                                    tool_name: Some(call.name),
                                });
                                final_response = result.output;
                            }
                            Err(e) => {
                                let duration_ms = start.elapsed().as_millis() as u64;
                                send_agent_event(
                                    &events,
                                    AgentEvent::ToolResult {
                                        id: call.id.clone(),
                                        result: Value::String(e.to_string()),
                                        status: ToolStatus::Error,
                                        duration_ms,
                                    },
                                )
                                .await;
                                break 'agent Err(e);
                            }
                        }
                    }
                }
                AgentStep::Stop => break,
            }
        }
        Ok(())
    };

    send_agent_event(&events, AgentEvent::Done).await;

    loop_result?;

    if final_response.is_null() {
        bail!("agent finished without a response")
    } else {
        Ok(final_response)
    }
}

pub fn load_provider(paths: &ConfigPaths) -> Result<AppConfig> {
    AppConfig::load_or_init(paths)
}

fn system_prompt() -> String {
    "You are appctl, an operations agent for a synced application. Prefer direct tool use. Never invent parameters. Summarize your result succinctly after using tools.".to_string()
}
