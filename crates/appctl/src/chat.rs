use anyhow::Result;
use rustyline::{Editor, error::ReadlineError, history::DefaultHistory};
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
pub struct ChatOptions {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub read_only: bool,
    pub dry_run: bool,
    pub confirm: bool,
    pub strict: bool,
}

pub async fn run_chat(
    paths: &ConfigPaths,
    config: &AppConfig,
    app_name: &str,
    mut options: ChatOptions,
) -> Result<()> {
    let schema = load_schema(paths)?;
    let tools = load_tools(paths)?;
    let context = crate::term::chat_context(app_name, &config.default, options.provider.as_deref());
    crate::term::print_chat_banner(&context, &paths.root, schema.resources.len(), tools.len());
    let mut editor = Editor::<crate::term::PromptHelper, DefaultHistory>::new()?;
    editor.set_helper(Some(crate::term::PromptHelper::new(context)));
    let session_id = Uuid::new_v4().to_string();
    let mut transcript = Vec::new();

    loop {
        let prompt = editor
            .helper()
            .map(|helper| helper.plain_prompt())
            .unwrap_or_else(|| "appctl▶ ".to_string());
        match editor.readline(&prompt) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                editor.add_history_entry(line)?;
                if handle_slash_command(line, &mut options) {
                    refresh_prompt_helper(
                        &mut editor,
                        app_name,
                        &config.default,
                        options.provider.as_deref(),
                    );
                    if line == "/exit" {
                        break;
                    }
                    continue;
                }

                let (tx, rx) = mpsc::channel(64);
                let printer = tokio::spawn(crate::term::run_event_printer(rx));
                let response = run_agent(
                    paths,
                    config,
                    options.provider.as_deref(),
                    options.model.as_deref(),
                    line,
                    &transcript,
                    &tools,
                    &schema,
                    ExecutionContext {
                        session_id: session_id.clone(),
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
                let outcome = response?;
                transcript = outcome.transcript;
                if !matches!(outcome.response, serde_json::Value::String(_)) {
                    crate::term::print_json_output(&outcome.response);
                }
            }
            Err(ReadlineError::Interrupted | ReadlineError::Eof) => break,
            Err(err) => return Err(err.into()),
        }
    }

    Ok(())
}

fn refresh_prompt_helper(
    editor: &mut Editor<crate::term::PromptHelper, DefaultHistory>,
    app_name: &str,
    default_provider: &str,
    override_provider: Option<&str>,
) {
    if let Some(helper) = editor.helper_mut() {
        helper.set_context(crate::term::chat_context(
            app_name,
            default_provider,
            override_provider,
        ));
    }
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
