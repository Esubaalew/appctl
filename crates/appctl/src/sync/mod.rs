use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::PathBuf,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;

use crate::{
    config::{AppConfig, ConfigPaths, read_json, write_json},
    doctor::{DoctorRunArgs, run_doctor},
    schema::{Schema, SyncSource},
    term::{print_path_row, print_section_title, print_status_success, print_tip},
    tools::{ToolDef, schema_to_tools},
};

pub mod aspnet;
pub mod db;
pub mod django;
pub mod flask;
pub mod laravel;
pub mod mcp;
pub mod openapi;
pub mod rails;
pub mod strapi;
pub mod supabase;
pub mod url;

#[derive(Debug, Clone, Default)]
pub struct SyncRequest {
    pub openapi: Option<String>,
    pub django: Option<PathBuf>,
    pub flask: Option<PathBuf>,
    pub db: Option<String>,
    pub url: Option<String>,
    pub mcp: Option<String>,
    pub rails: Option<PathBuf>,
    pub laravel: Option<PathBuf>,
    pub aspnet: Option<PathBuf>,
    pub strapi: Option<PathBuf>,
    pub supabase: Option<String>,
    pub supabase_anon_ref: Option<String>,
    pub auth_header: Option<String>,
    pub base_url: Option<String>,
    pub force: bool,
    pub watch: bool,
    pub watch_interval_secs: u64,
    pub doctor_write: bool,
    pub login_url: Option<String>,
    pub login_user: Option<String>,
    pub login_password: Option<String>,
    pub login_form_selector: Option<String>,
}

#[async_trait]
pub trait SyncPlugin {
    async fn introspect(&self) -> Result<Schema>;
}

pub async fn run_sync(paths: ConfigPaths, request: SyncRequest) -> Result<()> {
    if request.watch {
        return run_sync_watch(paths, request).await;
    }
    run_sync_once(paths, &request).await
}

async fn run_sync_once(paths: ConfigPaths, request: &SyncRequest) -> Result<()> {
    paths.ensure()?;

    if !request.force && paths.schema.exists() {
        bail!(
            ".appctl/schema.json already exists (pass --force to overwrite it and regenerate .appctl/tools.json)"
        );
    }

    let mut schema = if let Some(source) = &request.openapi {
        openapi::OpenApiSync::new(source.clone())
            .introspect()
            .await?
    } else if let Some(path) = &request.django {
        django::DjangoSync::new(path.clone(), request.base_url.clone())
            .introspect()
            .await?
    } else if let Some(path) = &request.flask {
        flask::FlaskSync::new(path.clone(), request.base_url.clone())
            .introspect()
            .await?
    } else if let Some(connection_string) = &request.db {
        db::DbSync::new(connection_string.clone())
            .introspect()
            .await?
    } else if let Some(source_url) = &request.url {
        url::UrlSync::new(source_url.clone(), &paths, request)?
            .introspect()
            .await?
    } else if let Some(server_url) = &request.mcp {
        mcp::McpSync::new(server_url.clone()).introspect().await?
    } else if let Some(path) = &request.rails {
        rails::RailsSync::new(path.clone(), request.base_url.clone())
            .introspect()
            .await?
    } else if let Some(path) = &request.laravel {
        laravel::LaravelSync::new(path.clone(), request.base_url.clone())
            .introspect()
            .await?
    } else if let Some(path) = &request.aspnet {
        aspnet::AspNetSync::new(path.clone(), request.base_url.clone())
            .introspect()
            .await?
    } else if let Some(path) = &request.strapi {
        strapi::StrapiSync::new(path.clone(), request.base_url.clone())
            .introspect()
            .await?
    } else if let Some(base) = &request.supabase {
        supabase::SupabaseSync::new(
            base.clone(),
            request
                .supabase_anon_ref
                .clone()
                .unwrap_or_else(|| "SUPABASE_ANON_KEY".to_string()),
        )
        .introspect()
        .await?
    } else {
        bail!(
            "choose one sync source: --openapi, --django, --flask, --db, --url, --mcp, --rails, --laravel, --aspnet, --strapi, --supabase"
        );
    };

    if request.base_url.is_some() {
        schema.base_url = request.base_url.clone();
    }
    if let Some(header) = &request.auth_header {
        schema.metadata.insert(
            "auth_header".to_string(),
            serde_json::Value::String(header.clone()),
        );
    }

    let tools = schema_to_tools(&schema);
    write_json(&paths.schema, &schema)?;
    write_json(&paths.tools, &tools)?;

    if let Some(conn) = &request.db {
        merge_target_database_url_from_sync(&paths, conn)?;
    }

    print_section_title("Sync complete");
    print_path_row("app directory", &paths.root);
    print_status_success(&format!(
        "{:?}: {} resources, {} tools written under .appctl",
        schema.source,
        schema.resources.len(),
        tools.len()
    ));
    if !paths.config.exists() {
        print_tip(&format!(
            "No provider config at {} yet — run `appctl init` (or `appctl --app-dir {} init`) before chat/run.",
            paths.config.display(),
            paths.root.display()
        ));
    }
    if request.doctor_write && paths.config.exists() {
        print_tip("Running `appctl doctor --write` after sync.");
        run_doctor(
            &paths,
            DoctorRunArgs {
                write: true,
                timeout_secs: 5,
            },
        )
        .await?;
    }

    Ok(())
}

