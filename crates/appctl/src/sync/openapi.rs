use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow};
use serde_json::{Map, Value, json};

use crate::schema::{
    Action, AuthStrategy, Field, FieldType, HttpMethod, ParameterLocation, Resource, Safety,
    Schema, SyncSource, Transport, Verb,
};

use super::SyncPlugin;

pub struct OpenApiSync {
    source: String,
}

impl OpenApiSync {
    pub fn new(source: String) -> Self {
        Self { source }
    }
}

pub async fn load_openapi_source(source: &str) -> Result<String> {
    if source.starts_with("http://") || source.starts_with("https://") {
        reqwest::get(source)
            .await
            .with_context(|| format!("failed to fetch {}", source))?
            .text()
            .await
            .map_err(Into::into)
    } else {
        tokio::fs::read_to_string(source)
            .await
            .with_context(|| format!("failed to read {}", source))
    }
}

#[async_trait::async_trait]
impl SyncPlugin for OpenApiSync {
    async fn introspect(&self) -> Result<Schema> {
        let raw = load_openapi_source(&self.source).await?;

        let document: Value = serde_json::from_str(&raw)
            .or_else(|_| serde_yaml::from_str(&raw))
            .with_context(|| {
                format!(
                    "failed to parse OpenAPI or Swagger document from {}",
                    self.source
                )
            })?;

        let base_url = detect_base_url(&document);
        let auth = detect_auth(&document);
        let resources = build_resources(&document)?;

        Ok(Schema {
            source: SyncSource::Openapi,
            base_url,
            auth,
            resources,
            metadata: Map::new(),
        })
    }
}

fn build_resources(document: &Value) -> Result<Vec<Resource>> {
    let paths = document
        .get("paths")
        .and_then(Value::as_object)
        .context("document missing paths object")?;

    let mut grouped: BTreeMap<String, Resource> = BTreeMap::new();

    for (path, path_item) in paths {
        let path_parameters = path_item
            .get("parameters")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let path_item_obj = path_item
            .as_object()
            .ok_or_else(|| anyhow!("path item for '{}' is not an object", path))?;

        for (method_name, operation) in path_item_obj {
            let Some(method) = parse_method(method_name) else {
                continue;
            };

            let op = operation
                .as_object()
                .ok_or_else(|| anyhow!("operation {} {} is not an object", method_name, path))?;

            let resource_name = operation_resource_name(path, op);
            let resource = grouped
                .entry(resource_name.clone())
                .or_insert_with(|| Resource {
                    name: resource_name.clone(),
                    description: op
                        .get("summary")
                        .and_then(Value::as_str)
                        .map(ToString::to_string),
                    fields: Vec::new(),
                    actions: Vec::new(),
                });

            let parameters = collect_parameters(document, &path_parameters, op)?;
            let action_name = operation_name(path, method_name, op, &resource.name);

            let action = Action {
                name: action_name,
                description: op
                    .get("summary")
                    .or_else(|| op.get("description"))
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                verb: verb_from_method(&method),
                transport: Transport::Http {
                    method,
                    path: path.clone(),
                    query: parameters
                        .iter()
                        .filter(|field| matches!(field.location, Some(ParameterLocation::Query)))
                        .map(|field| field.name.clone())
                        .collect(),
                },
                parameters,
                safety: safety_from_method_name(method_name),
                resource: Some(resource.name.clone()),
                provenance: crate::schema::Provenance::Declared,
                metadata: Map::new(),
            };

            resource.actions.push(action);
        }
    }

    Ok(grouped.into_values().collect())
}

fn collect_parameters(
    document: &Value,
    path_parameters: &[Value],
    operation: &Map<String, Value>,
) -> Result<Vec<Field>> {
    let mut params = Vec::new();

    for parameter in path_parameters.iter().chain(
        operation
            .get("parameters")
            .and_then(Value::as_array)
            .into_iter()
            .flatten(),
    ) {
        params.push(parameter_to_field(document, parameter)?);
    }

    if let Some(body) = operation.get("requestBody") {
        params.extend(request_body_fields(document, body)?);
    } else if let Some(body_param) = operation.get("consumes").and_then(Value::as_array) {
        let _ = body_param;
    }

    Ok(dedup_fields(params))
}

