use std::{collections::BTreeMap, fs, path::PathBuf};

use anyhow::{Context, Result};
use regex::Regex;
use serde_json::{Map, json};
use walkdir::WalkDir;

use crate::schema::{
    Action, AuthStrategy, Field, FieldType, HttpMethod, ParameterLocation, Provenance, Resource,
    Safety, Schema, SyncSource, Transport, Verb,
};

use super::SyncPlugin;

pub struct DjangoSync {
    root: PathBuf,
    base_url: Option<String>,
}

impl DjangoSync {
    pub fn new(root: PathBuf, base_url: Option<String>) -> Self {
        Self { root, base_url }
    }
}

#[async_trait::async_trait]
impl SyncPlugin for DjangoSync {
    async fn introspect(&self) -> Result<Schema> {
        let drf = django_looks_like_drf(&self.root);
        let resources = parse_models(&self.root)?;
        let routes = parse_urls(&self.root)?;

        let resources = resources
            .into_iter()
            .map(|mut resource| {
                if drf {
                    let route = routes
                        .get(&resource.name)
                        .cloned()
                        .unwrap_or_else(|| format!("/api/{}/", resource.name));
                    resource.actions =
                        standard_drf_actions(&resource.name, &route, &resource.fields);
                } else {
                    resource.actions = Vec::new();
                }
                resource
            })
            .collect();

        let mut metadata = Map::new();
        if !drf {
            metadata.insert(
                "warnings".to_string(),
                json!(["No Django REST Framework detected in project settings (missing `rest_framework`). HTTP tools were not generated — add DRF + API routes and re-sync, or use `appctl sync --url` for server-rendered Django apps."]),
            );
        }

        Ok(Schema {
            source: SyncSource::Django,
            base_url: self.base_url.clone(),
            auth: AuthStrategy::Bearer {
                env_ref: "django_api_token".to_string(),
            },
            resources,
            metadata,
        })
    }
}

/// True if any `settings.py` (or `*/settings/*.py`) mentions `rest_framework`.
fn django_looks_like_drf(root: &PathBuf) -> bool {
    for entry in WalkDir::new(root).max_depth(10) {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("py") {
            continue;
        }
        let rel = path.strip_prefix(root).ok();
        let rel_str = rel.map(|p| p.to_string_lossy()).unwrap_or_default();
        if rel_str.contains("__pycache__") || rel_str.contains("migrations") {
            continue;
        }
        let is_settings =
            path.file_name().is_some_and(|n| n == "settings.py") || rel_str.contains("/settings/");
        if !is_settings {
            continue;
        }
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        if content.contains("rest_framework") {
            return true;
        }
    }
    false
}

fn parse_models(root: &PathBuf) -> Result<Vec<Resource>> {
    let class_re = Regex::new(r"class\s+([A-Za-z0-9_]+)\s*\(([^)]*)\)\s*:")?;
    let field_re =
        Regex::new(r"^\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*=\s*models\.([A-Za-z0-9_]+)\((.*)\)")?;

    let mut resources = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name() == "models.py")
    {
        let content = fs::read_to_string(entry.path())
            .with_context(|| format!("failed to read {}", entry.path().display()))?;

        let mut current: Option<Resource> = None;
        for line in content.lines() {
            if let Some(caps) = class_re.captures(line) {
                if let Some(resource) = current.take() {
                    resources.push(resource);
                }
                let parents = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
                if parents.contains("models.Model") {
                    current = Some(Resource {
                        name: to_resource_name(caps.get(1).unwrap().as_str()),
                        description: None,
                        fields: Vec::new(),
                        actions: Vec::new(),
                    });
                } else {
                    current = None;
                }
                continue;
            }

            if let Some(resource) = current.as_mut()
                && let Some(caps) = field_re.captures(line)
            {
                let field_name = caps.get(1).unwrap().as_str();
                let field_kind = caps.get(2).unwrap().as_str();
                let args = caps.get(3).map(|m| m.as_str()).unwrap_or_default();
                resource.fields.push(Field {
                    name: field_name.to_string(),
                    description: None,
                    field_type: django_field_type(field_kind),
                    required: !args.contains("blank=True") && !args.contains("null=True"),
                    location: Some(ParameterLocation::Body),
                    default: None,
                    enum_values: Vec::new(),
                });
            }
        }

        if let Some(resource) = current.take() {
            resources.push(resource);
        }
    }

    Ok(resources)
}

