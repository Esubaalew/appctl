use std::time::Instant;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::{
    config::{AppConfig, ConfigPaths, ProviderKind, ResolvedProvider},
    events::{AgentEvent, ToolStatus},
    executor::{ExecutionContext, ExecutionRequest, Executor, tool_result_is_error},
    history::HistoryStore,
    schema::{Action, Field, Resource, Schema, Transport},
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
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        events: Option<mpsc::Sender<AgentEvent>>,
    ) -> Result<AgentStep>;
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
    let mut last_tool_outputs = Vec::<Value>::new();

    let loop_result: Result<()> = 'agent: {
        for _ in 0..config.behavior.max_iterations {
            let trimmed = trim_transcript(&mut messages, config.behavior.history_limit);
            if trimmed > 0 {
                send_agent_event(
                    &events,
                    AgentEvent::ContextNotice {
                        message: format!("Trimmed {trimmed} older message(s) from model context."),
                    },
                )
                .await;
            }
            let step = match provider.chat(&messages, tools, events.clone()).await {
                Ok(step) => step,
                Err(err) if !last_tool_outputs.is_empty() => {
                    let fallback = fallback_response_after_provider_error(&last_tool_outputs, &err);
                    send_agent_event(
                        &events,
                        AgentEvent::ContextNotice {
                            message: format!(
                                "The model failed while summarizing tool results; appctl is showing a local fallback. Detail: {err:#}"
                            ),
                        },
                    )
                    .await;
                    send_agent_event(
                        &events,
                        AgentEvent::AssistantMessage {
                            text: fallback.clone(),
                        },
                    )
                    .await;
                    messages.push(Message {
                        role: "assistant".to_string(),
                        content: fallback.clone(),
                        tool_calls: Vec::new(),
                        tool_call_id: None,
                        tool_name: None,
                    });
                    final_response = Value::String(fallback);
                    break;
                }
                Err(err) => return Err(err),
            };
            match step {
                AgentStep::Message { content } => {
                    let content = guard_assistant_response(prompt, &content);
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
                                last_tool_outputs.push(result.output.clone());
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
        let transcript = compact_transcript_for_storage(messages);
        Ok(AgentRunOutcome {
            response: final_response,
            transcript,
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
    if let Some(provider) = config
        .target
        .oauth_provider
        .as_deref()
        .map(str::trim)
        .filter(|provider| !provider.is_empty())
    {
        s.push_str(&format!(
            "- **Target auth profile**: `{provider}` (stored by appctl; do not ask the user for its token or password)\n"
        ));
    }
    if let Some(hint) = current_user_hint(config, schema) {
        s.push_str(&format!("- **Current target-user lookup**: {hint}\n"));
    }
    s.push_str(&format!(
        "- **Tools / schema from**: {}\n",
        session_sync_line(schema)
    ));
    s.push_str(&compose_tool_guide(schema));
    s
}

fn fallback_response_after_provider_error(tool_outputs: &[Value], err: &anyhow::Error) -> String {
    let successes = tool_outputs
        .iter()
        .filter(|value| value.get("ok").and_then(Value::as_bool) == Some(true))
        .count();
    let failures = tool_outputs.len().saturating_sub(successes);
    let mut lines = Vec::new();
    lines.push(format!(
        "Tool calls completed, but the model failed while writing the final reply. appctl preserved the results locally. ({successes} succeeded, {failures} failed.)"
    ));
    if let Some(last) = tool_outputs.last() {
        if let Some(summary) = last.get("summary").and_then(Value::as_str) {
            lines.push(format!("Last tool result: {summary}"));
        }
        if let Some(data) = last.get("data") {
            let compact = compact_json_preview(data, 1200);
            if !compact.is_empty() {
                lines.push(format!("Last data: {compact}"));
            }
        }
    }
    lines.push(format!("Model error: {err:#}"));
    lines.join("\n")
}

fn compact_json_preview(value: &Value, max_chars: usize) -> String {
    let raw = serde_json::to_string(value).unwrap_or_else(|_| json!(value).to_string());
    if raw.chars().count() <= max_chars {
        return raw;
    }
    let mut out = raw
        .chars()
        .take(max_chars.saturating_sub(18))
        .collect::<String>();
    out.push_str("… [truncated]");
    out
}

fn compact_transcript_for_storage(messages: Vec<Message>) -> Vec<Message> {
    messages
        .into_iter()
        .filter(|message| message.role != "tool" && message.tool_calls.is_empty())
        .collect()
}

fn current_user_hint(config: &AppConfig, schema: &Schema) -> Option<String> {
    if let Some(tool) = config
        .target
        .me_tool
        .as_deref()
        .map(str::trim)
        .filter(|tool| !tool.is_empty())
    {
        return Some(format!(
            "call tool `{tool}` when the user asks who appctl is authenticated as"
        ));
    }
    if let Some(path) = config
        .target
        .me_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        return Some(format!(
            "use the configured current-user HTTP path `{path}` when available"
        ));
    }

    let likely = schema
        .resources
        .iter()
        .flat_map(|resource| resource.actions.iter())
        .find(|action| {
            let name = action.name.to_ascii_lowercase();
            let name_match = name == "me"
                || name == "whoami"
                || name.contains("current_user")
                || name.contains("get_me")
                || name.contains("users_me");
            let path_match = match &action.transport {
                Transport::Http { path, .. } => {
                    matches!(path.as_str(), "/me" | "/users/me" | "/user/me" | "/whoami")
                }
                _ => false,
            };
            name_match || path_match
        })?;
    Some(format!(
        "likely current-user tool `{}` is available; prefer it over guessing from user lists",
        likely.name
    ))
}

fn guard_assistant_response(prompt: &str, content: &str) -> String {
    if should_force_identity_answer(prompt, content) {
        return "I am appctl, your application operations agent.".to_string();
    }
    guard_secret_collection_response(content)
}

fn guard_secret_collection_response(content: &str) -> String {
    if assistant_asks_for_secret(content) {
        return "I cannot collect passwords, bearer tokens, cookies, API keys, or OAuth client secrets in chat. Configure target-app auth outside the conversation, for example with `appctl setup` or `appctl auth target login <name>`, then retry the request. If the app needs an OAuth client id, store it in appctl config or an environment variable rather than sending it to the AI."
            .to_string();
    }
    guard_invalid_auth_command_response(content)
}

fn guard_invalid_auth_command_response(content: &str) -> String {
    let lower = content.to_lowercase();
    if lower.contains("appctl auth task login")
        || lower.contains("appctl auth <app> login")
        || lower.contains("appctl auth <username> login")
    {
        return "Target-app auth must be configured outside chat. Use `appctl setup`, `appctl auth target set-bearer --env API_TOKEN`, `appctl auth target token-login <profile> --url <token-url>`, or `appctl auth target login <profile> --client-id <id> --auth-url <url> --token-url <url>`. Then retry the request."
            .to_string();
    }
    content.to_string()
}

fn should_force_identity_answer(prompt: &str, content: &str) -> bool {
    let prompt = prompt.to_lowercase();
    let asks_identity = prompt.contains("who is this")
        || prompt.contains("who are you")
        || prompt.contains("what are you")
        || prompt.contains("your identity");
    if !asks_identity {
        return false;
    }
    let content = content.to_lowercase();
    content.contains("critical identity")
        || content.contains("system prompt")
        || content.contains("according to the")
}

fn assistant_asks_for_secret(content: &str) -> bool {
    let lower = content.to_lowercase();
    let asks = [
        "provide",
        "paste",
        "send",
        "enter",
        "share",
        "give me",
        "what is",
        "need your",
        "please provide",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    let secret = [
        "password",
        "passcode",
        "client secret",
        "client_secret",
        "api key",
        "apikey",
        "bearer",
        "token",
        "cookie",
        "authorization header",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    asks && secret
}

fn system_prompt() -> String {
    r#"Critical identity: you are only "appctl" (the end-user’s application operations agent). You must not name or imply Gemini, Google, OpenAI, Anthropic, a model name, a vendor, a cloud, or a subscription product. If asked who/what you are, answer exactly: I am appctl, your application operations agent. One short reply; do not add a second self-introduction paragraph.

You help users with the tools synced for this app (see the appctl banner for the sync source). Prefer direct tool use. Never invent parameters.

Operating rules:
- Work step by step like an IDE agent: choose a tool, inspect the result, then decide the next tool call.
- Use returned IDs, foreign keys, URLs, names, and other values from one tool result as inputs to later calls.
- For create/update tools, send the smallest payload that satisfies the user request and required schema fields. Omit optional fields when the user did not provide a value; do not send empty strings as placeholders.
- If a tool returns `validation_error`, inspect the returned detail and retry once with the invalid optional fields removed or corrected. If the fix is not obvious, ask one concise follow-up.
- If a tool returns `server_error`, do not repeatedly guess payload variants. Retry at most once with a smaller payload only if the previous result clearly shows optional fields may have caused the failure; otherwise report the backend failure.
- For database `list_*` tools, do not assume the first page is complete. If a target row is missing, retry with `filter`, then use `offset`/`limit` when needed.
- When the user gives a business identifier instead of a primary key, try likely columns such as `uic`, `old_code`, `code`, `slug`, `name`, `email`, or fields shown in the tool guide.
- To answer relationship questions, follow join-style fields: for example use `parcel_id`, `party_id`, `user_id`, or any `*_id` returned by one tool in a related list/get tool.
- Ask the user for more information only after the available read-only tools cannot find or disambiguate the needed data.
- If a read-only lookup fails, explain the specific tool path tried and the missing key/field; do not simply say the data is unavailable.

For HTTP tools, appctl may add Authorization headers, cookies, sessions, and default query parameters from the user’s app configuration (not shown to you in full). Prefer calling business tools; never ask the user to paste API tokens, passwords, OAuth client secrets, cookies, or bearer strings into chat. If the user asks you to log in as a target-app user, do not ask for their password or client secret; tell them to configure target auth outside chat with `appctl setup`, `appctl auth target set-bearer --env API_TOKEN`, `appctl auth target token-login <profile> --url <token-url>`, or `appctl auth target login <profile> --client-id <id> --auth-url <url> --token-url <url>`. Treat username/password token endpoints (for example `/auth/token`, `/login`, `token_login`) as auth setup endpoints: do not call them from chat unless non-secret credentials are already configured by appctl; recommend `appctl auth target token-login` instead. Never invent commands like `appctl auth task login`. If a tool result returns 401/403, say that the target app auth configured in appctl was missing, expired, rejected, or lacks permission, and tell the user to fix appctl target auth/config outside chat using one of those valid commands. Only ask for ordinary non-secret business inputs (project name, task title, record id, date range, etc.).

Response style rules:
- Do not volunteer unrelated information the user did not ask for.
- Keep answers concise and task-focused.
- Do not end every response with "let me know..." style filler.
- If a follow-up question is required, ask at most one short follow-up sentence.
- Tool results may include `status`, `classification`, and `summary`. Treat the summary as the best appctl diagnosis.
- Do not infer permissions, admin access, or login state from `405 Method Not Allowed` alone. A 405 can mean wrong HTTP method, route mismatch, or backend policy."#
        .to_string()
}

fn compose_tool_guide(schema: &Schema) -> String {
    if schema.resources.is_empty() {
        return String::new();
    }

    const MAX_RESOURCES: usize = 12;
    const MAX_FIELDS: usize = 12;
    const MAX_ACTIONS: usize = 8;

    let mut out = String::from(
        "\n## Tool guide\nUse this compact catalog to choose tools and chain values. Tool schemas remain the source of truth for exact parameter names.\n",
    );

    for resource in schema.resources.iter().take(MAX_RESOURCES) {
        out.push_str(&format!("- Resource `{}`", resource.name));
        if let Some(description) = resource
            .description
            .as_deref()
            .filter(|d| !d.trim().is_empty())
        {
            out.push_str(&format!(" ({})", description.trim()));
        }
        out.push('\n');

        let fields = summarize_fields(&resource.fields, MAX_FIELDS);
        if !fields.is_empty() {
            out.push_str(&format!("  - Fields: {fields}\n"));
        }

        let join_fields = summarize_join_fields(&resource.fields);
        if !join_fields.is_empty() {
            out.push_str(&format!(
                "  - Chain these ID/relationship fields into related tools: {join_fields}\n"
            ));
        }

        let actions = summarize_actions(&resource.actions, MAX_ACTIONS);
        if !actions.is_empty() {
            out.push_str(&format!("  - Actions: {actions}\n"));
        }

        for action in resource
            .actions
            .iter()
            .filter(|action| is_filterable_sql_list(action))
            .take(2)
        {
            let candidate_columns = candidate_filter_columns(resource);
            out.push_str(&format!(
                "  - `{}` supports `filter` for exact column matches",
                action.name
            ));
            if !candidate_columns.is_empty() {
                out.push_str(&format!(" such as {candidate_columns}"));
            }
            out.push_str("; use `limit` and `offset` to page.\n");
        }
    }

    if schema.resources.len() > MAX_RESOURCES {
        out.push_str(&format!(
            "- appctl: {} more resource(s) are available through the tool list.\n",
            schema.resources.len() - MAX_RESOURCES
        ));
    }

    out
}

fn summarize_fields(fields: &[Field], max: usize) -> String {
    let mut names: Vec<String> = fields
        .iter()
        .take(max)
        .map(|field| format!("`{}`", field.name))
        .collect();
    if fields.len() > max {
        names.push(format!("... +{}", fields.len() - max));
    }
    names.join(", ")
}

fn summarize_join_fields(fields: &[Field]) -> String {
    fields
        .iter()
        .filter(|field| field.name == "id" || field.name.ends_with("_id"))
        .take(10)
        .map(|field| format!("`{}`", field.name))
        .collect::<Vec<_>>()
        .join(", ")
}

fn summarize_actions(actions: &[Action], max: usize) -> String {
    let mut names: Vec<String> = actions
        .iter()
        .take(max)
        .map(|action| format!("`{}`", action.name))
        .collect();
    if actions.len() > max {
        names.push(format!("... +{}", actions.len() - max));
    }
    names.join(", ")
}

fn is_filterable_sql_list(action: &Action) -> bool {
    matches!(
        &action.transport,
        Transport::Sql {
            operation: crate::schema::SqlOperation::Select,
            ..
        }
    ) && action.parameters.iter().any(|field| field.name == "filter")
}

fn candidate_filter_columns(resource: &Resource) -> String {
    let preferred = [
        "uic",
        "old_code",
        "code",
        "slug",
        "name",
        "email",
        "id",
        "parcel_id",
        "party_id",
        "user_id",
        "owner_id",
    ];
    let mut names = Vec::<String>::new();
    for wanted in preferred {
        if resource.fields.iter().any(|field| field.name == wanted) {
            names.push(format!("`{wanted}`"));
        }
    }
    for field in &resource.fields {
        if names.len() >= 8 {
            break;
        }
        if field.name.ends_with("_id") {
            let quoted = format!("`{}`", field.name);
            if !names.contains(&quoted) {
                names.push(quoted);
            }
        }
    }
    names.join(", ")
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

fn trim_transcript(messages: &mut Vec<Message>, history_limit: usize) -> usize {
    if history_limit == 0 {
        return 0;
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
        return 0;
    }
    let start = non_system.len().saturating_sub(history_limit);
    let removed = start;
    let mut trimmed = Vec::with_capacity(history_limit + usize::from(system.is_some()));
    if let Some(system) = system {
        trimmed.push(system);
    }
    trimmed.extend(non_system.into_iter().skip(start));
    *messages = trimmed;
    removed
}

#[cfg(test)]
mod tests {
    use super::{
        Message, ToolCall, assistant_asks_for_secret, build_turn_messages,
        compact_transcript_for_storage, compose_tool_guide, current_user_hint,
        fallback_response_after_provider_error, guard_assistant_response,
        guard_secret_collection_response, system_prompt, trim_transcript,
    };
    use crate::config::AppConfig;
    use crate::schema::{
        Action, AuthStrategy, DatabaseKind, Field, FieldType, ParameterLocation, Provenance,
        Resource, Safety, Schema, SqlOperation, SyncSource, Transport, Verb,
    };
    use serde_json::json;

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

    #[test]
    fn system_prompt_teaches_iterative_tool_use() {
        let prompt = system_prompt();
        assert!(prompt.contains("Work step by step like an IDE agent"));
        assert!(prompt.contains("Use returned IDs"));
        assert!(prompt.contains("send the smallest payload"));
        assert!(prompt.contains("do not send empty strings as placeholders"));
        assert!(prompt.contains("Retry at most once"));
        assert!(prompt.contains("retry with `filter`"));
        assert!(prompt.contains("never ask the user to paste API tokens"));
        assert!(prompt.contains("If the user asks you to log in as a target-app user"));
        assert!(prompt.contains("fix appctl target auth/config outside chat"));
    }

    #[test]
    fn secret_collection_guard_blocks_password_and_client_secret_request() {
        let bad = "Please provide esubalew's password (and the client ID/secret if required).";

        assert!(assistant_asks_for_secret(bad));
        let safe = guard_secret_collection_response(bad);

        assert!(!safe.contains("Please provide esubalew"));
        assert!(safe.contains("I cannot collect passwords"));
        assert!(safe.contains("appctl auth target login"));
    }

    #[test]
    fn auth_command_guard_blocks_invented_app_command() {
        let bad = "Configure a valid login with `appctl auth task login <username>`.";

        let safe = guard_secret_collection_response(bad);

        assert!(!safe.contains("appctl auth task login"));
        assert!(safe.contains("appctl auth target set-bearer"));
        assert!(safe.contains("appctl auth target token-login"));
        assert!(safe.contains("appctl auth target login"));
    }

    #[test]
    fn secret_collection_guard_allows_normal_business_question() {
        let ok = "Which project name should I use?";

        assert!(!assistant_asks_for_secret(ok));
        assert_eq!(guard_secret_collection_response(ok), ok);
    }

    #[test]
    fn identity_guard_blocks_system_prompt_leak() {
        let leaked = "According to the Critical identity section, I should answer exactly: I am appctl, your application operations agent.";

        assert_eq!(
            guard_assistant_response("who are you?", leaked),
            "I am appctl, your application operations agent."
        );
    }

    #[test]
    fn current_user_hint_prefers_explicit_me_tool() {
        let mut config = AppConfig::default();
        config.target.me_tool = Some("get_current_user".to_string());

        let schema = Schema {
            source: SyncSource::Openapi,
            base_url: None,
            auth: AuthStrategy::None,
            resources: vec![],
            metadata: serde_json::Map::new(),
        };
        let hint = current_user_hint(&config, &schema).unwrap();

        assert!(hint.contains("get_current_user"));
    }

    #[test]
    fn fallback_response_preserves_completed_tool_result_context() {
        let outputs = vec![json!({
            "ok": true,
            "summary": "created task #42",
            "data": {
                "id": 42,
                "title": "Ship fallback",
            }
        })];

        let err = anyhow::anyhow!("Model HTTP API returned status 500 Internal Server Error");
        let response = fallback_response_after_provider_error(&outputs, &err);

        assert!(response.contains("Tool calls completed"));
        assert!(response.contains("1 succeeded, 0 failed"));
        assert!(response.contains("created task #42"));
        assert!(response.contains("\"id\":42"));
        assert!(response.contains("Model HTTP API returned status 500"));
    }

    #[test]
    fn compact_transcript_for_storage_drops_stale_tool_protocol() {
        let mut assistant_tool_call = msg("assistant", "");
        assistant_tool_call.tool_calls = vec![ToolCall {
            id: "call_1".to_string(),
            name: "list_projects".to_string(),
            arguments: json!({}),
        }];

        let mut tool_message = msg("tool", "{\"ok\":true}");
        tool_message.tool_call_id = Some("call_1".to_string());
        tool_message.tool_name = Some("list_projects".to_string());

        let compacted = compact_transcript_for_storage(vec![
            msg("system", "sys"),
            msg("user", "how many?"),
            assistant_tool_call,
            tool_message,
            msg("assistant", "There are 3 projects."),
        ]);

        assert_eq!(compacted.len(), 3);
        assert_eq!(compacted[0].role, "system");
        assert_eq!(compacted[1].role, "user");
        assert_eq!(compacted[2].content, "There are 3 projects.");
        assert!(compacted.iter().all(|message| message.role != "tool"));
        assert!(
            compacted
                .iter()
                .all(|message| message.tool_calls.is_empty())
        );
    }

    #[test]
    fn tool_guide_summarizes_filterable_db_resources() {
        let schema = Schema {
            source: SyncSource::Db,
            base_url: None,
            auth: AuthStrategy::None,
            resources: vec![Resource {
                name: "cis_core__land_record".to_string(),
                description: Some("Table cis_core.land_record".to_string()),
                fields: vec![
                    field("id", FieldType::Uuid),
                    field("parcel_id", FieldType::Uuid),
                    field("uic", FieldType::String),
                    field("old_code", FieldType::String),
                ],
                actions: vec![Action {
                    name: "list_cis_core__land_record".to_string(),
                    description: Some("List rows".to_string()),
                    verb: Verb::List,
                    transport: Transport::Sql {
                        database_kind: DatabaseKind::Postgres,
                        schema: Some("cis_core".to_string()),
                        table: "land_record".to_string(),
                        operation: SqlOperation::Select,
                        primary_key: Some("id".to_string()),
                    },
                    parameters: vec![Field {
                        name: "filter".to_string(),
                        description: None,
                        field_type: FieldType::Object,
                        required: false,
                        location: Some(ParameterLocation::Body),
                        default: None,
                        enum_values: vec![],
                    }],
                    safety: Safety::ReadOnly,
                    resource: Some("cis_core__land_record".to_string()),
                    provenance: Provenance::Declared,
                    metadata: Default::default(),
                }],
                metadata: Default::default(),
            }],
            metadata: Default::default(),
        };

        let guide = compose_tool_guide(&schema);
        assert!(guide.contains("Resource `cis_core__land_record`"));
        assert!(guide.contains("`parcel_id`"));
        assert!(guide.contains("supports `filter`"));
        assert!(guide.contains("`old_code`"));
    }

    fn field(name: &str, field_type: FieldType) -> Field {
        Field {
            name: name.to_string(),
            description: None,
            field_type,
            required: false,
            location: Some(ParameterLocation::Body),
            default: None,
            enum_values: vec![],
        }
    }
}
