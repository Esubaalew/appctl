//! Pretty-print [`AgentEvent`] streams for terminal chat / `appctl run`.

use std::{
    borrow::Cow,
    collections::HashMap,
    io::{self, Write},
    path::Path,
    sync::OnceLock,
    time::Duration,
};

use anyhow::Result;
use crossterm::terminal;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use owo_colors::OwoColorize;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph, Widget},
};
use rustyline::{
    Context, Helper,
    completion::{Completer, Pair},
    highlight::Highlighter,
    hint::Hinter,
    validate::Validator,
};
use serde_json::Value;
use syntect::{
    easy::HighlightLines,
    highlighting::{Theme, ThemeSet},
    parsing::SyntaxSet,
    util::as_24_bit_terminal_escaped,
};
use tokio::sync::mpsc::Receiver;

use crate::events::{AgentEvent, ToolStatus};
use crate::schema::{Schema, SyncSource};

const TOOL_PREVIEW_LINES: usize = 12;

// ── Product UI (shared CLI design system) ────────────────────────────────────

/// Typical width for framed blocks and wrapped text in non-TUI flows.
pub fn cli_content_width() -> usize {
    response_width()
}

/// Branded one-shot command header (before `dialoguer` prompts).
pub fn print_flow_header(title: &str, subtitle: Option<&str>) {
    let w = cli_content_width().min(72);
    let line = "═".repeat(w);
    println!();
    println!("{}", line.dimmed());
    println!(
        "  {}{}",
        "appctl".cyan().bold(),
        format!(" — {title}").white().bold()
    );
    if let Some(s) = subtitle {
        println!("  {}", s.dimmed());
    }
    println!("{}", line.dimmed());
    println!();
}

/// Section title with horizontal rules (for multi-step flows).
pub fn print_section_title(title: &str) {
    let w = cli_content_width().min(78);
    println!();
    println!("{}", "─".repeat(w).dimmed());
    println!("  {}", title.white().bold());
    println!("{}", "─".repeat(w).dimmed());
}

/// Smaller subheading.
pub fn print_subsection(label: &str) {
    println!();
    println!("  {}", label.cyan().bold());
}

/// Parse JSON error bodies and return a short human line (no huge JSON dumps).
pub fn format_api_error_summary(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "<empty body>".to_string();
    }
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        let mut line = String::new();
        if let Some(e) = v.get("error").and_then(Value::as_object) {
            if let Some(s) = e.get("message").and_then(Value::as_str) {
                line.push_str(s);
            }
            if let Some(st) = e.get("status").and_then(Value::as_str) {
                if !line.is_empty() {
                    line.push_str(" — ");
                }
                line.push_str("status: ");
                line.push_str(st);
            }
        }
        if line.is_empty() {
            if let Some(s) = v.pointer("/error/message").and_then(Value::as_str) {
                line.push_str(s);
            } else if let Some(s) = v.get("message").and_then(Value::as_str) {
                line.push_str(s);
            } else if let Some(e) = v.get("error") {
                if let Some(s) = e.as_str() {
                    line.push_str(s);
                } else if let Some(s) = e.get("message").and_then(Value::as_str) {
                    line.push_str(s);
                }
            }
        }
        if !line.is_empty() {
            return trim_one_line(&line, 400);
        }
    }
    trim_one_line(
        &trimmed.split_whitespace().collect::<Vec<_>>().join(" "),
        320,
    )
}

fn extract_google_error_message(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body.trim()).ok()?;
    v.pointer("/error/message")
        .and_then(|x| x.as_str())
        .map(str::to_string)
}

/// Like [`format_api_error_summary`] but prefers `error.message` only (no extra `status:` tail).
pub fn format_google_error_detail_line(body: &str) -> String {
    extract_google_error_message(body).unwrap_or_else(|| format_api_error_summary(body))
}