fn parse_urls(root: &PathBuf) -> Result<BTreeMap<String, String>> {
    let path_re = Regex::new(r#"path\(\s*"([^"]+)""#)?;
    let router_re = Regex::new(r#"router\.register\(\s*r?"([^"]+)"#)?;

    let mut routes = BTreeMap::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name() == "urls.py")
    {
        let content = fs::read_to_string(entry.path())
            .with_context(|| format!("failed to read {}", entry.path().display()))?;
        for caps in path_re.captures_iter(&content) {
            let route = caps.get(1).unwrap().as_str().trim_matches('/').to_string();
            let key = singularize(route.split('/').next().unwrap_or("resource"));
            routes.entry(key).or_insert_with(|| format!("/{route}/"));
        }
        for caps in router_re.captures_iter(&content) {
            let route = caps.get(1).unwrap().as_str().trim_matches('/').to_string();
            let key = singularize(route.split('/').next().unwrap_or("resource"));
            routes.entry(key).or_insert_with(|| format!("/{route}/"));
        }
    }

    Ok(routes)
}

fn standard_drf_actions(resource: &str, route: &str, fields: &[Field]) -> Vec<Action> {
    let mut by_id = vec![Field {
        name: "id".to_string(),
        description: Some("Primary key".to_string()),
        field_type: FieldType::Integer,
        required: true,
        location: Some(ParameterLocation::Path),
        default: None,
        enum_values: Vec::new(),
    }];
    let body_fields: Vec<Field> = fields
        .iter()
        .filter(|field| field.name != "id")
        .cloned()
        .collect();

    vec![
        Action {
            name: format!("list_{}s", resource),
            description: Some(format!("List {} records", resource)),
            verb: Verb::List,
            transport: Transport::Http {
                method: HttpMethod::GET,
                path: route.to_string(),
                query: Vec::new(),
            },
            parameters: Vec::new(),
            safety: Safety::ReadOnly,
            resource: Some(resource.to_string()),
            provenance: Provenance::Inferred,
            metadata: Map::new(),
        },
        Action {
            name: format!("get_{resource}"),
            description: Some(format!("Get one {}", resource)),
            verb: Verb::Get,
            transport: Transport::Http {
                method: HttpMethod::GET,
                path: format!("{}/{{id}}/", route.trim_end_matches('/')),
                query: Vec::new(),
            },
            parameters: by_id.clone(),
            safety: Safety::ReadOnly,
            resource: Some(resource.to_string()),
            provenance: Provenance::Inferred,
            metadata: Map::new(),
        },
        Action {
            name: format!("create_{resource}"),
            description: Some(format!("Create one {}", resource)),
            verb: Verb::Create,
            transport: Transport::Http {
                method: HttpMethod::POST,
                path: route.to_string(),
                query: Vec::new(),
            },
            parameters: body_fields.clone(),
            safety: Safety::Mutating,
            resource: Some(resource.to_string()),
            provenance: Provenance::Inferred,
            metadata: Map::new(),
        },
        Action {
            name: format!("update_{resource}"),
            description: Some(format!("Update one {}", resource)),
            verb: Verb::Update,
            transport: Transport::Http {
                method: HttpMethod::PATCH,
                path: format!("{}/{{id}}/", route.trim_end_matches('/')),
                query: Vec::new(),
            },
            parameters: {
                let mut params = by_id.clone();
                params.extend(body_fields.clone());
                params
            },
            safety: Safety::Mutating,
            resource: Some(resource.to_string()),
            provenance: Provenance::Inferred,
            metadata: Map::new(),
        },
        Action {
            name: format!("delete_{resource}"),
            description: Some(format!("Delete one {}", resource)),
            verb: Verb::Delete,
            transport: Transport::Http {
                method: HttpMethod::DELETE,
                path: format!("{}/{{id}}/", route.trim_end_matches('/')),
                query: Vec::new(),
            },
            parameters: by_id.split_off(0),
            safety: Safety::Destructive,
            resource: Some(resource.to_string()),
            provenance: Provenance::Inferred,
            metadata: Map::new(),
        },
    ]
}

fn django_field_type(field_kind: &str) -> FieldType {
    match field_kind {
        "IntegerField" | "BigIntegerField" | "AutoField" => FieldType::Integer,
        "DecimalField" | "FloatField" => FieldType::Number,
        "BooleanField" => FieldType::Boolean,
        "DateTimeField" => FieldType::DateTime,
        "DateField" => FieldType::Date,
        "JSONField" => FieldType::Json,
        "ForeignKey" => FieldType::Integer,
        _ => FieldType::String,
    }
}

fn to_resource_name(name: &str) -> String {
    singularize(&name.to_lowercase())
}

fn singularize(name: &str) -> String {
    name.trim_end_matches('s').to_string()
}
