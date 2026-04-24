use anyhow::Result;
use rustyline::{Editor, error::ReadlineError, history::DefaultHistory};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{
    ai::run_agent,
    config::{AppConfig, ConfigPaths},
    executor::ExecutionContext,
    safety::SafetyMode,
    sync::{load_runtime_tools, load_schema},
};

#[derive(Debug, Clone)]
pub struct ChatOptions {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub session: Option<String>,
    pub read_only: bool,
    pub dry_run: bool,
    pub confirm: bool,
    pub strict: bool,
    /// Shown under the app directory in the session banner.
    pub context_note: Option<String>,
}

pub async fn run_chat(
    paths: &ConfigPaths,
    config: &AppConfig,
    registry_name: &str,
    mut options: ChatOptions,
) -> Result<()> {
    let schema = load_schema(paths)?;
    let tools = load_runtime_tools(paths, config)?;
    let banner = config.banner_label(registry_name);
    let context = crate::term::chat_context(banner, &config.default, options.provider.as_deref());
    crate::term::print_chat_banner(&crate::term::ChatBannerInfo {
        context: &context,
        registry_name,
        app_dir: &paths.root,
        schema: &schema,
        resource_count: schema.resources.len(),
        tool_count: tools.len(),
        context_note: options.context_note.as_deref(),
        app_description: config.description.as_deref(),
    });
    let mut editor = Editor::<crate::term::PromptHelper, DefaultHistory>::new()?;
    editor.set_helper(Some(crate::term::PromptHelper::new(context)));
    let session_name = options.session.clone();
    let session_id = session_name
        .as_deref()
        .map(session_id_from_name)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let mut transcript = Vec::new();
    let registry_name = registry_name.to_string();

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
                        config,
                        &registry_name,
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
                    &registry_name,
                    options.provider.as_deref(),
                    options.model.as_deref(),
                    line,
                    &transcript,
                    &tools,
                    &schema,
                    ExecutionContext {
                        session_id: session_id.clone(),
                        session_name: session_name.clone(),
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

fn session_id_from_name(name: &str) -> String {
    let trimmed = name.trim();
    let slug = trimmed
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let prefix = if slug.is_empty() { "session" } else { &slug };
    format!("{prefix}-{}", Uuid::new_v4())
}

fn refresh_prompt_helper(
    editor: &mut Editor<crate::term::PromptHelper, DefaultHistory>,
    config: &AppConfig,
    registry_name: &str,
    default_provider: &str,
    override_provider: Option<&str>,
) {
    if let Some(helper) = editor.helper_mut() {
        let label = config.banner_label(registry_name);
        helper.set_context(crate::term::chat_context(
            label,
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
