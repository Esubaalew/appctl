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

pub struct FlaskSync {
    root: PathBuf,
    base_url: Option<String>,
}

impl FlaskSync {
    pub fn new(root: PathBuf, base_url: Option<String>) -> Self {
        Self { root, base_url }
    }
}

#[async_trait::async_trait]
impl SyncPlugin for FlaskSync {
    async fn introspect(&self) -> Result<Schema> {
        let resources = parse_models(&self.root)?;
        let routes = parse_routes(&self.root)?;
        let mut warnings = Vec::new();

        let resources = resources
            .into_iter()
            .map(|mut resource| {
                if let Some((path, methods)) = best_route_for_resource(&resource.name, &routes) {
                    resource.actions =
                        standard_flask_actions(&resource.name, path, methods, &resource.fields);
                }
                resource
            })
            .collect::<Vec<_>>();

        if routes.is_empty() {
            warnings.push("No Flask route decorators were detected. CRUD tools were generated only for discovered models.".to_string());
        }

        let mut metadata = Map::new();
        if !warnings.is_empty() {
            metadata.insert("warnings".to_string(), json!(warnings));
        }

        Ok(Schema {
            source: SyncSource::Flask,
            base_url: self.base_url.clone(),
            auth: AuthStrategy::Cookie {
                env_ref: None,
                session_file: None,
            },
            resources,
            metadata,
        })
    }
}

