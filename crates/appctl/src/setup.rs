//! Guided first-run setup (`appctl setup`).
//!
//! The **portable** tool source is an **OpenAPI** document (file or URL): one contract, any stack.
//! **Framework- and database-specific** sync paths (Flask, Django, `sync --db`, …) are **optional
//! heuristics** for repos that do not have (or do not want to maintain) a spec; they are not a
//! substitute for a published OpenAPI contract when you need spec-accurate HTTP.

use std::{
    fs,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use dialoguer::{Confirm, Input, Select};

use crate::{
    config::{AppConfig, ConfigPaths},
    doctor::{DoctorRunArgs, run_doctor},
    init::{prompt_register_app, refine_app_label_and_description, run_init},
    sync::{SyncRequest, run_sync},
    term::{
        print_flow_header, print_path_row, print_section_title, print_status_success,
        print_status_warn, print_tip,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupSourceChoice {
    AutoDetect,
    OpenApi,
    Database,
    Django,
    Flask,
    Rails,
    Laravel,
    AspNet,
    Strapi,
    Url,
    SkipSync,
    NotSure,
}

impl SetupSourceChoice {
    fn items() -> &'static [&'static str] {
        &[
            "Inspect project folder — find OpenAPI, DB, or framework (start here)",
            "OpenAPI spec: URL (running server) or path to file",
            "Database / datastore connection",
            "Django project",
            "Flask project",
            "Rails project",
            "Laravel project",
            "ASP.NET project",
            "Strapi project",
            "Website / browser-login URL",
            "Skip sync for now (I already have tools or will sync later)",
            "I'm not sure yet",
        ]
    }

    fn from_index(index: usize) -> Self {
        match index {
            0 => Self::AutoDetect,
            1 => Self::OpenApi,
            2 => Self::Database,
            3 => Self::Django,
            4 => Self::Flask,
            5 => Self::Rails,
            6 => Self::Laravel,
            7 => Self::AspNet,
            8 => Self::Strapi,
            9 => Self::Url,
            10 => Self::SkipSync,
            _ => Self::NotSure,
        }
    }

    fn is_http_like(self) -> bool {
        !matches!(
            self,
            Self::AutoDetect | Self::Database | Self::SkipSync | Self::NotSure
        )
    }
}

#[derive(Debug, Clone)]
struct SetupSyncOutcome {
    did_sync: bool,
    doctor_source: Option<SetupSourceChoice>,
}

impl SetupSyncOutcome {
    fn skipped() -> Self {
        Self {
            did_sync: false,
            doctor_source: None,
        }
    }

    fn synced(source: SetupSourceChoice) -> Self {
        Self {
            did_sync: true,
            doctor_source: Some(source),
        }
    }
}

#[derive(Debug, Clone)]
struct DetectedSyncSource {
    label: String,
    source: SetupSourceChoice,
    request: SyncRequest,
}

pub async fn run_setup(paths: &ConfigPaths) -> Result<()> {
    paths.ensure()?;
    print_flow_header(
        "setup",
        Some("A guided path from install to a working terminal or web chat"),
    );
    print_app_context(paths)?;

    if !io::stdin().is_terminal() {
        print_non_interactive_setup(paths);
        return Ok(());
    }

    ensure_provider(paths).await?;
    ensure_app_identity(paths)?;
    let source = choose_source()?;
    let outcome = maybe_run_sync(paths, source).await?;
    let checks_ok = maybe_run_doctor(paths, outcome).await;
    print_next_steps(paths, checks_ok);
    Ok(())
}

fn print_app_context(paths: &ConfigPaths) -> Result<()> {
    print_section_title("0. App context");
    let cwd = std::env::current_dir().context("failed to read current working directory")?;
    print_path_row("current directory", &cwd);
    print_path_row("app directory", &paths.root);
    print_path_row("config", &paths.config);
    print_path_row("tools", &paths.tools);

    if dirs::home_dir()
        .map(|h| h.join(".appctl") == paths.root)
        .unwrap_or(false)
    {
        print_tip(
            "Global home app: use this only on purpose. For a project, run setup from the project folder or pass `--app-dir project/.appctl`.",
        );
    } else {
        print_tip(
            "Project app: setup will write config and synced tools under this project’s `.appctl`.",
        );
    }
    Ok(())
}

async fn ensure_provider(paths: &ConfigPaths) -> Result<()> {
    let needs_provider = match AppConfig::load(paths) {
        Ok(config) => config.providers.is_empty(),
        Err(_) => true,
    };

    if needs_provider {
        print_section_title("1. AI provider");
        println!("  appctl needs one AI provider before chat, run, or serve can work.");
        run_init(paths).await?;
    } else {
        print_section_title("1. AI provider");
        print_status_success("provider already configured");
        if Confirm::new()
            .with_prompt("Reconfigure the AI provider now?")
            .default(false)
            .interact()?
        {
            run_init(paths).await?;
        }
    }
    Ok(())
}

fn ensure_app_identity(paths: &ConfigPaths) -> Result<()> {
    print_section_title("App identity");
    refine_app_label_and_description(paths)?;
    prompt_register_app(paths)?;
    Ok(())
}

fn choose_source() -> Result<SetupSourceChoice> {
    print_section_title("2. App tools");
    println!("  appctl needs synced tools so the agent knows what it can do.");
    let items = [
        "Inspect this project and recommend a source",
        "OpenAPI spec URL or file",
        "Database connection",
        "Manual / advanced source list",
        "Skip sync for now",
    ];
    let index = Select::new()
        .with_prompt("What should appctl connect to?")
        .items(items)
        .default(0)
        .interact()?;
    Ok(match index {
        0 => SetupSourceChoice::AutoDetect,
        1 => SetupSourceChoice::OpenApi,
        2 => SetupSourceChoice::Database,
        3 => SetupSourceChoice::NotSure,
        _ => SetupSourceChoice::SkipSync,
    })
}

async fn maybe_run_sync(
    paths: &ConfigPaths,
    source: SetupSourceChoice,
) -> Result<SetupSyncOutcome> {
    match source {
        SetupSourceChoice::AutoDetect => run_auto_detected_sync(paths).await,
        SetupSourceChoice::NotSure => run_not_sure_sync(paths).await,
        _ => run_manual_source_sync(paths, source).await,
    }
}

/// When the user is unsure, explain options instead of jumping straight into a scan.
async fn run_not_sure_sync(paths: &ConfigPaths) -> Result<SetupSyncOutcome> {
    print_section_title("2. App tools — not sure yet");
    tip_if_home_global_appctl(paths);
    print_tip("Scan this directory, or use `--app-dir` for another project’s `.appctl`.");

    let items = [
        "Scan my current directory for sources (OpenAPI / framework / SQLite)",
        "Choose OpenAPI, database, or framework manually",
        "Skip sync for now",
    ];
    let selected = Select::new()
        .with_prompt("What do you want to do?")
        .items(items)
        .default(0)
        .interact()?;

    match selected {
        0 => run_auto_detected_sync(paths).await,
        1 => {
            let manual = choose_manual_source()?;
            run_manual_source_sync(paths, manual).await
        }
        2 => {
            print_status_warn("skipped sync");
            print_tip("Run `appctl sync --help` when you are ready to add tools.");
            Ok(SetupSyncOutcome::skipped())
        }
        _ => unreachable!(),
    }
}

async fn run_manual_source_sync(
    paths: &ConfigPaths,
    source: SetupSourceChoice,
) -> Result<SetupSyncOutcome> {
    if matches!(source, SetupSourceChoice::SkipSync) {
        print_status_warn("skipped sync");
        print_tip("Run `appctl sync --help` when you are ready to add tools.");
        return Ok(SetupSyncOutcome::skipped());
    }

    let force = confirm_replace_existing_tools(paths)?;

    let mut request = SyncRequest {
        force,
        ..Default::default()
    };

    match source {
        SetupSourceChoice::OpenApi => {
            let scan_root = inspection_project_root(paths)?;
            let spec_files = find_openapi_spec_files(&scan_root);
            let default_line = spec_files.first().map(|p| openapi_prompt_default_path(p));
            if default_line.is_none() {
                print_tip(
                    "No openapi/swagger file found under this project. Use a file path, a live URL (server running), or go back and pick \"Inspect project\".",
                );
            }
            request.openapi = Some(prompt_string(
                "OpenAPI: URL or spec file path",
                default_line.as_deref(),
            )?);
            request.base_url = prompt_optional("Base URL (optional, for calling the API)")?;
            request.auth_header = prompt_auth_header_optional(
                "Auth header (optional, e.g. Authorization: Bearer env:TOKEN)",
            )?;
        }
        SetupSourceChoice::Database => {
            request.db = Some(prompt_string(
                "Database URL, e.g. sqlite:///path/app.db or postgres://...",
                None,
            )?);
        }
        SetupSourceChoice::Django => {
            request.django = Some(prompt_path("Django project root", Some("."))?);
            request.base_url = Some(prompt_string_nonempty(
                "Base URL for HTTP tool calls (your running app)",
                "http://127.0.0.1:8000",
            )?);
        }
        SetupSourceChoice::Flask => {
            request.flask = Some(prompt_path("Flask project root", Some("."))?);
            request.base_url = Some(prompt_string_nonempty(
                "Base URL for HTTP tool calls (your running app)",
                "http://127.0.0.1:5000",
            )?);
        }
        SetupSourceChoice::Rails => {
            request.rails = Some(prompt_path("Rails project root", Some("."))?);
            request.base_url = Some(prompt_string_nonempty(
                "Base URL for HTTP tool calls (your running app)",
                "http://127.0.0.1:3000",
            )?);
        }
        SetupSourceChoice::Laravel => {
            request.laravel = Some(prompt_path("Laravel project root", Some("."))?);
            request.base_url = Some(prompt_string_nonempty(
                "Base URL for HTTP tool calls (your running app)",
                "http://127.0.0.1:8000",
            )?);
        }
        SetupSourceChoice::AspNet => {
            request.aspnet = Some(prompt_path("ASP.NET project root", Some("."))?);
            request.base_url = Some(prompt_string_nonempty(
                "Base URL for HTTP tool calls (your running app)",
                "http://127.0.0.1:5000",
            )?);
        }
        SetupSourceChoice::Strapi => {
            request.strapi = Some(prompt_path("Strapi project root", Some("."))?);
            request.base_url = Some(prompt_string_nonempty(
                "Base URL for HTTP tool calls (your running app)",
                "http://127.0.0.1:1337",
            )?);
        }
        SetupSourceChoice::Url => {
            request.url = Some(prompt_string("Website root URL", None)?);
            request.login_url = prompt_optional("Login page URL (optional)")?;
            request.login_user = prompt_optional("Login username (optional)")?;
            request.login_password = prompt_optional("Login password (optional)")?;
        }
        SetupSourceChoice::AutoDetect
        | SetupSourceChoice::SkipSync
        | SetupSourceChoice::NotSure => {
            unreachable!()
        }
    }

    run_sync(paths.clone(), request).await?;
    Ok(SetupSyncOutcome::synced(source))
}

async fn run_auto_detected_sync(paths: &ConfigPaths) -> Result<SetupSyncOutcome> {
    let project_root = inspection_project_root(paths)?;
    print_section_title("2a. Inspecting project");
    print_path_row(
        "inspection root (scan here for OpenAPI / code / DB files)",
        &project_root,
    );
    tip_if_home_global_appctl(paths);

    let candidates = detect_sync_sources(&project_root);
    if candidates.is_empty() {
        print_status_warn("no obvious sync source found");
        print_tip("If your app exposes OpenAPI, choose OpenAPI and paste the URL or file path.");
        print_tip("If you mainly need data tools, choose Database and paste a connection string.");
        let manual = choose_manual_source()?;
        return run_manual_source_sync(paths, manual).await;
    }

    let mut items: Vec<String> = candidates
        .iter()
        .map(|candidate| candidate.label.clone())
        .collect();
    items.push("Choose a source manually instead".to_string());
    items.push("Skip sync for now".to_string());

    let selected = Select::new()
        .with_prompt("I found these likely app sources. Which should I sync?")
        .items(&items)
        .default(0)
        .interact()?;

    if selected == candidates.len() {
        let manual = choose_manual_source()?;
        return run_manual_source_sync(paths, manual).await;
    }
    if selected == candidates.len() + 1 {
        print_status_warn("skipped sync");
        return Ok(SetupSyncOutcome::skipped());
    }

    let candidate = &candidates[selected];
    if matches!(candidate.source, SetupSourceChoice::Flask) {
        print_tip(
            "Flask: tools are inferred from Python in the project (introspection only). For a spec-driven API, prefer the OpenAPI menu. You will be asked for the HTTP `base_url` the tools should call; default is http://127.0.0.1:5000 if your `flask run` matches.",
        );
    }
    let mut request = candidate.request.clone();
    request.force = confirm_replace_existing_tools(paths)?;
    fill_detected_missing_values(candidate.source, &mut request)?;

    print_status_success(&format!("syncing detected source: {}", candidate.label));
    run_sync(paths.clone(), request).await?;
    Ok(SetupSyncOutcome::synced(candidate.source))
}

fn choose_manual_source() -> Result<SetupSourceChoice> {
    let items = &SetupSourceChoice::items()[1..SetupSourceChoice::items().len() - 1];
    let index = Select::new()
        .with_prompt("Choose the source manually")
        .items(items)
        .default(0)
        .interact()?;
    Ok(SetupSourceChoice::from_index(index + 1))
}

fn confirm_replace_existing_tools(paths: &ConfigPaths) -> Result<bool> {
    if paths.schema.exists() {
        Confirm::new()
            .with_prompt("Tools are already synced. Replace them with this setup sync?")
            .default(false)
            .interact()
            .context("prompt failed")
    } else {
        Ok(false)
    }
}

/// Default dev `http://` guess for the selected sync source. User can override; wrong host/port
/// is better than an empty base URL, which would break every HTTP tool at runtime.
fn default_dev_base_url_for_setup(source: SetupSourceChoice) -> &'static str {
    match source {
        SetupSourceChoice::Django | SetupSourceChoice::Laravel => "http://127.0.0.1:8000",
        SetupSourceChoice::Rails => "http://127.0.0.1:3000",
        SetupSourceChoice::Strapi => "http://127.0.0.1:1337",
        SetupSourceChoice::Flask | SetupSourceChoice::AspNet => "http://127.0.0.1:5000",
        SetupSourceChoice::Url => "http://127.0.0.1:5000",
        // Only used when a future auto-detect candidate adds a new `http_like` source.
        _ => "http://127.0.0.1:8080",
    }
}

fn fill_detected_missing_values(
    source: SetupSourceChoice,
    request: &mut SyncRequest,
) -> Result<()> {
    match source {
        SetupSourceChoice::OpenApi => {
            request.base_url = prompt_optional("Base URL for the running API (optional)")?;
            request.auth_header = prompt_auth_header_optional(
                "Auth header for protected routes, e.g. Authorization: Bearer env:API_TOKEN (optional)",
            )?;
        }
        source if source.is_http_like() => {
            let def = default_dev_base_url_for_setup(source);
            request.base_url = Some(prompt_string_nonempty(
                "Base URL for HTTP tool calls (where this app is reachable; required for list/get/…)",
                def,
            )?);
        }
        _ => {}
    }
    Ok(())
}

/// Folder to scan for OpenAPI files, framework markers, and SQLite databases during setup.
///
/// If `.appctl` is `~/.appctl`, the parent directory would be the entire home folder — that is
/// never a useful “project” for a shallow scan. In that case we use **current working directory**
/// (the folder the user `cd`’d into) instead. Tools and config still live under `paths.root`.
fn inspection_project_root(paths: &ConfigPaths) -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("failed to get current working directory")?;
    let home = dirs::home_dir();
    Ok(inspection_project_root_inner(paths, home.as_deref(), &cwd))
}

