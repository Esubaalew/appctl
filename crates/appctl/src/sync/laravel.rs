//! `--laravel <path>` sync: parse migrations + routes/api.php.

use std::path::PathBuf;

use anyhow::{Context, Result};
use regex::Regex;
use serde_json::{Map, json};

use crate::schema::{
    AuthStrategy, Field, FieldType, ParameterLocation, Resource, Schema, SyncSource,
};

use super::SyncPlugin;

pub struct LaravelSync {
    root: PathBuf,
    base_url: Option<String>,
}

impl LaravelSync {
    pub fn new(root: PathBuf, base_url: Option<String>) -> Self {
        Self { root, base_url }
    }
}

#[async_trait::async_trait]
impl SyncPlugin for LaravelSync {
    async fn introspect(&self) -> Result<Schema> {
        let migrations_dir = self.root.join("database").join("migrations");
        let routes = self.root.join("routes").join("api.php");

        let mut resources = parse_migrations(&migrations_dir).unwrap_or_default();
        let routed = parse_api_routes(&routes).unwrap_or_default();

        let mut metadata = Map::new();
        if routed.is_empty() && !resources.is_empty() {
            metadata.insert(
                "warnings".to_string(),
                json!(["No routes parsed from routes/api.php; HTTP tools were not generated. Define API routes or use OpenAPI sync."]),
            );
        }

        for name in routed {
            let base_path = format!("/api/{}", pluralize(&name));
            let resource = match resources.iter_mut().find(|r| r.name == name) {
                Some(r) => r,
                None => {
                    resources.push(Resource {
                        name: name.clone(),
                        description: None,
                        fields: Vec::new(),
                        actions: Vec::new(),
                    });
                    resources.last_mut().unwrap()
                }
            };
            super::rails::attach_crud_actions(resource, &base_path);
        }

        Ok(Schema {
            source: SyncSource::Laravel,
            base_url: self.base_url.clone(),
            auth: AuthStrategy::Bearer {
                env_ref: "LARAVEL_API_TOKEN".to_string(),
            },
            resources,
            metadata,
        })
    }
}

fn parse_migrations(dir: &std::path::Path) -> Result<Vec<Resource>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let table_re = Regex::new(r#"Schema::create\(\s*'([^']+)'"#)?;
    let col_re = Regex::new(
        r#"\$table->(string|integer|bigInteger|unsignedBigInteger|float|double|boolean|text|json|jsonb|date|dateTime|timestamp|uuid)\(\s*'([^']+)'"#,
    )?;
    let mut resources = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("php") {
            continue;
        }
        let src = std::fs::read_to_string(&path)?;
        let Some(table_caps) = table_re.captures(&src) else {
            continue;
        };
        let name = super::rails::singularize_public(&table_caps[1]);
        let mut fields = Vec::new();
        for caps in col_re.captures_iter(&src) {
            fields.push(Field {
                name: caps[2].to_string(),
                description: None,
                field_type: laravel_type(&caps[1]),
                required: false,
                location: Some(ParameterLocation::Body),
                default: None,
                enum_values: Vec::new(),
            });
        }
        resources.push(Resource {
            name,
            description: Some(format!("Laravel model from {}", path.display())),
            fields,
            actions: Vec::new(),
        });
    }
    Ok(resources)
}

fn parse_api_routes(path: &std::path::Path) -> Result<Vec<String>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let src = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let re = Regex::new(r#"Route::apiResource\(\s*'([^']+)'"#)?;
    Ok(re
        .captures_iter(&src)
        .map(|c| super::rails::singularize_public(&c[1]))
        .collect::<Vec<_>>())
}

fn laravel_type(kind: &str) -> FieldType {
    match kind {
        "integer" | "bigInteger" | "unsignedBigInteger" => FieldType::Integer,
        "float" | "double" => FieldType::Number,
        "boolean" => FieldType::Boolean,
        "date" => FieldType::Date,
        "dateTime" | "timestamp" => FieldType::DateTime,
        "json" | "jsonb" => FieldType::Json,
        "uuid" => FieldType::Uuid,
        _ => FieldType::String,
    }
}

fn pluralize(name: &str) -> String {
    if let Some(stem) = name.strip_suffix('y') {
        format!("{stem}ies")
    } else if name.ends_with('s') {
        name.to_string()
    } else {
        format!("{name}s")
    }
}
