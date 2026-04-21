//! Pretty-print [`AgentEvent`] streams for terminal chat / `appctl run`.

use std::time::Duration;

use anyhow::Result;
use indicatif::{ProgressBar, ProgressDrawTarget};
use owo_colors::OwoColorize;
use tokio::sync::mpsc::Receiver;

use crate::events::{AgentEvent, ToolStatus};

pub async fn run_event_printer(mut rx: Receiver<AgentEvent>) -> Result<()> {
    let mut spinner: Option<ProgressBar> = None;
    let skin = termimad::MadSkin::default_dark();

    while let Some(ev) = rx.recv().await {
        match ev {
            AgentEvent::UserPrompt { text } => {
                println!();
                println!("{} {}", "❯".cyan(), text.bold());
                let pb = ProgressBar::new_spinner();
                pb.set_draw_target(ProgressDrawTarget::stderr());
                pb.set_message("thinking…");
                pb.enable_steady_tick(Duration::from_millis(90));
                spinner = Some(pb);
            }
            AgentEvent::AssistantDelta { text } => {
                if text.is_empty() {
                    continue;
                }
                if let Some(pb) = spinner.take() {
                    pb.finish_and_clear();
                }
                print!("{}", skin.term_text(&text));
            }
            AgentEvent::AssistantMessage { text } => {
                if let Some(pb) = spinner.take() {
                    pb.finish_and_clear();
                }
                println!("{}", skin.term_text(&text));
            }
            AgentEvent::ToolCall {
                id,
                name,
                arguments,
            } => {
                if let Some(pb) = spinner.take() {
                    pb.finish_and_clear();
                }
                println!(
                    "  {} {} {}",
                    "•".yellow(),
                    "tool".dimmed(),
                    name.as_str().yellow().bold()
                );
                println!("    {} {}", "id".dimmed(), id.dimmed());
                if let Ok(pretty) = serde_json::to_string_pretty(&arguments) {
                    for line in pretty.lines() {
                        println!("    {}", line.dimmed());
                    }
                }
            }
            AgentEvent::ToolResult {
                id,
                result,
                status,
                duration_ms,
            } => {
                let st = match status {
                    ToolStatus::Ok => "ok".green().to_string(),
                    ToolStatus::Error => "error".red().to_string(),
                };
                println!(
                    "    {} {} · {}ms · {}",
                    "→".green(),
                    st,
                    duration_ms,
                    format!("id={id}").dimmed()
                );
                if let Ok(pretty) = serde_json::to_string_pretty(&result) {
                    let lines: Vec<_> = pretty.lines().take(12).collect();
                    for line in &lines {
                        println!("      {}", line.dimmed());
                    }
                    if pretty.lines().count() > 12 {
                        println!("      {}", "…".dimmed());
                    }
                }
            }
            AgentEvent::Error { message } => {
                if let Some(pb) = spinner.take() {
                    pb.finish_and_clear();
                }
                eprintln!("{} {}", "error:".red().bold(), message);
            }
            AgentEvent::Done => break,
        }
    }

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    Ok(())
}