fn inspection_project_root_inner(paths: &ConfigPaths, home: Option<&Path>, cwd: &Path) -> PathBuf {
    if let Some(h) = home {
        if Some(h) == paths.root.parent() {
            return cwd.to_path_buf();
        }
    }
    paths
        .root
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| paths.root.clone())
}

fn tip_if_home_global_appctl(paths: &ConfigPaths) {
    let Some(h) = dirs::home_dir() else {
        return;
    };
    if h.join(".appctl") == paths.root {
        print_tip("Scan uses this directory only; config stays in ~/.appctl.");
    }
}

fn is_openapi_spec_file_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "openapi.json"
            | "openapi.yaml"
            | "openapi.yml"
            | "swagger.json"
            | "swagger.yaml"
            | "swagger.yml"
    )
}

/// Shallowest matching spec files first (used as the default in the OpenAPI prompt).
fn find_openapi_spec_files(project_root: &Path) -> Vec<PathBuf> {
    let files = collect_project_files(project_root, 5);
    let mut out: Vec<PathBuf> = files
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(is_openapi_spec_file_name)
                .unwrap_or(false)
        })
        .collect();
    out.sort_by_key(|p| p.components().count());
    out
}

fn openapi_prompt_default_path(file: &Path) -> String {
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(rel) = file.strip_prefix(&cwd) {
            return rel.display().to_string();
        }
    }
    file.display().to_string()
}

