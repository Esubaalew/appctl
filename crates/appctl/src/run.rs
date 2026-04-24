use std::collections::BTreeMap;

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{
    ai::run_agent,
    config::{AppConfig, ConfigPaths},
    events::{AgentEvent, ToolStatus},
    executor::ExecutionContext,
    safety::SafetyMode,
    sync::{load_runtime_tools, load_schema},
};

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub prompt: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub json: bool,
    pub read_only: bool,
    pub dry_run: bool,
    pub confirm: bool,
    pub strict: bool,
    /// Shown under the app directory in the session banner.
    pub context_note: Option<String>,
}

#[derive(Debug, Serialize)]
struct RunJsonOutput {
    answer: Value,
    session_id: String,
    tools_called: Vec<ToolCallRecord>,
    events: Vec<AgentEvent>,
}

#[derive(Debug, Serialize)]
struct ToolCallRecord {
    name: String,
    arguments: Value,
    result: Option<Value>,
    status: String,
    duration_ms: Option<u64>,
}

pub async fn run_once(
    paths: &ConfigPaths,
    config: &AppConfig,
    app_name: &str,
    options: RunOptions,
) -> Result<()> {
    let schema = load_schema(paths)?;
    let tools = load_runtime_tools(paths, config)?;
    let session_id = Uuid::new_v4().to_string();
    if !options.json {
        let label = config.banner_label(app_name);
        let context =
            crate::term::chat_context(label, &config.default, options.provider.as_deref());
        crate::term::print_chat_banner(&crate::term::ChatBannerInfo {
            context: &context,
            registry_name: app_name,
            app_dir: &paths.root,
            schema: &schema,
            resource_count: schema.resources.len(),
            tool_count: tools.len(),
            context_note: options.context_note.as_deref(),
            app_description: config.description.as_deref(),
        });
    }
    let (tx, mut rx) = mpsc::channel(64);
    let response = run_agent(
        paths,
        config,
        app_name,
        options.provider.as_deref(),
        options.model.as_deref(),
        &options.prompt,
        &[],
        &tools,
        &schema,
        ExecutionContext {
            session_id: session_id.clone(),
            session_name: None,
            safety: SafetyMode {
                read_only: options.read_only,
                dry_run: options.dry_run,
                confirm: options.confirm,
                strict: options.strict,
            },
        },
        Some(tx),
    );
    let mut events = Vec::new();
    if options.json {
        let response = response.await?;
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        let response = response.response;
        let payload = RunJsonOutput {
            tools_called: collect_tool_calls(&events),
            events,
            answer: response,
            session_id,
        };
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    let printer = tokio::spawn(crate::term::run_event_printer(rx));
    let response = response.await;
    let _ = printer.await;
    let response = response?.response;

    if let Some(text) = response.as_str()
        && text.contains('?')
    {
        crate::term::print_tip(
            "`appctl run` is one-shot — use `appctl chat` for follow-up conversation.",
        );
    }

    if !matches!(response, serde_json::Value::String(_)) {
        crate::term::print_json_pretty_value(&response);
    }
    Ok(())
}

fn collect_tool_calls(events: &[AgentEvent]) -> Vec<ToolCallRecord> {
    let mut calls = BTreeMap::<String, ToolCallRecord>::new();
    for event in events {
        match event {
            AgentEvent::ToolCall {
                id,
                name,
                arguments,
            } => {
                calls.insert(
                    id.clone(),
                    ToolCallRecord {
                        name: name.clone(),
                        arguments: arguments.clone(),
                        result: None,
                        status: "pending".to_string(),
                        duration_ms: None,
                    },
                );
            }
            AgentEvent::ToolResult {
                id,
                result,
                status,
                duration_ms,
            } => {
                if let Some(call) = calls.get_mut(id) {
                    call.result = Some(result.clone());
                    call.status = match status {
                        ToolStatus::Ok => "ok".to_string(),
                        ToolStatus::Error => "error".to_string(),
                    };
                    call.duration_ms = Some(*duration_ms);
                }
            }
            _ => {}
        }
    }
    calls.into_values().collect()
}
