use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow, bail};
use reqwest::header::HeaderName;
use serde_json::{Map, Value, json};
use url::Url;

use crate::schema::{
    Action, ApiKeyLocation, AuthStrategy, Field, FieldType, HttpMethod, ParameterLocation,
    Resource, Safety, Schema, SyncSource, Transport, Verb,
};

use super::SyncPlugin;

const OPENAPI_UA: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

/// Well-known paths tried after the first URL when the base URL is the site root and the
/// initial document GET returns 404.
const OPENAPI_PROBE_PATHS: &[&str] = &[
    "openapi.json",
    "v3/api-docs",
    "v2/api-docs",
    "api-docs",
    "swagger.json",
    "api/openapi.json",
];

pub struct OpenApiSync {
    source: String,
    /// `Header: value` for HTTP(S) fetches of the spec (e.g. `Authorization: Bearer env:STAGING_TOKEN`).
    auth_header: Option<String>,
}

impl OpenApiSync {
    pub fn new(source: String, auth_header: Option<String>) -> Self {
        Self {
            source,
            auth_header,
        }
    }
}

/// Fetches (or reads) the raw OpenAPI / Swagger text. `auth_header` is only used for `http`/`https` URLs
/// (same `Header: value` form as `appctl sync --auth-header`, including `env:NAME` in the value).
pub async fn load_openapi_source(source: &str, auth_header: Option<&str>) -> Result<String> {
    if source.starts_with("http://") || source.starts_with("https://") {
        fetch_openapi_url(source, auth_header)
            .await
            .with_context(|| format!("failed to load OpenAPI from {}", source))
    } else {
        tokio::fs::read_to_string(source)
            .await
            .with_context(|| format!("failed to read {}", source))
    }
}

fn build_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(OPENAPI_UA)
        .redirect(reqwest::redirect::Policy::default())
        .build()
        .map_err(Into::into)
}

fn expand_header_value_resolved(value: &str) -> Result<String> {
    let t = value.trim();
    if let Some(var) = t.strip_prefix("env:") {
        return std::env::var(var.trim()).with_context(|| {
            format!(
                "environment variable '{}' (from --auth-header value) is not set",
                var.trim()
            )
        });
    }
    if let Some(rest) = t.strip_prefix("Bearer ") {
        if let Some(var) = rest.trim().strip_prefix("env:") {
            let v = std::env::var(var.trim()).with_context(|| {
                format!(
                    "environment variable '{}' (from --auth-header Bearer) is not set",
                    var.trim()
                )
            })?;
            return Ok(format!("Bearer {v}"));
        }
    }
    Ok(t.to_string())
}

fn resolve_auth_line_resolved(line: &str) -> Result<(HeaderName, String)> {
    let (name, value) = line.split_once(':').ok_or_else(|| {
        anyhow!("auth header must look like 'Header-Name: value' (e.g. Authorization: Bearer …)")
    })?;
    let name = name.trim();
    if name.is_empty() {
        bail!("empty header name in --auth-header");
    }
    let hname = HeaderName::from_bytes(name.as_bytes())
        .map_err(|e| anyhow!("invalid header name {name}: {e}"))?;
    let value = expand_header_value_resolved(value.trim())?;
    Ok((hname, value))
}

fn is_root_path_url(u: &Url) -> bool {
    let p = u.path();
    p.is_empty() || p == "/"
}

