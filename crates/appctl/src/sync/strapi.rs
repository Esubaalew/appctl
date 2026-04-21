//! `--strapi <path>` sync: walk `src/api/<name>/content-types/<name>/schema.json`.

use std::path::PathBuf;

use anyhow::Result;
use serde_json::{Map, Value};
use walkdir::WalkDir;

use crate::schema::{
    Action, AuthStrategy, Field, FieldType, HttpMethod, ParameterLocation, Resource, Safety,
    Schema, SyncSource, Transport, Verb,
};

use super::SyncPlugin;

pub struct StrapiSync {
    root: PathBuf,
    base_url: Option<String>,
}

impl StrapiSync {
    pub fn new(root: PathBuf, base_url: Option<String>) -> Self {
        Self { root, base_url }
    }
}

#[async_trait::async_trait]
impl SyncPlugin for StrapiSync {
    async fn introspect(&self) -> Result<Schema> {
        let api_root = self.root.join("src").join("api");
        let mut resources = Vec::new();
        if api_root.exists() {
            for entry in WalkDir::new(&api_root)
                .into_iter()
                .filter_map(std::result::Result::ok)
            {
                let path = entry.path();
                if path.file_name().and_then(|n| n.to_str()) != Some("schema.json") {
                    continue;
                }
                if let Some(resource) = parse_schema_file(path)? {
                    resources.push(resource);
                }
            }
        }

        Ok(Schema {
            source: SyncSource::Strapi,
            base_url: self.base_url.clone(),
            auth: AuthStrategy::Bearer {
                env_ref: "STRAPI_API_TOKEN".to_string(),
            },
            resources,
            metadata: Map::new(),
        })
    }
}

fn parse_schema_file(path: &std::path::Path) -> Result<Option<Resource>> {
    let raw = std::fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&raw)?;
    let info = value.get("info").and_then(Value::as_object);
    let Some(info) = info else {
        return Ok(None);
    };
    let singular = info
        .get("singularName")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let plural_fallback = if singular.is_empty() {
        "items"
    } else {
        singular.as_str()
    };
    let plural = info
        .get("pluralName")
        .and_then(Value::as_str)
        .unwrap_or(plural_fallback)
        .to_string();
    if singular.is_empty() {
        return Ok(None);
    }

    let mut fields = Vec::new();
    if let Some(attrs) = value.get("attributes").and_then(Value::as_object) {
        for (name, attr) in attrs {
            let ty = attr
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("string")
                .to_string();
            let required = attr
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            fields.push(Field {
                name: name.clone(),
                description: None,
                field_type: strapi_type(&ty),
                required,
                location: Some(ParameterLocation::Body),
                default: None,
                enum_values: Vec::new(),
            });
        }
    }

    let base_path = format!("/api/{plural}");
    let actions = crud_actions(&singular, &base_path, &fields);
    Ok(Some(Resource {
        name: singular,
        description: Some(format!("Strapi content-type at {}", path.display())),
        fields,
        actions,
    }))
}

fn crud_actions(name: &str, base: &str, fields: &[Field]) -> Vec<Action> {
    let safety_read = Safety::ReadOnly;
    let safety_write = Safety::Mutating;
    let safety_del = Safety::Destructive;
    let id_param = Field {
        name: "id".to_string(),
        description: None,
        field_type: FieldType::String,
        required: true,
        location: Some(ParameterLocation::Path),
        default: None,
        enum_values: Vec::new(),
    };
    let data_params: Vec<Field> = fields
        .iter()
        .cloned()
        .map(|mut f| {
            f.location = Some(ParameterLocation::Body);
            f
        })
        .collect();
    vec![
        Action {
            name: format!("{name}_list"),
            description: None,
            verb: Verb::List,
            transport: Transport::Http {
                method: HttpMethod::GET,
                path: base.to_string(),
                query: Vec::new(),
            },
            parameters: Vec::new(),
            safety: safety_read.clone(),
            resource: Some(name.to_string()),
            metadata: Map::new(),
        },
        Action {
            name: format!("{name}_get"),
            description: None,
            verb: Verb::Get,
            transport: Transport::Http {
                method: HttpMethod::GET,
                path: format!("{base}/{{id}}"),
                query: Vec::new(),
            },
            parameters: vec![id_param.clone()],
            safety: safety_read,
            resource: Some(name.to_string()),
            metadata: Map::new(),
        },
        Action {
            name: format!("{name}_create"),
            description: None,
            verb: Verb::Create,
            transport: Transport::Http {
                method: HttpMethod::POST,
                path: base.to_string(),
                query: Vec::new(),
            },
            parameters: data_params.clone(),
            safety: safety_write.clone(),
            resource: Some(name.to_string()),
            metadata: Map::new(),
        },
        Action {
            name: format!("{name}_update"),
            description: None,
            verb: Verb::Update,
            transport: Transport::Http {
                method: HttpMethod::PUT,
                path: format!("{base}/{{id}}"),
                query: Vec::new(),
            },
            parameters: {
                let mut p = vec![id_param.clone()];
                p.extend(data_params);
                p
            },
            safety: safety_write,
            resource: Some(name.to_string()),
            metadata: Map::new(),
        },
        Action {
            name: format!("{name}_delete"),
            description: None,
            verb: Verb::Delete,
            transport: Transport::Http {
                method: HttpMethod::DELETE,
                path: format!("{base}/{{id}}"),
                query: Vec::new(),
            },
            parameters: vec![id_param],
            safety: safety_del,
            resource: Some(name.to_string()),
            metadata: Map::new(),
        },
    ]
}

fn strapi_type(ty: &str) -> FieldType {
    match ty {
        "integer" | "biginteger" => FieldType::Integer,
        "decimal" | "float" => FieldType::Number,
        "boolean" => FieldType::Boolean,
        "date" => FieldType::Date,
        "datetime" | "time" => FieldType::DateTime,
        "json" => FieldType::Json,
        "uid" => FieldType::String,
        _ => FieldType::String,
    }
}
