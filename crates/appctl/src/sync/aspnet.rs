//! `--aspnet <path>` sync: delegate to OpenAPI if a swagger JSON file is
//! shipped; otherwise scan `*.cs` controllers for `[ApiController]`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use regex::Regex;
use serde_json::Map;
use walkdir::WalkDir;

use crate::schema::{
    Action, AuthStrategy, Field, FieldType, HttpMethod, ParameterLocation, Resource, Safety,
    Schema, SyncSource, Transport, Verb,
};

use super::{SyncPlugin, openapi::OpenApiSync};

pub struct AspNetSync {
    root: PathBuf,
    base_url: Option<String>,
}

impl AspNetSync {
    pub fn new(root: PathBuf, base_url: Option<String>) -> Self {
        Self { root, base_url }
    }
}

#[async_trait::async_trait]
impl SyncPlugin for AspNetSync {
    async fn introspect(&self) -> Result<Schema> {
        if let Some(swagger) = find_swagger(&self.root) {
            tracing::info!("delegating to OpenAPI sync via {}", swagger.display());
            let mut schema = OpenApiSync::new(swagger.display().to_string())
                .introspect()
                .await?;
            schema.source = SyncSource::Aspnet;
            if self.base_url.is_some() {
                schema.base_url = self.base_url.clone();
            }
            return Ok(schema);
        }

        let mut resources = Vec::new();
        for entry in WalkDir::new(&self.root)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().extension().and_then(|e| e.to_str()) == Some("cs"))
        {
            if let Some(resource) = parse_controller(entry.path()).transpose()? {
                resources.push(resource);
            }
        }

        Ok(Schema {
            source: SyncSource::Aspnet,
            base_url: self.base_url.clone(),
            auth: AuthStrategy::Bearer {
                env_ref: "ASPNET_API_TOKEN".to_string(),
            },
            resources,
            metadata: Map::new(),
        })
    }
}

fn find_swagger(root: &std::path::Path) -> Option<PathBuf> {
    for entry in WalkDir::new(root).max_depth(4).into_iter().flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if matches!(name.as_str(), "swagger.json" | "openapi.json") {
            return Some(entry.into_path());
        }
    }
    None
}

fn parse_controller(path: &std::path::Path) -> Option<Result<Resource>> {
    let src = std::fs::read_to_string(path).ok()?;
    if !src.contains("[ApiController]") {
        return None;
    }
    Some(do_parse_controller(&src, path))
}

fn do_parse_controller(src: &str, path: &std::path::Path) -> Result<Resource> {
    let route_re = Regex::new(r#"\[Route\("([^"]+)"\)\]"#)?;
    let controller_re = Regex::new(r#"class\s+(\w+Controller)"#)?;
    let method_re = Regex::new(
        r#"\[Http(Get|Post|Put|Patch|Delete)(?:\(\s*"([^"]*)"\s*\))?\][^\n]*\n[^\n]*\s+(\w+)\s*\("#,
    )?;
    let controller = controller_re
        .captures(src)
        .map(|c| c[1].to_string())
        .with_context(|| format!("no controller class in {}", path.display()))?;
    let resource_name = controller.trim_end_matches("Controller").to_string();
    let route_template = route_re
        .captures(src)
        .map(|c| c[1].to_string())
        .unwrap_or_else(|| format!("api/{}", resource_name.to_lowercase()));
    let base_path = route_template.replace("[controller]", &resource_name.to_lowercase());

    let mut actions = Vec::new();
    for caps in method_re.captures_iter(src) {
        let verb_raw = &caps[1];
        let sub_path = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
        let method_name = caps[3].to_string();
        let http_method = match verb_raw {
            "Get" => HttpMethod::GET,
            "Post" => HttpMethod::POST,
            "Put" => HttpMethod::PUT,
            "Patch" => HttpMethod::PATCH,
            "Delete" => HttpMethod::DELETE,
            _ => continue,
        };
        let verb = match verb_raw {
            "Get" if sub_path.is_empty() => Verb::List,
            "Get" => Verb::Get,
            "Post" => Verb::Create,
            "Put" | "Patch" => Verb::Update,
            "Delete" => Verb::Delete,
            _ => Verb::Custom,
        };
        let full_path = if sub_path.is_empty() {
            format!("/{}", base_path.trim_start_matches('/'))
        } else {
            format!(
                "/{}/{}",
                base_path.trim_start_matches('/'),
                sub_path.trim_start_matches('/')
            )
        };
        let mut params = Vec::new();
        if full_path.contains("{id}") || full_path.contains("{Id}") {
            params.push(Field {
                name: "id".to_string(),
                description: None,
                field_type: FieldType::String,
                required: true,
                location: Some(ParameterLocation::Path),
                default: None,
                enum_values: Vec::new(),
            });
        }
        actions.push(Action {
            name: format!(
                "{}_{}",
                resource_name.to_lowercase(),
                method_name.to_lowercase()
            ),
            description: None,
            verb,
            transport: Transport::Http {
                method: http_method,
                path: full_path,
                query: Vec::new(),
            },
            parameters: params,
            safety: match verb_raw {
                "Delete" => Safety::Destructive,
                "Get" => Safety::ReadOnly,
                _ => Safety::Mutating,
            },
            resource: Some(resource_name.clone()),
            metadata: Map::new(),
        });
    }

    Ok(Resource {
        name: resource_name,
        description: Some(format!("ASP.NET controller at {}", path.display())),
        fields: Vec::new(),
        actions,
    })
}
