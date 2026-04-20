//! `--rails <path>` sync: parse `db/schema.rb` + `config/routes.rb`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use regex::Regex;
use serde_json::Map;

use crate::schema::{
    Action, AuthStrategy, Field, FieldType, HttpMethod, ParameterLocation, Resource, Safety,
    Schema, SyncSource, Transport, Verb,
};

use super::SyncPlugin;

pub struct RailsSync {
    root: PathBuf,
    base_url: Option<String>,
}

impl RailsSync {
    pub fn new(root: PathBuf, base_url: Option<String>) -> Self {
        Self { root, base_url }
    }
}

#[async_trait::async_trait]
impl SyncPlugin for RailsSync {
    async fn introspect(&self) -> Result<Schema> {
        let schema_rb = self.root.join("db").join("schema.rb");
        let routes_rb = self.root.join("config").join("routes.rb");

        let mut resources = parse_schema_rb(&schema_rb).unwrap_or_default();
        let routed = parse_routes_rb(&routes_rb).unwrap_or_default();

        // For every `resources :foo` route, ensure the resource exists and has
        // the standard 5 REST actions.
        for resource_name in routed {
            let base_path = format!("/api/v1/{resource_name}");
            let resource = match resources.iter_mut().find(|r| r.name == resource_name) {
                Some(existing) => existing,
                None => {
                    resources.push(Resource {
                        name: resource_name.clone(),
                        description: None,
                        fields: Vec::new(),
                        actions: Vec::new(),
                    });
                    resources.last_mut().unwrap()
                }
            };
            attach_crud_actions(resource, &base_path);
        }

        Ok(Schema {
            source: SyncSource::Rails,
            base_url: self.base_url.clone(),
            auth: AuthStrategy::Bearer {
                env_ref: "RAILS_API_TOKEN".to_string(),
            },
            resources,
            metadata: Map::new(),
        })
    }
}

fn parse_schema_rb(path: &std::path::Path) -> Result<Vec<Resource>> {
    let src = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let table_re = Regex::new(r#"create_table\s+"([^"]+)""#)?;
    let column_re = Regex::new(
        r#"t\.(string|integer|bigint|float|decimal|boolean|text|datetime|date|json|jsonb|uuid)\s+"([^"]+)""#,
    )?;
    let mut resources = Vec::new();
    let mut current: Option<Resource> = None;
    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(caps) = table_re.captures(trimmed) {
            if let Some(prev) = current.take() {
                resources.push(prev);
            }
            current = Some(Resource {
                name: singularize(&caps[1]),
                description: None,
                fields: Vec::new(),
                actions: Vec::new(),
            });
            continue;
        }
        if trimmed.starts_with("end")
            && let Some(prev) = current.take()
        {
            resources.push(prev);
            continue;
        }
        if let Some(resource) = current.as_mut()
            && let Some(caps) = column_re.captures(trimmed)
        {
            let kind = &caps[1];
            let name = caps[2].to_string();
            resource.fields.push(Field {
                name,
                description: None,
                field_type: rails_type(kind),
                required: false,
                location: Some(ParameterLocation::Body),
                default: None,
                enum_values: Vec::new(),
            });
        }
    }
    if let Some(prev) = current.take() {
        resources.push(prev);
    }
    Ok(resources)
}

fn parse_routes_rb(path: &std::path::Path) -> Result<Vec<String>> {
    let src = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let re = Regex::new(r#"resources\s+:([a-z_][a-z0-9_]*)"#)?;
    let names = re
        .captures_iter(&src)
        .map(|c| singularize(&c[1]))
        .collect::<Vec<_>>();
    Ok(dedup(names))
}

pub(crate) fn attach_crud_actions(resource: &mut Resource, base_path: &str) {
    let rn = resource.name.clone();
    let plural = pluralize(&rn);
    let base = base_path.replace("{resource}", &plural);
    let actions = [
        (
            Verb::List,
            format!("{plural}_list"),
            HttpMethod::GET,
            base.clone(),
            Safety::ReadOnly,
        ),
        (
            Verb::Get,
            format!("{rn}_get"),
            HttpMethod::GET,
            format!("{base}/{{id}}"),
            Safety::ReadOnly,
        ),
        (
            Verb::Create,
            format!("{rn}_create"),
            HttpMethod::POST,
            base.clone(),
            Safety::Mutating,
        ),
        (
            Verb::Update,
            format!("{rn}_update"),
            HttpMethod::PATCH,
            format!("{base}/{{id}}"),
            Safety::Mutating,
        ),
        (
            Verb::Delete,
            format!("{rn}_delete"),
            HttpMethod::DELETE,
            format!("{base}/{{id}}"),
            Safety::Destructive,
        ),
    ];
    for (verb, name, method, path, safety) in actions {
        if resource.actions.iter().any(|a| a.name == name) {
            continue;
        }
        let mut parameters = Vec::new();
        if path.contains("{id}") {
            parameters.push(Field {
                name: "id".to_string(),
                description: None,
                field_type: FieldType::String,
                required: true,
                location: Some(ParameterLocation::Path),
                default: None,
                enum_values: Vec::new(),
            });
        }
        if matches!(verb, Verb::Create | Verb::Update) {
            for f in &resource.fields {
                let mut p = f.clone();
                p.location = Some(ParameterLocation::Body);
                parameters.push(p);
            }
        }
        resource.actions.push(Action {
            name,
            description: None,
            verb,
            transport: Transport::Http {
                method,
                path,
                query: Vec::new(),
            },
            parameters,
            safety,
            resource: Some(rn.clone()),
            metadata: Map::new(),
        });
    }
}

fn rails_type(kind: &str) -> FieldType {
    match kind {
        "integer" | "bigint" => FieldType::Integer,
        "float" | "decimal" => FieldType::Number,
        "boolean" => FieldType::Boolean,
        "datetime" => FieldType::DateTime,
        "date" => FieldType::Date,
        "json" | "jsonb" => FieldType::Json,
        "uuid" => FieldType::Uuid,
        _ => FieldType::String,
    }
}

pub(crate) fn singularize_public(name: &str) -> String {
    singularize(name)
}

fn singularize(name: &str) -> String {
    let lower = name.to_string();
    if let Some(stripped) = lower.strip_suffix("ies") {
        return format!("{stripped}y");
    }
    if let Some(stripped) = lower.strip_suffix('s') {
        return stripped.to_string();
    }
    lower
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

fn dedup(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v.dedup();
    v
}
