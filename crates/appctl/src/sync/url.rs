use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use reqwest::{Client, cookie::Jar};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::Map;

use crate::{
    config::ConfigPaths,
    schema::{
        Action, AuthStrategy, Field, FieldType, HttpMethod, ParameterLocation, Provenance,
        Resource, Safety, Schema, SyncSource, Transport, Verb,
    },
};

use super::{SyncPlugin, SyncRequest};

pub struct UrlSync {
    url: String,
    client: Client,
    jar: Arc<Jar>,
    session_path: PathBuf,
    login_url: Option<String>,
    login_user: Option<String>,
    login_password: Option<String>,
    login_form_selector: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SerializedJar {
    cookies: Vec<SerializedCookie>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SerializedCookie {
    url: String,
    header: String,
}

impl UrlSync {
    pub fn new(url: String, paths: &ConfigPaths, request: &SyncRequest) -> Result<Self> {
        let session_path = paths.root.join("session.json");
        let jar = Arc::new(Jar::default());
        load_jar(&jar, &session_path);
        let client = Client::builder()
            .cookie_provider(jar.clone())
            .user_agent("appctl/0.1 (+https://github.com/esubaalew/appctl)")
            .build()
            .context("failed to build cookie-enabled reqwest client")?;
        Ok(Self {
            url,
            client,
            jar,
            session_path,
            login_url: request.login_url.clone(),
            login_user: request.login_user.clone(),
            login_password: request.login_password.clone(),
            login_form_selector: request.login_form_selector.clone(),
        })
    }

    async fn perform_login(&self) -> Result<()> {
        let Some(login_url) = self.login_url.as_deref() else {
            return Ok(());
        };
        let Some(username) = self.login_user.as_deref() else {
            return Ok(());
        };
        let Some(password) = self.login_password.as_deref() else {
            return Ok(());
        };

        tracing::info!("logging in at {login_url} as {username}");

        let initial_html = self
            .client
            .get(login_url)
            .send()
            .await
            .with_context(|| format!("failed to GET {login_url}"))?
            .text()
            .await?;

        let (form_data, action) = {
            let document = Html::parse_document(&initial_html);
            let form_selector_str = self
                .login_form_selector
                .clone()
                .unwrap_or_else(|| "form".to_string());
            let form_selector = Selector::parse(&form_selector_str)
                .map_err(|err| anyhow::anyhow!("invalid form selector: {err}"))?;
            let form = document
                .select(&form_selector)
                .next()
                .context("no login form found on page")?;

            let input_selector =
                Selector::parse("input").map_err(|err| anyhow::anyhow!("{err}"))?;

            let mut form_data: HashMap<String, String> = HashMap::new();
            for input in form.select(&input_selector) {
                let name = match input.value().attr("name") {
                    Some(n) => n.to_string(),
                    None => continue,
                };
                let value = input.value().attr("value").unwrap_or_default().to_string();
                form_data.insert(name, value);
            }

            if let Some(csrf) = extract_csrf(&document) {
                form_data
                    .entry("csrf_token".to_string())
                    .or_insert_with(|| csrf.clone());
                form_data.entry("_token".to_string()).or_insert(csrf);
            }

            for (key, value) in form_data.iter_mut() {
                let lower = key.to_ascii_lowercase();
                if lower.contains("email") || lower.contains("user") || lower == "login" {
                    *value = username.to_string();
                }
                if lower.contains("pass") {
                    *value = password.to_string();
                }
            }

            let action = form
                .value()
                .attr("action")
                .filter(|a| !a.is_empty())
                .map(|a| a.to_string())
                .unwrap_or_else(|| login_url.to_string());
            (form_data, action)
        };
        let post_url = resolve_url(login_url, &action)?;

        let response = self
            .client
            .post(post_url.as_str())
            .form(&form_data)
            .send()
            .await
            .with_context(|| format!("login POST failed: {post_url}"))?;

        if !response.status().is_success() && !response.status().is_redirection() {
            anyhow::bail!(
                "login failed: {} {}",
                response.status().as_u16(),
                response.status().canonical_reason().unwrap_or_default()
            );
        }
        let _ = response.bytes().await;

        save_jar(&self.jar, &self.session_path)?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl SyncPlugin for UrlSync {
    async fn introspect(&self) -> Result<Schema> {
        self.perform_login().await?;

        let html = self
            .client
            .get(&self.url)
            .send()
            .await
            .with_context(|| format!("failed to fetch {}", self.url))?
            .text()
            .await?;
        save_jar(&self.jar, &self.session_path).ok();

        let document = Html::parse_document(&html);
        let form_selector = Selector::parse("form").expect("valid selector");
        let input_selector = Selector::parse("input,textarea,select").expect("valid selector");

        let mut resources = Vec::new();
        for (index, form) in document.select(&form_selector).enumerate() {
            let method = match form
                .value()
                .attr("method")
                .unwrap_or("post")
                .to_ascii_lowercase()
                .as_str()
            {
                "get" => HttpMethod::GET,
                "put" => HttpMethod::PUT,
                "patch" => HttpMethod::PATCH,
                "delete" => HttpMethod::DELETE,
                _ => HttpMethod::POST,
            };
            let action = form.value().attr("action").unwrap_or(&self.url).to_string();
            let fields = form
                .select(&input_selector)
                .filter_map(|input| input.value().attr("name"))
                .map(|name| Field {
                    name: name.to_string(),
                    description: None,
                    field_type: FieldType::String,
                    required: input_required(&document, name),
                    location: Some(ParameterLocation::Body),
                    default: None,
                    enum_values: Vec::new(),
                })
                .collect::<Vec<_>>();

            let resource_name = format!("form_{}", index + 1);
            resources.push(Resource {
                name: resource_name.clone(),
                description: Some(format!("HTML form discovered at {}", self.url)),
                fields: fields.clone(),
                actions: vec![Action {
                    name: format!("submit_{}", resource_name),
                    description: Some(format!("Submit form {} at {}", index + 1, self.url)),
                    verb: Verb::Custom,
                    transport: Transport::Form {
                        method: method.clone(),
                        action,
                    },
                    parameters: fields,
                    safety: if matches!(method, HttpMethod::GET) {
                        Safety::ReadOnly
                    } else {
                        Safety::Mutating
                    },
                    resource: Some(resource_name),
                    provenance: Provenance::Inferred,
                    metadata: Map::new(),
                }],
            });
        }

        let mut metadata = Map::new();
        metadata.insert(
            "session_file".to_string(),
            serde_json::Value::String(self.session_path.display().to_string()),
        );

        Ok(Schema {
            source: SyncSource::Url,
            base_url: Some(self.url.clone()),
            auth: AuthStrategy::Cookie {
                env_ref: None,
                session_file: Some(self.session_path.display().to_string()),
            },
            resources,
            metadata,
        })
    }
}

fn load_jar(_jar: &Arc<Jar>, path: &std::path::Path) {
    // We persist a flat list of (url, Set-Cookie header) pairs. On load we
    // replay them back into the jar via `add_cookie_str`.
    let Ok(raw) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(parsed): std::result::Result<SerializedJar, _> = serde_json::from_str(&raw) else {
        return;
    };
    for SerializedCookie { url, header } in parsed.cookies {
        let Ok(url) = url::Url::parse(&url) else {
            continue;
        };
        _jar.add_cookie_str(&header, &url);
    }
}

fn save_jar(_jar: &Arc<Jar>, path: &std::path::Path) -> Result<()> {
    // reqwest's Jar does not expose an enumerate API, so we persist only a
    // marker here; cookies remain in process memory for subsequent calls. In
    // future we can swap Jar for a custom provider that exposes iteration.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_string_pretty(&SerializedJar::default())?;
    std::fs::write(path, payload)?;
    Ok(())
}

fn extract_csrf(document: &Html) -> Option<String> {
    if let Ok(selector) = Selector::parse(r#"meta[name="csrf-token"]"#)
        && let Some(el) = document.select(&selector).next()
        && let Some(v) = el.value().attr("content")
    {
        return Some(v.to_string());
    }

    for name in &[
        "authenticity_token",
        "csrfmiddlewaretoken",
        "_token",
        "csrf_token",
    ] {
        if let Ok(selector) = Selector::parse(&format!(r#"input[name="{name}"]"#))
            && let Some(el) = document.select(&selector).next()
            && let Some(v) = el.value().attr("value")
        {
            return Some(v.to_string());
        }
    }

    None
}

fn resolve_url(base: &str, target: &str) -> Result<url::Url> {
    match url::Url::parse(target) {
        Ok(u) => Ok(u),
        Err(_) => {
            let base = url::Url::parse(base)?;
            Ok(base.join(target)?)
        }
    }
}

fn input_required(document: &Html, name: &str) -> bool {
    let selector = Selector::parse(&format!(r#"[name="{name}"]"#)).ok();
    selector
        .and_then(|selector| document.select(&selector).next())
        .is_some_and(|element| element.value().attr("required").is_some())
}
