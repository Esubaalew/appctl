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

pub async fn run_once(paths: &ConfigPaths, config: &AppConfig, options: RunOptions) -> Result<()> {
    let schema = load_schema(paths)?;
    let tools = load_tools(paths)?;
    let (tx, rx) = mpsc::channel(64);
    let printer = tokio::spawn(crate::term::run_event_printer(rx));
    let response = run_agent(
        paths,
        config,
        options.provider.as_deref(),
        options.model.as_deref(),
        &options.prompt,
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
    let response = response?;

    if !matches!(response, serde_json::Value::String(_)) {
        println!("{}", serde_json::to_string_pretty(&response)?);
    }
    Ok(())
}