/// Chat-facing text for `generateContent` HTTP failures: no raw JSON, no “Google API returned …” phrasing.
pub fn user_message_google_genai_http_error(status: u16, body: &str, model: &str) -> String {
    let api_msg = extract_google_error_message(body);
    let low = api_msg.as_deref().unwrap_or("").to_ascii_lowercase();
    let model_hint = if model.is_empty() {
        String::new()
    } else {
        format!(" (model: {model})")
    };

    match status {
        503 if low.contains("high demand")
            || low.contains("overloaded")
            || low.contains("unavailable")
            || low.contains("capacity")
            || low.contains("try again later") =>
        {
            format!(
                "The model is temporarily overloaded — demand spikes are usually short. \
                 Wait a few seconds and resend, or use /model to try another model.{model_hint}"
            )
        }
        503 => {
            format!(
                "The model is temporarily unavailable. Retry shortly or use /model.{model_hint}"
            )
        }
        429 if low.contains("quota") => {
            format!(
                "Quota or usage limits may apply for this key or model. \
                 Try /model for a lighter model, wait, or check limits for the account tied to your API key.{model_hint}"
            )
        }
        429 => {
            format!(
                "Too many requests right now. Wait a moment, then retry, or use /model.{model_hint}"
            )
        }
        401 | 403 => {
            format!(
                "The API key was not accepted. Update the stored key, then run `appctl auth provider login` or `appctl init` — and `appctl doctor` to verify.{model_hint}"
            )
        }
        400 | 404 => {
            let detail = api_msg
                .as_deref()
                .map_or_else(String::new, |m| format!(" {m}"));
            format!("The request was rejected (model name or parameters).{detail}{model_hint}")
        }
        _ => {
            let detail = api_msg
                .or_else(|| {
                    let s = format_api_error_summary(body);
                    if s == "<empty body>" { None } else { Some(s) }
                })
                .map(|d| format!(" {d}"))
                .unwrap_or_default();
            format!(
                "appctl could not get a model reply (HTTP {status}).{detail} \
                 Retry, use /model, or run `appctl doctor` if this continues.{model_hint}"
            )
        }
    }
}

fn trim_one_line(s: &str, max: usize) -> String {
    let c: String = s.chars().take(max).collect();
    if c.len() < s.chars().count() {
        format!("{c}…")
    } else {
        c
    }
}

/// `HTTP …` + human summary, optional multi-line `details` for support.
pub fn print_http_error_block(status_label: &str, body: &str) {
    let summary = format_api_error_summary(body);
    println!();
    print_status_error(&format!("{status_label}: {summary}"));
    if let Ok(v) = serde_json::from_str::<Value>(body.trim()) {
        if serde_json::to_string(&v).unwrap_or_default().len() > summary.len() + 40 {
            println!();
            print_subsection("response detail");
            if let Ok(pretty) = serde_json::to_string_pretty(&v) {
                print_boxed_text("", pretty.lines(), Some(16));
            }
        }
    }
}

pub fn print_path_row(label: &str, path: &Path) {
    println!("  {} {}", format!("{label}:").dimmed(), path.display());
}

pub fn print_kv_block(title: &str, rows: &[(&str, &str)]) {
    print_subsection(title);
    for (k, v) in rows {
        println!("  {} {}", format!("{k}:").dimmed(), v);
    }
}

/// Success / warn / error lines with consistent iconography.
pub fn print_status_success(msg: &str) {
    println!("  {} {}", "✔".green().bold(), msg);
}

pub fn print_status_warn(msg: &str) {
    eprintln!("  {} {}", "⚠".yellow().bold(), msg);
}

pub fn print_status_error(msg: &str) {
    eprintln!("  {} {}", "✘".red().bold(), msg);
}

pub fn print_tip(msg: &str) {
    println!();
    println!("  {} {}", "→".cyan(), msg.dimmed());
}

/// Bulleted list inside a light frame.
pub fn print_bullets_framed(title: Option<&str>, items: &[impl AsRef<str>]) {
    if let Some(t) = title {
        print_subsection(t);
    }
    let w = (cli_content_width() - 4).max(32);
    println!("  ┌{}", "─".repeat(w.min(60)));
    for item in items {
        println!("  │ {}", item.as_ref());
    }
    println!("  └{}", "─".repeat(w.min(60)));
}