fn detect_sync_sources(project_root: &Path) -> Vec<DetectedSyncSource> {
    let mut candidates = Vec::new();
    let dirs = collect_project_dirs(project_root, 3);
    let files = collect_project_files(project_root, 4);

    for file in &files {
        let Some(name) = file.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if is_openapi_spec_file_name(name) {
            candidates.push(DetectedSyncSource {
                label: format!("OpenAPI document: {}", short_path(project_root, file)),
                source: SetupSourceChoice::OpenApi,
                request: SyncRequest {
                    openapi: Some(file.display().to_string()),
                    ..Default::default()
                },
            });
        }

        if is_likely_sqlite_file(file) && !is_junk_sqlite_path(file) {
            candidates.push(DetectedSyncSource {
                label: format!("SQLite database: {}", short_path(project_root, file)),
                source: SetupSourceChoice::Database,
                request: SyncRequest {
                    db: Some(format!("sqlite://{}", file.display())),
                    ..Default::default()
                },
            });
        }
    }

    for dir in dirs {
        if dir.join("manage.py").exists() {
            candidates.push(path_candidate(
                project_root,
                &dir,
                "Django project",
                SetupSourceChoice::Django,
                |path| SyncRequest {
                    django: Some(path),
                    ..Default::default()
                },
            ));
        }
        if dir.join("artisan").exists() {
            candidates.push(path_candidate(
                project_root,
                &dir,
                "Laravel project",
                SetupSourceChoice::Laravel,
                |path| SyncRequest {
                    laravel: Some(path),
                    ..Default::default()
                },
            ));
        }
        if dir.join("config").join("routes.rb").exists() {
            candidates.push(path_candidate(
                project_root,
                &dir,
                "Rails project",
                SetupSourceChoice::Rails,
                |path| SyncRequest {
                    rails: Some(path),
                    ..Default::default()
                },
            ));
        }
        if dir.join("src").join("api").exists()
            && dir.join("package.json").exists()
            && file_contains(&dir.join("package.json"), "strapi")
        {
            candidates.push(path_candidate(
                project_root,
                &dir,
                "Strapi project",
                SetupSourceChoice::Strapi,
                |path| SyncRequest {
                    strapi: Some(path),
                    ..Default::default()
                },
            ));
        }
        if has_aspnet_project_file(&dir) {
            candidates.push(path_candidate(
                project_root,
                &dir,
                "ASP.NET project",
                SetupSourceChoice::AspNet,
                |path| SyncRequest {
                    aspnet: Some(path),
                    ..Default::default()
                },
            ));
        }
        if looks_like_flask_project(&dir) {
            candidates.push(path_candidate(
                project_root,
                &dir,
                "Flask project",
                SetupSourceChoice::Flask,
                |path| SyncRequest {
                    flask: Some(path),
                    ..Default::default()
                },
            ));
        }
    }

    candidates.truncate(12);
    candidates
}

