use anyhow::{Result, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    config::{AppConfig, ConfigPaths, ProviderKind, ResolvedProvider},
    executor::{ExecutionContext, ExecutionRequest, Executor},
    history::HistoryStore,
    tools::ToolDef,
};

pub mod anthropic;
pub mod openai_compat;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
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
) -> Result<Value> {
    let provider = provider_from_config(config.resolve_provider(provider_name, model_override)?);
    let executor = Executor::new(paths)?;
    let history = HistoryStore::open(paths)?;
    let mut messages = vec![
        Message {
            role: "system".to_string(),
            content: system_prompt(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        },
        Message {
            role: "user".to_string(),
            content: prompt.to_string(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        },
    ];

    let mut final_response = Value::Null;

    for _ in 0..config.behavior.max_iterations {
        match provider.chat(&messages, tools).await? {
            AgentStep::Message { content } => {
                final_response = Value::String(content.clone());
                messages.push(Message {
                    role: "assistant".to_string(),
                    content,
                    tool_calls: Vec::new(),
                    tool_call_id: None,
                });
            }
            AgentStep::ToolCalls { calls } => {
                messages.push(Message {
                    role: "assistant".to_string(),
                    content: String::new(),
                    tool_calls: calls.clone(),
                    tool_call_id: None,
                });

                for call in calls {
                    let request = ExecutionRequest::new(call.name.clone(), call.arguments.clone());
                    let result = executor
                        .execute(schema, exec_context.clone(), request.clone())
                        .await?;
                    history.log(&exec_context.session_id, &request, &result, "ok")?;
                    messages.push(Message {
                        role: "tool".to_string(),
                        content: serde_json::to_string(&result.output)?,
                        tool_calls: Vec::new(),
                        tool_call_id: Some(call.id),
                    });
                    final_response = result.output;
                }
            }
            AgentStep::Stop => break,
        }
    }

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