/// Framed panel with a list of lines (e.g. model names).
pub fn print_framed_list(title: &str, items: &[impl AsRef<str>], max_items: Option<usize>) {
    let take = max_items.map_or(items.len(), |m| m.min(items.len()));
    let w = (cli_content_width() - 4).clamp(40, 70);
    println!();
    println!("  ┌─ {}", title.dimmed());
    for line in items.iter().take(take) {
        let s = line.as_ref();
        let shown = if s.chars().count() > w {
            let c: String = s.chars().take(w - 1).collect();
            format!("{c}…")
        } else {
            s.to_string()
        };
        println!("  │ {}", shown);
    }
    if let Some(limit) = max_items
        && items.len() > limit
    {
        let rest = items.len() - limit;
        println!("  │ {}", format!("... +{rest} more").dimmed());
    }
    println!("  └{}", "─".repeat((w - 2).min(50)));
}

/// Pretty JSON in the same box style as tool previews (public for run/config).
pub fn print_boxed_pretty_value(value: &Value) {
    if let Ok(pretty) = serde_json::to_string_pretty(value) {
        print_boxed_text("", pretty.lines(), None);
    } else {
        println!("{value}");
    }
}

/// Same as chat [`print_json_output`] but explicit name for product CLI.
pub fn print_json_pretty_value(value: &Value) {
    print_json_output(value);
}

fn print_boxed_text<'a>(
    indent: &str,
    lines: impl Iterator<Item = &'a str>,
    max_lines: Option<usize>,
) {
    print_boxed_lines(indent, lines, max_lines);
}

pub struct PromptHelper {
    context: String,
}

impl PromptHelper {
    pub fn new(context: String) -> Self {
        Self { context }
    }

    pub fn set_context(&mut self, context: String) {
        self.context = context;
    }

    pub fn plain_prompt(&self) -> String {
        format!("appctl[{}]▶ ", self.context)
    }
}

impl Helper for PromptHelper {}

impl Hinter for PromptHelper {
    type Hint = String;
}

impl Validator for PromptHelper {}

impl Completer for PromptHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        _line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        Ok((pos, Vec::new()))
    }
}

impl Highlighter for PromptHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        _prompt: &'p str,
        _default: bool,
    ) -> Cow<'b, str> {
        Cow::Owned(styled_prompt(&self.context))
    }
}

pub fn chat_context(
    app_name: &str,
    default_provider: &str,
    override_provider: Option<&str>,
) -> String {
    format!(
        "{} · {}",
        app_name,
        override_provider.unwrap_or(default_provider)
    )
}

/// One-line description of the sync source for chat banners (not the OpenAPI-only legacy text).
pub fn session_sync_line(schema: &Schema) -> String {
    format!("Tools synced from {}.", session_sync_phrase(&schema.source))
}

fn session_sync_phrase(source: &SyncSource) -> String {
    match source {
        SyncSource::Openapi => "an OpenAPI document".to_string(),
        SyncSource::Django => "Django/DRF introspection".to_string(),
        SyncSource::Flask => "Flask route introspection".to_string(),
        SyncSource::Db => "a database connection".to_string(),
        SyncSource::Url => "a live URL and forms".to_string(),
        SyncSource::Mcp => "MCP (Model Context Protocol)".to_string(),
        SyncSource::Rails => "Rails route/schema introspection".to_string(),
        SyncSource::Laravel => "Laravel route/migration introspection".to_string(),
        SyncSource::Aspnet => "ASP.NET (or bundled OpenAPI) introspection".to_string(),
        SyncSource::Strapi => "Strapi introspection".to_string(),
        SyncSource::Supabase => "PostgREST (Supabase) introspection".to_string(),
        SyncSource::Plugin => "a dynamic plugin".to_string(),
    }
}