fn parse_models(root: &PathBuf) -> Result<Vec<Resource>> {
    let class_re = Regex::new(r"class\s+([A-Za-z0-9_]+)\s*\(([^)]*)\)\s*:")?;
    let field_re = Regex::new(
        r"^\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*=\s*(?:db\.)?(?:Column|mapped_column)\((.*)\)",
    )?;

    let mut resources = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("py"))
    {
        let path = entry.path();
        if path.to_string_lossy().contains("__pycache__") {
            continue;
        }
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;

        let mut current: Option<Resource> = None;
        for line in content.lines() {
            if let Some(caps) = class_re.captures(line) {
                if let Some(resource) = current.take() {
                    resources.push(resource);
                }
                let parents = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
                if parents.contains("db.Model") || parents.contains("Model") {
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
                let args = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
                resource.fields.push(Field {
                    name: field_name.to_string(),
                    description: None,
                    field_type: flask_field_type(args),
                    required: !args.contains("nullable=True"),
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

    resources.sort_by(|a, b| a.name.cmp(&b.name));
    resources.dedup_by(|a, b| a.name == b.name);
    Ok(resources)
}

fn parse_routes(root: &PathBuf) -> Result<BTreeMap<String, Vec<HttpMethod>>> {
    let route_re = Regex::new(
        r#"@(?:[A-Za-z_][A-Za-z0-9_]*\.)?route\(\s*["']([^"']+)["'](?:,\s*methods\s*=\s*\[([^\]]+)\])?"#,
    )?;
    let method_re = Regex::new(r#"["']([A-Z]+)["']"#)?;
    let mut routes = BTreeMap::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("py"))
    {
        let path = entry.path();
        if path.to_string_lossy().contains("__pycache__") {
            continue;
        }
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        for caps in route_re.captures_iter(&content) {
            let route = caps.get(1).unwrap().as_str().to_string();
            let mut methods = Vec::new();
            if let Some(raw_methods) = caps.get(2) {
                for method in method_re.captures_iter(raw_methods.as_str()) {
                    if let Some(parsed) = parse_method(method.get(1).unwrap().as_str()) {
                        push_method(&mut methods, parsed);
                    }
                }
            }
            if methods.is_empty() {
                methods.push(HttpMethod::GET);
            }
            routes
                .entry(route)
                .and_modify(|existing| {
                    for method in &methods {
                        push_method(existing, method.clone());
                    }
                })
                .or_insert(methods);
        }
    }

    Ok(routes)
}

fn best_route_for_resource<'a>(
    resource_name: &str,
    routes: &'a BTreeMap<String, Vec<HttpMethod>>,
) -> Option<(&'a str, &'a [HttpMethod])> {
    routes.iter().find_map(|(path, methods)| {
        let normalized = path.trim_matches('/').replace('-', "_");
        if normalized.ends_with(resource_name) || normalized.contains(&format!("/{resource_name}"))
        {
            Some((path.as_str(), methods.as_slice()))
        } else {
            None
        }
    })
}

fn standard_flask_actions(
    resource_name: &str,
    route: &str,
    methods: &[HttpMethod],
    fields: &[Field],
) -> Vec<Action> {
    let id_path = if route.contains('<') {
        route.to_string()
    } else {
        format!("{}/<id>", route.trim_end_matches('/'))
    };
    let id_field = Field {
        name: "id".to_string(),
        description: None,
        field_type: FieldType::String,
        required: true,
        location: Some(ParameterLocation::Path),
        default: None,
        enum_values: Vec::new(),
    };

    let mut actions = Vec::new();
    if has_method(methods, &HttpMethod::GET) {
        actions.push(http_action(
            format!("list_{resource_name}"),
            route.to_string(),
            HttpMethod::GET,
            Verb::List,
            Safety::ReadOnly,
            Vec::new(),
            resource_name,
        ));
        actions.push(http_action(
            format!("get_{resource_name}"),
            id_path.clone(),
            HttpMethod::GET,
            Verb::Get,
            Safety::ReadOnly,
            vec![id_field.clone()],
            resource_name,
        ));
    }
    if has_method(methods, &HttpMethod::POST) {
        actions.push(http_action(
            format!("create_{resource_name}"),
            route.to_string(),
            HttpMethod::POST,
            Verb::Create,
            Safety::Mutating,
            fields.to_vec(),
            resource_name,
        ));
    }
    if has_method(methods, &HttpMethod::PUT) || has_method(methods, &HttpMethod::PATCH) {
        let method = if has_method(methods, &HttpMethod::PATCH) {
            HttpMethod::PATCH
        } else {
            HttpMethod::PUT
        };
        let mut params = vec![id_field.clone()];
        params.extend(fields.iter().cloned());
        actions.push(http_action(
            format!("update_{resource_name}"),
            id_path.clone(),
            method,
            Verb::Update,
            Safety::Mutating,
            params,
            resource_name,
        ));
    }
    if has_method(methods, &HttpMethod::DELETE) {
        actions.push(http_action(
            format!("delete_{resource_name}"),
            id_path,
            HttpMethod::DELETE,
            Verb::Delete,
            Safety::Destructive,
            vec![id_field],
            resource_name,
        ));
    }
    actions
}

fn has_method(methods: &[HttpMethod], needle: &HttpMethod) -> bool {
    methods.iter().any(|method| method == needle)
}

fn push_method(methods: &mut Vec<HttpMethod>, method: HttpMethod) {
    if !has_method(methods, &method) {
        methods.push(method);
    }
}

fn http_action(
    name: String,
    path: String,
    method: HttpMethod,
    verb: Verb,
    safety: Safety,
    parameters: Vec<Field>,
    resource: &str,
) -> Action {
    Action {
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
        resource: Some(resource.to_string()),
        provenance: Provenance::Inferred,
        metadata: Map::new(),
    }
}

fn parse_method(value: &str) -> Option<HttpMethod> {
    match value {
        "GET" => Some(HttpMethod::GET),
        "POST" => Some(HttpMethod::POST),
        "PUT" => Some(HttpMethod::PUT),
        "PATCH" => Some(HttpMethod::PATCH),
        "DELETE" => Some(HttpMethod::DELETE),
        _ => None,
    }
}

fn flask_field_type(args: &str) -> FieldType {
    let value = args.to_ascii_lowercase();
    if value.contains("integer") || value.contains("smallinteger") || value.contains("biginteger") {
        FieldType::Integer
    } else if value.contains("float") || value.contains("numeric") || value.contains("decimal") {
        FieldType::Number
    } else if value.contains("boolean") {
        FieldType::Boolean
    } else if value.contains("datetime") {
        FieldType::DateTime
    } else if value.contains("date") {
        FieldType::Date
    } else if value.contains("json") {
        FieldType::Json
    } else {
        FieldType::String
    }
}

fn to_resource_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_uppercase() && !out.is_empty() {
            out.push('_');
        }
        out.extend(ch.to_lowercase());
    }
    if out.ends_with('s') {
        out
    } else {
        format!("{out}s")
    }
}