fn path_candidate(
    project_root: &Path,
    dir: &Path,
    label: &str,
    source: SetupSourceChoice,
    build_request: impl FnOnce(PathBuf) -> SyncRequest,
) -> DetectedSyncSource {
    DetectedSyncSource {
        label: format!("{label}: {}", short_path(project_root, dir)),
        source,
        request: build_request(dir.to_path_buf()),
    }
}

fn collect_project_dirs(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut dirs = vec![root.to_path_buf()];
    collect_project_dirs_inner(root, root, max_depth, &mut dirs);
    dirs
}

fn collect_project_dirs_inner(root: &Path, dir: &Path, max_depth: usize, out: &mut Vec<PathBuf>) {
    if depth_from(root, dir) >= max_depth {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || should_skip_dir(&path) {
            continue;
        }
        out.push(path.clone());
        collect_project_dirs_inner(root, &path, max_depth, out);
    }
}

fn collect_project_files(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_project_files_inner(root, root, max_depth, &mut files);
    files
}

fn collect_project_files_inner(root: &Path, dir: &Path, max_depth: usize, out: &mut Vec<PathBuf>) {
    if depth_from(root, dir) > max_depth {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if !should_skip_dir(&path) {
                collect_project_files_inner(root, &path, max_depth, out);
            }
        } else {
            out.push(path);
        }
    }
}