/// Framed session header for `appctl chat` and `appctl run` (non-JSON).
pub struct ChatBannerInfo<'a> {
    pub context: &'a str,
    pub registry_name: &'a str,
    pub app_dir: &'a Path,
    pub schema: &'a Schema,
    pub resource_count: usize,
    pub tool_count: usize,
    pub context_note: Option<&'a str>,
    pub app_description: Option<&'a str>,
}

pub fn print_chat_banner(b: &ChatBannerInfo<'_>) {
    let width = terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(88)
        .clamp(64, 100) as u16;
    let text_cols = (width as usize).saturating_sub(4);
    let mut lines = vec![
        Line::from(session_sync_line(b.schema)),
        Line::from(format!("app list / app use name: {}", b.registry_name)),
    ];
    if let Some(s) = b.app_description.map(str::trim).filter(|s| !s.is_empty()) {
        lines.push(Line::from(format!(
            "about: {}",
            trim_one_line(s, text_cols.max(40))
        )));
    }
    lines.push(Line::from(format!(
        "app directory: {}",
        b.app_dir.display()
    )));
    if let Some(note) = b.context_note {
        lines.push(Line::from(trim_one_line(note, text_cols.max(40))));
    }
    let text_line_count = lines.len() as u16;
    // `Constraint::Length(4)` fits two inner text lines; add one per extra line of content.
    let hero_h = 4u16 + text_line_count.saturating_sub(2);
    let area_height = (hero_h + 4u16).min(18);
    let area = Rect::new(0, 0, width, area_height);
    let mut buffer = Buffer::empty(area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(hero_h), Constraint::Length(4)])
        .split(area);

    let title = format!(" appctl — chat [{}] ", b.context);
    let hero = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::ALL).title(title));
    hero.render(chunks[0], &mut buffer);

    let details = Paragraph::new(Text::from(vec![
        Line::from(format!(
            "resources: {}   tools: {}",
            b.resource_count, b.tool_count
        )),
        Line::from("slash commands: /provider NAME   /model NAME   /read-only on|off   /dry-run on|off   /exit"),
    ]))
    .block(Block::default().borders(Borders::ALL).title(" slash commands "));
    details.render(chunks[1], &mut buffer);

    println!();
    for line in buffer_to_lines(&buffer, area) {
        println!("{}", line.dimmed());
    }
    println!();
}

pub fn print_json_output(value: &Value) {
    if let Ok(pretty) = serde_json::to_string_pretty(value) {
        print_boxed_lines("", pretty.lines(), None);
    } else {
        println!("{value}");
    }
}

pub async fn run_event_printer(mut rx: Receiver<AgentEvent>) -> Result<()> {
    let mut printer = EventPrinter::new();

    while let Some(ev) = rx.recv().await {
        match ev {
            AgentEvent::UserPrompt { text } => {
                printer.stop_spinner();
                println!();
                println!("{} {}", "❯".cyan().bold(), text.bold());
                printer.start_spinner("thinking…");
            }
            AgentEvent::AwaitingInput => {
                printer.stop_spinner();
                printer.finish_inline_assistant_text();
            }
            AgentEvent::AssistantDelta { text } => {
                if text.is_empty() {
                    continue;
                }
                printer.stop_spinner();
                printer.finish_inline_thought();
                print!("{}", text);
                let _ = io::stdout().flush();
                printer.assistant_text_inline = !text.ends_with('\n');
            }
            AgentEvent::AssistantThoughtDelta { text } => {
                if text.is_empty() {
                    continue;
                }
                printer.stop_spinner();
                printer.note_thinking_delta(&text);
            }
            AgentEvent::AssistantThought { text } => {
                if text.is_empty() {
                    continue;
                }
                printer.stop_spinner();
                printer.finish_inline_thought();
            }
            AgentEvent::AssistantMessage { text } => {
                printer.stop_spinner();
                printer.assistant_text_inline = false;
                printer.finish_inline_thought();
                printer.print_markdown_response(&text);
            }
            AgentEvent::ToolCall {
                id,
                name,
                arguments,
            } => {
                printer.stop_spinner();
                printer.finish_inline_thought();
                printer.assistant_text_inline = false;
                printer.tool_names.insert(id.clone(), name.clone());
                printer.print_tool_call(&name, &id, &arguments);
                printer.start_spinner(&format!("calling {name}…"));
            }
            AgentEvent::ToolResult {
                id,
                result,
                status,
                duration_ms,
            } => {
                printer.stop_spinner();
                printer.finish_inline_thought();
                printer.assistant_text_inline = false;
                let tool_name = printer
                    .tool_names
                    .remove(&id)
                    .unwrap_or_else(|| "tool".to_string());
                printer.print_tool_result(&tool_name, &id, &result, status, duration_ms);
                printer.start_spinner("thinking…");
            }
            AgentEvent::Error { message } => {
                printer.stop_spinner();
                printer.finish_inline_thought();
                print_status_error(&message);
            }
            AgentEvent::SessionState { .. } => {}
            AgentEvent::ContextNotice { message } => {
                printer.stop_spinner();
                printer.assistant_text_inline = false;
                print_tip(&message);
                printer.start_spinner("thinking…");
            }
            AgentEvent::Done => {
                printer.finish_inline_thought();
                break;
            }
        }
    }

    printer.stop_spinner();
    Ok(())
}