/// Extra URLs to try (deduped) when the user gives a root base and the first response is 404.
fn open_api_probe_urls(initial: &Url) -> Vec<String> {
    if !is_root_path_url(initial) {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    for seg in OPENAPI_PROBE_PATHS {
        if let Ok(j) = initial.join(seg) {
            let s = j.to_string();
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}

async fn fetch_openapi_get(
    client: &reqwest::Client,
    url: &str,
    auth: Option<&str>,
) -> Result<reqwest::Response> {
    let mut req = client.get(url);
    if let Some(line) = auth.map(str::trim).filter(|s| !s.is_empty()) {
        let (k, v) = resolve_auth_line_resolved(line)?;
        req = req.header(k, v);
    }
    req = req.header(
        reqwest::header::ACCEPT,
        "application/json, application/yaml, text/yaml, */*;q=0.1",
    );
    let res = req.send().await.with_context(|| format!("GET {url}"))?;
    Ok(res)
}

async fn fetch_openapi_url(user_url: &str, auth_header: Option<&str>) -> Result<String> {
    let client = build_http_client()?;
    let primary =
        Url::parse(user_url).with_context(|| format!("invalid OpenAPI URL {user_url}"))?;

    let mut candidates: Vec<String> = vec![user_url.to_string()];
    for extra in open_api_probe_urls(&primary) {
        if !candidates.contains(&extra) {
            candidates.push(extra);
        }
    }

    let mut last_err: Option<anyhow::Error> = None;
    for u in &candidates {
        let res = match fetch_openapi_get(&client, u, auth_header).await {
            Ok(r) => r,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };
        let status = res.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            last_err = Some(anyhow!("{u} -> 404"));
            continue;
        }
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            bail!(
                "OpenAPI at {u} -> {status}. Pass --auth-header (e.g. 'Authorization: Bearer <token>' or 'Authorization: Bearer env:STAGING_API_TOKEN')."
            );
        }
        if !status.is_success() {
            last_err = Some(anyhow!("{u} -> {status}"));
            continue;
        }
        let text = res.text().await.unwrap_or_default();
        if text.trim().is_empty() {
            last_err = Some(anyhow!("{u} returned empty body"));
            continue;
        }
        return Ok(text);
    }

    if let Some(e) = last_err {
        if candidates.len() > 1 {
            bail!(
                "could not load OpenAPI from {user_url} (candidates: {}); last error: {e:#}",
                candidates.join(", ")
            );
        }
        return Err(e.context(format!("failed to fetch OpenAPI from {user_url}")));
    }
    Err(anyhow!(
        "no OpenAPI document found (candidates: {})",
        candidates.join(", ")
    ))
}

#[async_trait::async_trait]
impl SyncPlugin for OpenApiSync {
    async fn introspect(&self) -> Result<Schema> {
        let raw = load_openapi_source(&self.source, self.auth_header.as_deref()).await?;

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
                    metadata: Map::new(),
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

    for name in preferred_security_scheme_names(document, schemes) {
        let Some(scheme) = schemes.get(&name) else {
            continue;
        };
        if let Some(strategy) = auth_strategy_from_scheme(&name, scheme) {
            return strategy;
        }
    }

    AuthStrategy::None
}

fn preferred_security_scheme_names(document: &Value, schemes: &Map<String, Value>) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(security) = document.get("security").and_then(Value::as_array) {
        for requirement in security {
            let Some(requirement) = requirement.as_object() else {
                continue;
            };
            for name in requirement.keys() {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
    }
    for name in schemes.keys() {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    names
}

fn auth_strategy_from_scheme(name: &str, scheme: &Value) -> Option<AuthStrategy> {
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
            let location = match scheme.get("in").and_then(Value::as_str) {
                Some("query") => ApiKeyLocation::Query,
                Some("cookie") => ApiKeyLocation::Cookie,
                _ => ApiKeyLocation::Header,
            };
            Some(AuthStrategy::ApiKey {
                header,
                env_ref: name.to_string(),
                location,
            })
        }
        "http" if scheme.get("scheme").and_then(Value::as_str) == Some("bearer") => {
            Some(AuthStrategy::Bearer {
                env_ref: name.to_string(),
            })
        }
        "http" if scheme.get("scheme").and_then(Value::as_str) == Some("basic") => {
            Some(AuthStrategy::Basic {
                username_ref: format!("{name}_username"),
                password_ref: format!("{name}_password"),
            })
        }
        "basic" => Some(AuthStrategy::Basic {
            username_ref: format!("{name}_username"),
            password_ref: format!("{name}_password"),
        }),
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
            Some(AuthStrategy::OAuth2 {
                provider: Some(name.to_string()),
                client_id_ref: format!("{name}_client_id"),
                client_secret_ref: Some(format!("{name}_client_secret")),
                auth_url,
                token_url,
                scopes,
                redirect_port: 8421,
            })
        }
        _ => None,
    }
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

#[cfg(test)]
mod openapi_fetch_tests {
    use super::*;

    #[test]
    fn probe_urls_include_openapi_json_for_root_base() {
        let u = Url::parse("https://a.example:8443/").unwrap();
        let v = open_api_probe_urls(&u);
        assert!(v.iter().any(|s| s.ends_with("/openapi.json")));
        assert!(v.iter().any(|s| s.contains("v3/api-docs")));
    }

    #[test]
    fn probe_urls_empty_when_path_is_not_root() {
        let u = Url::parse("https://a.example/foo/bar.json").unwrap();
        assert!(open_api_probe_urls(&u).is_empty());
    }

    #[test]
    fn detect_auth_prefers_declared_security_and_query_api_keys() {
        let document = json!({
            "openapi": "3.0.0",
            "security": [{ "queryKey": [] }],
            "components": {
                "securitySchemes": {
                    "unusedBearer": { "type": "http", "scheme": "bearer" },
                    "queryKey": { "type": "apiKey", "in": "query", "name": "api_key" }
                }
            }
        });

        let AuthStrategy::ApiKey {
            header, location, ..
        } = detect_auth(&document)
        else {
            panic!("expected query api key");
        };
        assert_eq!(header, "api_key");
        assert_eq!(location, ApiKeyLocation::Query);
    }

    #[test]
    fn detect_auth_supports_openapi3_http_basic() {
        let document = json!({
            "openapi": "3.0.0",
            "components": {
                "securitySchemes": {
                    "basicAuth": { "type": "http", "scheme": "basic" }
                }
            }
        });

        assert!(matches!(detect_auth(&document), AuthStrategy::Basic { .. }));
    }
}
