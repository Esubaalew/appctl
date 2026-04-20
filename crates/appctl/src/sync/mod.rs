use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;

use crate::{
    config::{ConfigPaths, read_json, write_json},
    schema::{Schema, SyncSource},
    tools::{ToolDef, schema_to_tools},
};

pub mod aspnet;
pub mod db;
pub mod django;
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
    paths.ensure()?;

    if !request.force && paths.schema.exists() {
        tracing::info!("overwriting existing schema at {}", paths.schema.display());
    }

    let mut schema = if let Some(source) = &request.openapi {
        openapi::OpenApiSync::new(source.clone())
            .introspect()
            .await?
    } else if let Some(path) = &request.django {
        django::DjangoSync::new(path.clone(), request.base_url.clone())
            .introspect()
            .await?
    } else if let Some(connection_string) = &request.db {
        db::DbSync::new(connection_string.clone())
            .introspect()
            .await?
    } else if let Some(source_url) = &request.url {
        url::UrlSync::new(source_url.clone(), &paths, &request)?
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
            "choose one sync source: --openapi, --django, --db, --url, --mcp, --rails, --laravel, --aspnet, --strapi, --supabase"
        );
    };

    if request.base_url.is_some() {
        schema.base_url = request.base_url.clone();
    }
    if let Some(header) = request.auth_header {
        schema
            .metadata
            .insert("auth_header".to_string(), serde_json::Value::String(header));
    }

    let tools = schema_to_tools(&schema);
    write_json(&paths.schema, &schema)?;
    write_json(&paths.tools, &tools)?;

    println!(
        "Synced {:?}: {} resources, {} tools written to {}",
        schema.source,
        schema.resources.len(),
        tools.len(),
        paths.root.display()
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

pub fn source_name(source: &SyncSource) -> &'static str {
    match source {
        SyncSource::Openapi => "openapi",
        SyncSource::Django => "django",
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