struct EventPrinter {
    spinner: Option<ProgressBar>,
    skin: termimad::MadSkin,
    tool_names: HashMap<String, String>,
    assistant_text_inline: bool,
    thought_inline: bool,
}

impl EventPrinter {
    fn new() -> Self {
        let mut skin = termimad::MadSkin::default_dark();
        skin.bold.set_fg(termimad::crossterm::style::Color::Cyan);
        skin.inline_code
            .set_bg(termimad::crossterm::style::Color::DarkGrey);
        Self {
            spinner: None,
            skin,
            tool_names: HashMap::new(),
            assistant_text_inline: false,
            thought_inline: false,
        }
    }

    fn start_spinner(&mut self, label: &str) {
        self.stop_spinner();
        let pb = ProgressBar::new_spinner();
        pb.set_draw_target(ProgressDrawTarget::stderr());
        pb.set_style(
            ProgressStyle::with_template("{spinner:.cyan} {msg}")
                .expect("valid spinner template")
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.set_message(label.to_string());
        pb.enable_steady_tick(Duration::from_millis(80));
        self.spinner = Some(pb);
    }

    fn stop_spinner(&mut self) {
        if let Some(pb) = self.spinner.take() {
            pb.finish_and_clear();
        }
    }

    fn finish_inline_assistant_text(&mut self) {
        if self.assistant_text_inline {
            println!();
            self.assistant_text_inline = false;
        }
    }

    fn note_thinking_delta(&mut self, text: &str) {
        if !self.thought_inline {
            println!();
            print!("{}", "  thinking…".dimmed());
            self.thought_inline = true;
        }
        let _ = text;
        let _ = io::stdout().flush();
    }

    fn finish_inline_thought(&mut self) {
        if self.thought_inline {
            println!();
            self.thought_inline = false;
        }
    }

    fn print_markdown_response(&self, text: &str) {
        let width = response_width();
        let border = "─".repeat(width);
        let text = normalize_assistant_text(text);
        let rendered = self.skin.text(&text, Some(width));
        println!();
        println!("{}", border.dimmed());
        print!("{}", rendered);
        println!("{}", border.dimmed());
    }

    fn print_tool_call(&self, name: &str, id: &str, arguments: &Value) {
        println!();
        println!("  {} {}", "●".cyan().bold(), name.yellow().bold());
        println!("  {} {}", "id".dimmed(), id.dimmed());
        if let Ok(pretty) = serde_json::to_string_pretty(arguments) {
            print_boxed_lines("  ", pretty.lines(), Some(TOOL_PREVIEW_LINES));
        }
    }

    fn print_tool_result(
        &self,
        tool_name: &str,
        id: &str,
        result: &Value,
        status: ToolStatus,
        duration_ms: u64,
    ) {
        let status_label = match status {
            ToolStatus::Ok => "ok".green().bold().to_string(),
            ToolStatus::Error => "error".red().bold().to_string(),
        };
        let arrow = match status {
            ToolStatus::Ok => "→".green().to_string(),
            ToolStatus::Error => "→".red().to_string(),
        };
        println!(
            "  {} {} · {}ms · {}={}",
            arrow,
            status_label,
            duration_ms.to_string().dimmed(),
            "id".dimmed(),
            id.dimmed()
        );
        if let Ok(pretty) = serde_json::to_string_pretty(result) {
            print_boxed_lines("  ", pretty.lines(), Some(TOOL_PREVIEW_LINES));
        } else {
            println!("  {} {}", tool_name.dimmed(), result);
        }
    }
}

fn response_width() -> usize {
    terminal::size()
        .map(|(width, _)| width as usize)
        .unwrap_or(80)
        .saturating_sub(4)
        .max(20)
}

fn normalize_assistant_text(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        normalized.push(ch);
        if matches!(ch, '.' | '!' | '?')
            && let Some(next) = chars.peek()
            && !next.is_whitespace()
            && sentence_continuation(*next)
        {
            normalized.push(' ');
        }
    }
    normalize_markdown_tables(&normalized)
}