fn parameter_to_field(document: &Value, parameter: &Value) -> Result<Field> {
    let resolved = resolve_ref(document, parameter);
    let name = resolved
        .get("name")
        .and_then(Value::as_str)
        .context("parameter missing name")?;
    let location = match resolved
        .get("in")
        .and_then(Value::as_str)
        .unwrap_or("query")
    {
        "path" => ParameterLocation::Path,
        "query" => ParameterLocation::Query,
        "header" => ParameterLocation::Header,
        _ => ParameterLocation::Body,
    };

    let schema = resolved.get("schema").unwrap_or(&Value::Null);
    let field = if schema.is_null() {
        Field {
            name: name.to_string(),
            description: resolved
                .get("description")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            field_type: FieldType::from_openapi_type(
                resolved.get("type").and_then(Value::as_str),
                resolved.get("format").and_then(Value::as_str),
            ),
            required: resolved
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            location: Some(location),
            default: None,
            enum_values: resolved
                .get("enum")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        }
    } else {
        schema_to_field(
            document,
            name,
            schema,
            Some(location),
            resolved.get("required"),
        )
    };

    Ok(field)
}

fn request_body_fields(document: &Value, body: &Value) -> Result<Vec<Field>> {
    let resolved = resolve_ref(document, body);
    let required = resolved
        .get("required")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let Some(content) = resolved.get("content").and_then(Value::as_object) else {
        return Ok(Vec::new());
    };

    let schema = content
        .get("application/json")
        .or_else(|| content.values().next())
        .and_then(|entry| entry.get("schema"))
        .unwrap_or(&Value::Null);

    if schema.is_null() {
        return Ok(Vec::new());
    }

    let schema = resolve_ref(document, schema);
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        let required_fields = schema
            .get("required")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut out = Vec::new();
        for (name, property) in properties {
            let mut field = schema_to_field(
                document,
                name,
                property,
                Some(ParameterLocation::Body),
                None,
            );
            field.required = required || required_fields.contains(&Value::String(name.clone()));
            out.push(field);
        }
        Ok(out)
    } else {
        Ok(vec![schema_to_field(
            document,
            "body",
            schema,
            Some(ParameterLocation::Body),
            Some(&Value::Bool(required)),
        )])
    }
}

fn schema_to_field(
    document: &Value,
    name: &str,
    schema: &Value,
    location: Option<ParameterLocation>,
    required: Option<&Value>,
) -> Field {
    let schema = resolve_ref(document, schema);
    let field_type = FieldType::from_openapi_type(
        schema.get("type").and_then(Value::as_str),
        schema.get("format").and_then(Value::as_str),
    );

    Field {
        name: name.to_string(),
        description: schema
            .get("description")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        field_type,
        required: required.and_then(Value::as_bool).unwrap_or(false),
        location,
        default: schema.get("default").cloned(),
        enum_values: schema
            .get("enum")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    }
}

fn dedup_fields(fields: Vec<Field>) -> Vec<Field> {
    let mut seen = BTreeMap::new();
    for field in fields {
        seen.insert(field.name.clone(), field);
    }
    seen.into_values().collect()
}

fn detect_base_url(document: &Value) -> Option<String> {
    document
        .get("servers")
        .and_then(Value::as_array)
        .and_then(|servers| servers.first())
        .and_then(|server| server.get("url"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            let host = document.get("host").and_then(Value::as_str)?;
            let base_path = document
                .get("basePath")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let scheme = document
                .get("schemes")
                .and_then(Value::as_array)
                .and_then(|schemes| schemes.first())
                .and_then(Value::as_str)
                .unwrap_or("https");
            Some(format!("{scheme}://{host}{base_path}"))
        })
}