fn depth_from(root: &Path, path: &Path) -> usize {
    path.strip_prefix(root)
        .map(|relative| relative.components().count())
        .unwrap_or(0)
}

fn should_skip_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(
            ".appctl"
                | ".config"
                | ".continue"
                | ".cursor"
                | ".gemini"
                | ".git"
                | ".hg"
                | ".local"
                | ".svn"
                | ".next"
                | ".venv"
                | "build"
                | "dist"
                | "node_modules"
                | "target"
                | "vendor"
                | "venv"
        )
    )
}

/// Tool and browser caches often use `.db` / `.sqlite` under dot-folders; never suggest those for app sync.
fn is_junk_sqlite_path(path: &Path) -> bool {
    for c in path.components() {
        if let std::path::Component::Normal(name) = c {
            if matches!(
                name.to_string_lossy().as_ref(),
                ".config"
                    | ".cursor"
                    | ".continue"
                    | ".gemini"
                    | "node_modules"
                    | "target"
                    | "dist"
                    | "build"
                    | "vendor"
                    | "venv"
                    | ".venv"
            ) {
                return true;
            }
        }
    }
    // Extra guard: known paths under a macOS home even if layout changes.
    let s = path.to_string_lossy();
    s.contains("/.config/")
        || s.contains("/.continue/")
        || s.contains("/.cursor/")
        || s.contains("/.gemini/")
        || s.contains("github-copilot")
        || s.contains("gcloud")
        || s.contains("Application Support")
}