fn sentence_continuation(ch: char) -> bool {
    ch.is_uppercase() || matches!(ch, '"' | '\'' | '“' | '”' | '‘' | '’')
}

fn normalize_markdown_tables(text: &str) -> String {
    text.lines()
        .map(|line| {
            let trimmed = line.trim();
            if !looks_like_table_row(trimmed) {
                return line.to_string();
            }
            let cells = trimmed
                .trim_matches('|')
                .split('|')
                .map(|cell| cell.trim())
                .collect::<Vec<_>>();
            format!("| {} |", cells.join(" | "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn looks_like_table_row(line: &str) -> bool {
    line.starts_with('|') && line.ends_with('|') && line.matches('|').count() >= 3
}

fn styled_prompt(context: &str) -> String {
    format!(
        "{}{}{}{} ",
        "appctl".cyan().bold(),
        "[".dimmed(),
        context.magenta().bold(),
        "]▶".dimmed(),
    )
}

fn print_boxed_lines<'a>(
    indent: &str,
    lines: impl Iterator<Item = &'a str>,
    max_lines: Option<usize>,
) {
    let lines = lines.collect::<Vec<_>>();
    println!("{indent}┌────────────────────────────────────────────────────────");
    let visible_count = max_lines.map_or(lines.len(), |limit| lines.len().min(limit));
    for line in lines.iter().take(visible_count) {
        println!("{indent}│ {}", highlight_json_line(line));
    }
    if let Some(limit) = max_lines
        && lines.len() > limit
    {
        let hidden = lines.len() - limit;
        println!("{indent}│ {}", format!("... +{hidden} more lines").dimmed());
    }
    println!("{indent}└────────────────────────────────────────────────────────");
}

fn highlight_json_line(line: &str) -> String {
    let ps = syntax_set();
    let mut highlighter = HighlightLines::new(json_syntax(ps), theme());
    match highlighter.highlight_line(line, ps) {
        Ok(ranges) => as_24_bit_terminal_escaped(&ranges, false),
        Err(_) => line.to_string(),
    }
}

fn syntax_set() -> &'static SyntaxSet {
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        ThemeSet::load_defaults()
            .themes
            .get("base16-ocean.dark")
            .cloned()
            .expect("base16-ocean.dark theme should exist")
    })
}

fn json_syntax(ps: &SyntaxSet) -> &syntect::parsing::SyntaxReference {
    ps.find_syntax_by_extension("json")
        .expect("json syntax should exist")
}

fn buffer_to_lines(buffer: &Buffer, area: Rect) -> Vec<String> {
    let mut lines = Vec::new();
    for y in area.top()..area.bottom() {
        let mut line = String::new();
        for x in area.left()..area.right() {
            line.push_str(buffer.get(x, y).symbol());
        }
        lines.push(line.trim_end().to_string());
    }
    lines
}
