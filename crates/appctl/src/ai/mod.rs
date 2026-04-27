use std::time::Instant;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::{
    config::{AppConfig, ConfigPaths, ProviderKind, ResolvedProvider},
    events::{AgentEvent, ToolStatus},
    executor::{ExecutionContext, ExecutionRequest, Executor, tool_result_is_error},
    history::HistoryStore,
    schema::Schema,
    term::session_sync_line,
    tool_result_format::format_tool_result_message,
    tools::ToolDef,
};

pub mod anthropic;
pub mod azure_openai;
pub mod google_genai;
pub mod openai_compat;
pub mod vertex;

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

#[derive(Debug, Clone)]
pub struct AgentRunOutcome {
    pub response: Value,
    pub transcript: Vec<Message>,
}

pub fn provider_from_config(resolved: ResolvedProvider) -> Box<dyn LlmProvider> {
    match resolved.kind {
        ProviderKind::Anthropic => Box::new(anthropic::AnthropicProvider::new(resolved)),
        ProviderKind::OpenAiCompatible => {
            Box::new(openai_compat::OpenAiCompatProvider::new(resolved))
        }
        ProviderKind::GoogleGenai => Box::new(google_genai::GoogleGenaiProvider::new(resolved)),
        ProviderKind::Vertex => Box::new(vertex::VertexProvider::new(resolved)),
        ProviderKind::AzureOpenAi => Box::new(azure_openai::AzureOpenAiProvider::new(resolved)),
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
    registry_name: &str,
    provider_name: Option<&str>,
    model_override: Option<&str>,
    prompt: &str,
    prior_messages: &[Message],
    tools: &[ToolDef],
    schema: &Schema,
    exec_context: ExecutionContext,
    events: Option<mpsc::Sender<AgentEvent>>,
) -> Result<AgentRunOutcome> {
    send_agent_event(
        &events,
        AgentEvent::UserPrompt {
            text: prompt.to_string(),
        },
    )
    .await;

    let provider = provider_from_config(config.resolve_provider_with_paths(
        Some(paths),
        provider_name,
        model_override,
    )?);
    let executor = Executor::new(paths)?;
    let history = HistoryStore::open(paths)?;
    let system_content = compose_system_message(config, paths, schema, registry_name);
    let mut messages = build_turn_messages(prior_messages, prompt, &system_content);

    let mut final_response = Value::Null;

    let loop_result: Result<()> = 'agent: {
        for _ in 0..config.behavior.max_iterations {
            trim_transcript(&mut messages, config.behavior.history_limit);
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
                    // One user turn: a plain assistant reply ends this LLM round-trip.
                    // Do not call the model again until the next user message (avoids
                    // duplicate assistant blocks and extra provider calls).
                    break;
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
                        let resolved_name = config.resolve_tool_name(&call.name).to_string();
                        let action = schema
                            .action(&resolved_name)
                            .with_context(|| format!("tool '{}' not found", call.name))?;
                        send_agent_event(&events, AgentEvent::AwaitingInput).await;
                        // Let the printer task clear spinner frames before dialoguer asks
                        // for blocking confirmation on mutating actions.
                        tokio::task::yield_now().await;
                        if let Err(e) = exec_context.safety.check(action, &call.arguments) {
                            send_agent_event(
                                &events,
                                AgentEvent::Error {
                                    message: e.to_string(),
                                },
                            )
                            .await;
                            break 'agent Err(e);
                        }

                        send_agent_event(
                            &events,
                            AgentEvent::ToolCall {
                                id: call.id.clone(),
                                name: call.name.clone(),
                                arguments: call.arguments.clone(),
                            },
                        )
                        .await;

                        let request = ExecutionRequest::new(resolved_name, call.arguments.clone());
                        let start = Instant::now();
                        match executor
                            .execute(schema, exec_context.clone(), request.clone())
                            .await
                        {
                            Ok(result) => {
                                let duration_ms = start.elapsed().as_millis() as u64;
                                let tool_failed = tool_result_is_error(&result.output);
                                history.log(
                                    &exec_context.session_id,
                                    exec_context.session_name.as_deref(),
                                    &request,
                                    &result,
                                    if tool_failed { "error" } else { "ok" },
                                )?;
                                send_agent_event(
                                    &events,
                                    AgentEvent::ToolResult {
                                        id: call.id.clone(),
                                        result: result.output.clone(),
                                        status: if tool_failed {
                                            ToolStatus::Error
                                        } else {
                                            ToolStatus::Ok
                                        },
                                        duration_ms,
                                    },
                                )
                                .await;
                                let tool_content =
                                    format_tool_result_message(&result.output, &config.behavior)
                                        .map_err(|e| {
                                            anyhow::anyhow!("failed to serialize tool output: {e}")
                                        })?;
                                messages.push(Message {
                                    role: "tool".to_string(),
                                    content: tool_content,
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
        Ok(AgentRunOutcome {
            response: final_response,
            transcript: messages,
        })
    }
}

pub fn load_provider(paths: &ConfigPaths) -> Result<AppConfig> {
    AppConfig::load_for_runtime(paths, "run")
}

fn compose_system_message(
    config: &AppConfig,
    paths: &ConfigPaths,
    schema: &Schema,
    registry_name: &str,
) -> String {
    let mut s = String::new();
    s.push_str(&system_prompt());
    s.push_str("\n\n## This app (use this context; do not invent another project)\n");
    s.push_str(&format!(
        "- **Registry name** (what the user types for `appctl app use` / `app list`): `{}`\n",
        registry_name
    ));
    s.push_str(&format!(
        "- **Display label** (chat banner / UI): {}\n",
        config.banner_label(registry_name)
    ));
    if let Some(d) = config
        .description
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        s.push_str(&format!("- **Description**: {}\n", d));
    }
    s.push_str(&format!(
        "- **App directory** (this `.appctl`): {}\n",
        paths.root.display()
    ));
    s.push_str(&format!(
        "- **Tools / schema from**: {}\n",
        session_sync_line(schema)
    ));
    s
}

fn system_prompt() -> String {
    r#"Critical identity: you are only "appctl" (the end-user’s application operations agent). You must not name or imply Gemini, Google, OpenAI, Anthropic, a model name, a vendor, a cloud, or a subscription product. If asked who/what you are, answer exactly: I am appctl, your application operations agent. One short reply; do not add a second self-introduction paragraph.

You help users with the tools synced for this app (see the appctl banner for the sync source). Prefer direct tool use. Never invent parameters.

For HTTP tools, appctl may add Authorization headers and default query parameters from the user’s app configuration (not shown to you in full). Prefer calling the tool; do not ask the user to paste API tokens or secrets if a tool can run with optional parameters that appctl supplies. Only ask for a value when a tool result shows an auth or permission error and the spec requires a parameter the user must supply in chat.

Response style rules:
- Do not volunteer unrelated information the user did not ask for.
- Keep answers concise and task-focused.
- Do not end every response with "let me know..." style filler.
- If a follow-up question is required, ask at most one short follow-up sentence.
- Tool results may include `status`, `classification`, and `summary`. Treat the summary as the best appctl diagnosis.
- Do not infer permissions, admin access, or login state from `405 Method Not Allowed` alone. A 405 can mean wrong HTTP method, route mismatch, or backend policy."#
        .to_string()
}

fn build_turn_messages(
    prior_messages: &[Message],
    prompt: &str,
    system_content: &str,
) -> Vec<Message> {
    let mut messages = if prior_messages.is_empty() {
        vec![Message {
            role: "system".to_string(),
            content: system_content.to_string(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            tool_name: None,
        }]
    } else {
        let mut m = prior_messages.to_vec();
        if let Some(idx) = m.iter().position(|msg| msg.role == "system") {
            m[idx].content = system_content.to_string();
        } else {
            m.insert(
                0,
                Message {
                    role: "system".to_string(),
                    content: system_content.to_string(),
                    tool_calls: Vec::new(),
                    tool_call_id: None,
                    tool_name: None,
                },
            );
        }
        m
    };

    messages.push(Message {
        role: "user".to_string(),
        content: prompt.to_string(),
        tool_calls: Vec::new(),
        tool_call_id: None,
        tool_name: None,
    });
    messages
}

fn trim_transcript(messages: &mut Vec<Message>, history_limit: usize) {
    if history_limit == 0 {
        return;
    }
    let system = messages
        .iter()
        .find(|message| message.role == "system")
        .cloned();
    let non_system: Vec<_> = messages
        .iter()
        .filter(|message| message.role != "system")
        .cloned()
        .collect();
    if non_system.len() <= history_limit {
        return;
    }
    let start = non_system.len().saturating_sub(history_limit);
    let mut trimmed = Vec::with_capacity(history_limit + usize::from(system.is_some()));
    if let Some(system) = system {
        trimmed.push(system);
    }
    trimmed.extend(non_system.into_iter().skip(start));
    *messages = trimmed;
}

#[cfg(test)]
mod tests {
    use super::{Message, build_turn_messages, trim_transcript};

    fn msg(role: &str, content: &str) -> Message {
        Message {
            role: role.to_string(),
            content: content.to_string(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            tool_name: None,
        }
    }

    #[test]
    fn build_turn_messages_keeps_prior_transcript() {
        let prior = vec![
            msg("system", "sys"),
            msg("user", "first"),
            msg("assistant", "reply"),
        ];
        let messages = build_turn_messages(&prior, "second", "full-sys");
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].content, "full-sys");
        assert_eq!(messages[1].content, "first");
        assert_eq!(messages[2].content, "reply");
        assert_eq!(messages[3].content, "second");
    }

    #[test]
    fn trim_transcript_keeps_system_and_latest_messages() {
        let mut messages = vec![
            msg("system", "sys"),
            msg("user", "u1"),
            msg("assistant", "a1"),
            msg("user", "u2"),
            msg("assistant", "a2"),
        ];
        trim_transcript(&mut messages, 2);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].content, "u2");
        assert_eq!(messages[2].content, "a2");
    }
}