fn is_likely_sqlite_file(path: &Path) -> bool {
    if path.file_name().and_then(|name| name.to_str()) == Some("history.db") {
        return false;
    }
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("db" | "sqlite" | "sqlite3")
    )
}

fn has_aspnet_project_file(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        matches!(
            entry.path().extension().and_then(|ext| ext.to_str()),
            Some("csproj" | "fsproj" | "vbproj")
        )
    })
}

fn looks_like_flask_project(dir: &Path) -> bool {
    if !dir.join("app.py").exists() && !dir.join("wsgi.py").exists() {
        return false;
    }
    ["requirements.txt", "pyproject.toml", "Pipfile"]
        .iter()
        .any(|name| file_contains(&dir.join(name), "flask"))
}

fn file_contains(path: &Path, needle: &str) -> bool {
    fs::read_to_string(path)
        .map(|content| content.to_ascii_lowercase().contains(needle))
        .unwrap_or(false)
}

fn short_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

async fn maybe_run_doctor(paths: &ConfigPaths, outcome: SetupSyncOutcome) -> bool {
    let Some(source) = outcome.doctor_source else {
        return true;
    };
    if !outcome.did_sync || !source.is_http_like() {
        return true;
    }

    print_section_title("3. Checks");
    match run_doctor(
        paths,
        DoctorRunArgs {
            write: true,
            timeout_secs: 10,
        },
    )
    .await
    {
        Ok(()) => {
            print_status_success("doctor checks completed");
            true
        }
        Err(err) => {
            print_status_warn("doctor could not finish");
            print_tip(&format!(
                "Fix this before expecting protected tools to work. Later, run `appctl doctor --write` again. Detail: {err:#}"
            ));
            false
        }
    }
}

