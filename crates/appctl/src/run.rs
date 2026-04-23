use anyhow::Result;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{
    ai::run_agent,
    config::{AppConfig, ConfigPaths},
    executor::ExecutionContext,
    safety::SafetyMode,
    sync::{load_schema, load_tools},
};

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub prompt: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub read_only: bool,
    pub dry_run: bool,
    pub confirm: bool,
    pub strict: bool,
}

pub async fn run_once(
    paths: &ConfigPaths,
    config: &AppConfig,
    app_name: &str,
    options: RunOptions,
) -> Result<()> {
    let schema = load_schema(paths)?;
    let tools = load_tools(paths)?;
    let context = crate::term::chat_context(app_name, &config.default, options.provider.as_deref());
    crate::term::print_chat_banner(&context, &paths.root, schema.resources.len(), tools.len());
    let (tx, rx) = mpsc::channel(64);
    let printer = tokio::spawn(crate::term::run_event_printer(rx));
    let response = run_agent(
        paths,
        config,
        options.provider.as_deref(),
        options.model.as_deref(),
        &options.prompt,
        &[],
        &tools,
        &schema,
        ExecutionContext {
            session_id: Uuid::new_v4().to_string(),
            safety: SafetyMode {
                read_only: options.read_only,
                dry_run: options.dry_run,
                confirm: options.confirm,
                strict: options.strict,
            },
        },
        Some(tx),
    )
    .await;
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
