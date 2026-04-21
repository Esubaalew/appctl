use anyhow::Result;
use rustyline::{DefaultEditor, error::ReadlineError};
use uuid::Uuid;

use crate::{
    ai::run_agent,
    config::{AppConfig, ConfigPaths},
    executor::ExecutionContext,
    safety::SafetyMode,
    sync::{load_schema, load_tools},
};

#[derive(Debug, Clone)]
pub struct ChatOptions {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub read_only: bool,
    pub dry_run: bool,
    pub confirm: bool,
}

pub async fn run_chat(
    paths: &ConfigPaths,
    config: &AppConfig,
    mut options: ChatOptions,
) -> Result<()> {
    let schema = load_schema(paths)?;
    let tools = load_tools(paths)?;
    let mut editor = DefaultEditor::new()?;
    let session_id = Uuid::new_v4().to_string();

    loop {
        let prompt = format!(
            "appctl[{}]> ",
            options
                .provider
                .clone()
                .unwrap_or_else(|| config.default.clone())
        );
        match editor.readline(&prompt) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                editor.add_history_entry(line)?;
                if handle_slash_command(line, &mut options) {
                    if line == "/exit" {
                        break;
                    }
                    continue;
                }

                let response = run_agent(
                    paths,
                    config,
                    options.provider.as_deref(),
                    options.model.as_deref(),
                    line,
                    &tools,
                    &schema,
                    ExecutionContext {
                        session_id: session_id.clone(),
                        safety: SafetyMode {
                            read_only: options.read_only,
                            dry_run: options.dry_run,
                            confirm: options.confirm,
                        },
                    },
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&response)?);
            }
            Err(ReadlineError::Interrupted | ReadlineError::Eof) => break,
            Err(err) => return Err(err.into()),
        }
    }

    Ok(())
}

fn handle_slash_command(line: &str, options: &mut ChatOptions) -> bool {
    match line {
        "/exit" | "/quit" => true,
        "/read-only on" => {
            options.read_only = true;
            println!("read-only mode enabled");
            true
        }
        "/read-only off" => {
            options.read_only = false;
            println!("read-only mode disabled");
            true
        }
        "/dry-run on" => {
            options.dry_run = true;
            println!("dry-run mode enabled");
            true
        }
        "/dry-run off" => {
            options.dry_run = false;
            println!("dry-run mode disabled");
            true
        }
        _ if line.starts_with("/provider ") => {
            options.provider = Some(line.trim_start_matches("/provider ").trim().to_string());
            println!("provider set");
            true
        }
        _ if line.starts_with("/model ") => {
            options.model = Some(line.trim_start_matches("/model ").trim().to_string());
            println!("model override set");
            true
        }
        _ => false,
    }
}