fn detect_auth(document: &Value) -> AuthStrategy {
    let schemes = document
        .get("components")
        .and_then(|value| value.get("securitySchemes"))
        .or_else(|| document.get("securityDefinitions"))
        .and_then(Value::as_object);

    let Some(schemes) = schemes else {
        return AuthStrategy::None;
    };

    for (name, scheme) in schemes {
        match scheme
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "apiKey" => {
                let header = scheme
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Authorization")
                    .to_string();
                return AuthStrategy::ApiKey {
                    header,
                    env_ref: name.clone(),
                };
            }
            "http" if scheme.get("scheme").and_then(Value::as_str) == Some("bearer") => {
                return AuthStrategy::Bearer {
                    env_ref: name.clone(),
                };
            }
            "basic" => {
                return AuthStrategy::Basic {
                    username_ref: format!("{name}_username"),
                    password_ref: format!("{name}_password"),
                };
            }
            "oauth2" => {
                let auth_url = scheme
                    .pointer("/flows/authorizationCode/authorizationUrl")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let token_url = scheme
                    .pointer("/flows/authorizationCode/tokenUrl")
                    .or_else(|| scheme.pointer("/flows/clientCredentials/tokenUrl"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let scopes = scheme
                    .pointer("/flows/authorizationCode/scopes")
                    .and_then(Value::as_object)
                    .map(|scopes| scopes.keys().cloned().collect())
                    .unwrap_or_default();
                return AuthStrategy::OAuth2 {
                    provider: Some(name.clone()),
                    client_id_ref: format!("{name}_client_id"),
                    client_secret_ref: Some(format!("{name}_client_secret")),
                    auth_url,
                    token_url,
                    scopes,
                    redirect_port: 8421,
                };
            }
            _ => {}
        }
    }

    AuthStrategy::None
}

fn operation_resource_name(path: &str, operation: &Map<String, Value>) -> String {
    operation
        .get("tags")
        .and_then(Value::as_array)
        .and_then(|tags| tags.first())
        .and_then(Value::as_str)
        .map(sanitize_name)
        .unwrap_or_else(|| {
            path.trim_start_matches('/')
                .split('/')
                .find(|segment| !segment.starts_with('{') && !segment.is_empty())
                .map(sanitize_name)
                .unwrap_or_else(|| "resource".to_string())
        })
}

fn operation_name(
    path: &str,
    method_name: &str,
    operation: &Map<String, Value>,
    resource: &str,
) -> String {
    if let Some(id) = operation.get("operationId").and_then(Value::as_str) {
        return sanitize_name(id);
    }

    let has_id = path.contains('{');
    let prefix = match (method_name, has_id) {
        ("get", false) => "list",
        ("get", true) => "get",
        ("post", _) => "create",
        ("put", _) | ("patch", _) => "update",
        ("delete", _) => "delete",
        _ => "call",
    };

    format!("{prefix}_{}", sanitize_name(resource))
}

fn sanitize_name(input: &str) -> String {
    input
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_lowercase()
}

fn parse_method(name: &str) -> Option<HttpMethod> {
    Some(match name {
        "get" => HttpMethod::GET,
        "post" => HttpMethod::POST,
        "put" => HttpMethod::PUT,
        "patch" => HttpMethod::PATCH,
        "delete" => HttpMethod::DELETE,
        _ => return None,
    })
}

fn verb_from_method(method: &HttpMethod) -> Verb {
    match method {
        HttpMethod::GET => Verb::Get,
        HttpMethod::POST => Verb::Create,
        HttpMethod::PUT | HttpMethod::PATCH => Verb::Update,
        HttpMethod::DELETE => Verb::Delete,
    }
}

fn safety_from_method_name(method: &str) -> Safety {
    match method {
        "get" => Safety::ReadOnly,
        "delete" => Safety::Destructive,
        _ => Safety::Mutating,
    }
}

fn resolve_ref<'a>(document: &'a Value, value: &'a Value) -> &'a Value {
    let Some(reference) = value.get("$ref").and_then(Value::as_str) else {
        return value;
    };
    if !reference.starts_with("#/") {
        return value;
    }
    document.pointer(&reference[1..]).unwrap_or(value)
}

#[allow(dead_code)]
fn _example() -> Value {
    json!({})
}