async fn run_sync_watch(paths: ConfigPaths, request: SyncRequest) -> Result<()> {
    let Some(source) = request.openapi.as_deref() else {
        bail!("`appctl sync --watch` currently supports only `--openapi` sources");
    };

    let interval_secs = request.watch_interval_secs.max(1);
    print_tip(&format!(
        "watching OpenAPI source for changes every {interval_secs}s — press Ctrl+C to stop"
    ));

    let mut last_hash: Option<u64> = None;
    loop {
        let raw = openapi::load_openapi_source(source).await?;
        let next_hash = stable_hash(&raw);
        if last_hash != Some(next_hash) {
            run_sync_once(paths.clone(), &request).await?;
            last_hash = Some(next_hash);
        }
        tokio::time::sleep(Duration::from_secs(interval_secs)).await;
    }
}

fn stable_hash(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// `appctl sync --db` uses a connection string for introspection, but `appctl chat` / run read
/// [`AppConfig::target::database_url`](crate::config::TargetConfig::database_url). If that is
/// unset, copy the sync string so DB tools work without a second manual copy.
fn merge_target_database_url_from_sync(paths: &ConfigPaths, connection_string: &str) -> Result<()> {
    let mut config = AppConfig::load_or_init(paths)?;
    let missing = config
        .target
        .database_url
        .as_deref()
        .map(str::trim)
        .is_none_or(|s| s.is_empty());
    if !missing {
        return Ok(());
    }
    config.target.database_url = Some(connection_string.to_string());
    config.save(paths)?;
    print_tip(
        "Set [target] database_url from this `sync --db` connection (required for DB tool calls in chat/run).",
    );
    Ok(())
}

pub fn load_schema(paths: &ConfigPaths) -> Result<Schema> {
    read_json(&paths.schema).with_context(|| {
        format!(
            "failed to load schema; run `appctl sync` first ({})",
            paths.schema.display()
        )
    })
}

pub fn load_tools(paths: &ConfigPaths) -> Result<Vec<ToolDef>> {
    read_json(&paths.tools).with_context(|| {
        format!(
            "failed to load tools; run `appctl sync` first ({})",
            paths.tools.display()
        )
    })
}

pub fn load_runtime_tools(paths: &ConfigPaths, config: &AppConfig) -> Result<Vec<ToolDef>> {
    let tools = load_tools(paths)?;
    let pinned = if config.tooling.pin.is_empty() {
        None
    } else {
        Some(
            config
                .tooling
                .pin
                .iter()
                .map(|name| config.resolve_tool_name(name).to_string())
                .collect::<std::collections::BTreeSet<_>>(),
        )
    };

    let mut runtime_tools = tools
        .into_iter()
        .filter(|tool| {
            pinned
                .as_ref()
                .is_none_or(|names| names.contains(&tool.name))
        })
        .collect::<Vec<_>>();

    for (alias, canonical) in &config.tooling.aliases {
        if let Some(tool) = runtime_tools
            .iter()
            .find(|tool| tool.name == *canonical)
            .cloned()
        {
            runtime_tools.push(ToolDef {
                name: alias.clone(),
                description: format!("Alias for {}", tool.name),
                input_schema: tool.input_schema,
            });
        }
    }
    Ok(runtime_tools)
}

pub fn source_name(source: &SyncSource) -> &'static str {
    match source {
        SyncSource::Openapi => "openapi",
        SyncSource::Django => "django",
        SyncSource::Flask => "flask",
        SyncSource::Db => "db",
        SyncSource::Url => "url",
        SyncSource::Mcp => "mcp",
        SyncSource::Rails => "rails",
        SyncSource::Laravel => "laravel",
        SyncSource::Aspnet => "aspnet",
        SyncSource::Strapi => "strapi",
        SyncSource::Supabase => "supabase",
        SyncSource::Plugin => "plugin",
    }
}