fn print_next_steps(paths: &ConfigPaths, checks_ok: bool) {
    if checks_ok {
        print_section_title("Ready");
    } else {
        print_section_title("Setup finished — checks need attention");
    }
    print_path_row("app directory", &paths.root);
    if checks_ok {
        print_status_success("setup flow finished");
    } else {
        print_status_warn("setup saved config/tools, but target API checks did not pass");
    }
    print_tip("Terminal: appctl chat");
    print_tip("Web:      appctl serve --open");
}

fn print_non_interactive_setup(paths: &ConfigPaths) {
    print_section_title("Setup needs an interactive terminal");
    print_path_row("app directory", &paths.root);
    println!("  Run the guided setup in a terminal:");
    println!("    appctl setup");
    println!();
    println!("  Advanced manual path:");
    println!("    appctl init");
    println!("    appctl sync --openapi <url-or-file> --base-url <running-api-url>");
    println!("    appctl doctor --write");
    println!("    appctl chat");
}

fn prompt_string(prompt: &str, default: Option<&str>) -> Result<String> {
    let mut input = Input::<String>::new().with_prompt(prompt.to_string());
    if let Some(default) = default {
        input = input.default(default.to_string());
    }
    input.interact_text().context("prompt failed")
}

fn prompt_optional(prompt: &str) -> Result<Option<String>> {
    let value = Input::<String>::new()
        .with_prompt(prompt.to_string())
        .allow_empty(true)
        .interact_text()
        .context("prompt failed")?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

fn prompt_auth_header_optional(prompt: &str) -> Result<Option<String>> {
    loop {
        let Some(value) = prompt_optional(prompt)? else {
            return Ok(None);
        };
        if auth_header_looks_truncated(&value) {
            print_status_warn(
                "that auth header looks truncated (`...`). Paste the full header or use env:, e.g. Authorization: Bearer env:TOKEN.",
            );
            continue;
        }
        return Ok(Some(value));
    }
}

fn auth_header_looks_truncated(value: &str) -> bool {
    value.contains("...") || value.contains('…')
}

fn prompt_string_nonempty(prompt: &str, default: &str) -> Result<String> {
    let value = Input::<String>::new()
        .with_prompt(prompt.to_string())
        .default(default.to_string())
        .interact_text()
        .context("prompt failed")?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

fn prompt_path(prompt: &str, default: Option<&str>) -> Result<PathBuf> {
    prompt_string(prompt, default).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::{
        ConfigPaths, SetupSourceChoice, auth_header_looks_truncated, detect_sync_sources,
        find_openapi_spec_files, inspection_project_root_inner,
    };
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn setup_source_choice_maps_known_indexes() {
        assert_eq!(
            SetupSourceChoice::from_index(0),
            SetupSourceChoice::AutoDetect
        );
        assert_eq!(SetupSourceChoice::from_index(1), SetupSourceChoice::OpenApi);
        assert_eq!(
            SetupSourceChoice::from_index(2),
            SetupSourceChoice::Database
        );
        assert_eq!(
            SetupSourceChoice::from_index(11),
            SetupSourceChoice::NotSure
        );
    }

    #[test]
    fn doctor_runs_only_for_http_like_sources() {
        assert!(SetupSourceChoice::OpenApi.is_http_like());
        assert!(SetupSourceChoice::Django.is_http_like());
        assert!(!SetupSourceChoice::Database.is_http_like());
        assert!(!SetupSourceChoice::SkipSync.is_http_like());
    }

    #[test]
    fn setup_detects_openapi_file_and_sqlite_db() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("openapi.json"), "{}").unwrap();
        fs::write(dir.path().join("app.sqlite"), "").unwrap();

        let found = detect_sync_sources(dir.path());

        assert!(
            found
                .iter()
                .any(|candidate| candidate.source == SetupSourceChoice::OpenApi)
        );
        assert!(
            found
                .iter()
                .any(|candidate| candidate.source == SetupSourceChoice::Database)
        );
    }

    #[test]
    fn setup_detects_framework_markers() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("manage.py"), "").unwrap();
        fs::create_dir_all(dir.path().join("config")).unwrap();
        fs::write(dir.path().join("config").join("routes.rb"), "").unwrap();

        let found = detect_sync_sources(dir.path());

        assert!(
            found
                .iter()
                .any(|candidate| candidate.source == SetupSourceChoice::Django)
        );
        assert!(
            found
                .iter()
                .any(|candidate| candidate.source == SetupSourceChoice::Rails)
        );
    }

    #[test]
    fn inspection_root_uses_cwd_when_app_dir_is_home_dot_appctl() {
        let home = PathBuf::from("/Users/person");
        let paths = ConfigPaths::new(home.join(".appctl"));
        let cwd = home.join("open").join("source").join("Quorum");
        let root = inspection_project_root_inner(&paths, Some(home.as_path()), &cwd);
        assert_eq!(root, cwd);
    }

    #[test]
    fn inspection_root_uses_project_parent_for_nested_appctl() {
        let home = PathBuf::from("/Users/person");
        let project = home.join("repos").join("myapp");
        let paths = ConfigPaths::new(project.join(".appctl"));
        let cwd = home.join("somewhere-else");
        let root = inspection_project_root_inner(&paths, Some(home.as_path()), &cwd);
        assert_eq!(root, project);
    }

    #[test]
    fn openapi_spec_search_prefers_shallowest_file() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("nested")).unwrap();
        fs::write(dir.path().join("nested").join("openapi.json"), "{}").unwrap();
        fs::write(dir.path().join("openapi.json"), "{}").unwrap();
        let found = find_openapi_spec_files(dir.path());
        assert_eq!(found[0], dir.path().join("openapi.json"));
    }

    #[test]
    fn truncated_auth_headers_are_rejected() {
        assert!(auth_header_looks_truncated("Authorization: Bearer abc..."));
        assert!(auth_header_looks_truncated("Authorization: Bearer abc…"));
        assert!(!auth_header_looks_truncated(
            "Authorization: Bearer env:TASK_API_TOKEN"
        ));
    }
}
