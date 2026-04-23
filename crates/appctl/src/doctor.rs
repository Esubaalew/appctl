//! HTTP route probes for synced schemas (`appctl doctor`).

use std::time::Duration;

use crate::{
    config::{AppConfig, ConfigPaths, ProviderKind, ResolvedProvider, write_json},
    executor::build_headers,
    schema::{HttpMethod, Provenance, Transport},
    sync::load_schema,
    term::{
        format_api_error_summary, print_flow_header, print_framed_list, print_kv_block,
        print_path_row, print_section_title, print_status_success, print_status_warn,
        print_subsection, print_tip,
    },
    tools::schema_to_tools,
};
use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use reqwest::Method;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct DoctorRunArgs {
    pub write: bool,
    pub timeout_secs: u64,
}

pub async fn run_doctor(paths: &ConfigPaths, args: DoctorRunArgs) -> Result<()> {
    let mut schema = load_schema(paths)?;
    let config = AppConfig::load_or_init(paths)?;
    let client = reqwest::Client::new();
    let timeout = Duration::from_secs(args.timeout_secs.max(1));

    let base = schema
        .base_url
        .clone()
        .or_else(|| {
            config
                .target
                .base_url_env
                .as_deref()
                .and_then(|name| std::env::var(name).ok())
                .filter(|value| !value.trim().is_empty())
                .or_else(|| std::env::var("APPCTL_BASE_URL").ok())
                .filter(|value| !value.trim().is_empty())
                .or_else(|| config.target.base_url.clone())
        })
        .context("schema has no base_url; pass --base-url on sync or set target.base_url")?;

    let headers = build_headers(&schema.auth, &config, None)?;

    print_flow_header(
        "doctor",
        Some("Safe HTTP probes for each tool in the synced schema (HEAD/OPTIONS/GET)"),
    );
    print_path_row("app directory", &paths.root);
    print_kv_block("Target", &[("base URL", base.as_str())]);
    print_section_title("Route probes");
    println!(
        "  {:<32} {:<6} {:<48} {:>5}  {}",
        "tool".dimmed(),
        "method".dimmed(),
        "path".dimmed(),
        "HTTP".dimmed(),
        "verdict".dimmed()
    );
    println!("  {}", "─".repeat(88).dimmed());

    let mut any_http = false;
    let mut updates: Vec<(String, u16, bool)> = Vec::new();

    for resource in &schema.resources {
        for action in &resource.actions {
            let Transport::Http {
                method: ref hm,
                ref path,
                ..
            } = action.transport
            else {
                continue;
            };
            any_http = true;
            let path_resolved = resolve_path_placeholders(path);
            let url = format!(
                "{}/{}",
                base.trim_end_matches('/'),
                path_resolved.trim_start_matches('/')
            );

            let (status, verdict) =
                match probe_http_tool(&client, hm, &url, headers.clone(), timeout).await {
                    Ok(code) => {
                        let ok = verifies_route(code);
                        let v = if ok {
                            "reachable"
                        } else if code == 404 {
                            "missing (404)"
                        } else {
                            "check"
                        };
                        (code, v.to_string())
                    }
                    Err(e) => (0, format!("error: {e:#}")),
                };

            let verified = status != 0 && status != 404;
            updates.push((action.name.clone(), status, verified));

            println!(
                "  {:<32} {:<6} {:<48} {:>5}  {}",
                action.name,
                http_method_label(hm),
                truncate(&path_resolved, 48),
                if status == 0 {
                    "-".to_string()
                } else {
                    status.to_string()
                },
                verdict
            );
        }
    }

    if !any_http {
        print_subsection("Result");
        println!(
            "  {}",
            "(no HTTP tools in this schema — nothing to probe)".dimmed()
        );
        return Ok(());
    }

    if args.write {
        let mut changed = 0;
        for resource in &mut schema.resources {
            for action in &mut resource.actions {
                if let Some((_, status, verified)) =
                    updates.iter().find(|(n, _, _)| n == &action.name)
                {
                    if *verified && *status != 404 && action.provenance != Provenance::Verified {
                        action.provenance = Provenance::Verified;
                        changed += 1;
                    }
                }
            }
        }
        let tools = schema_to_tools(&schema);
        write_json(&paths.schema, &schema)?;
        write_json(&paths.tools, &tools)?;
        print_section_title("Write-back");
        print_status_success(&format!(
            "wrote {changed} provenance update(s) to {} (use --write only after reviewing probes)",
            paths.schema.display()
        ));
    } else {
        print_tip(
            "Pass --write to mark reachable (non-404) routes as provenance=verified in the schema.",
        );
    }

    if let Some(w) = schema.metadata.get("warnings") {
        print_subsection("Sync warnings");
        println!("  {w}");
    }

    Ok(())
}

pub async fn run_doctor_models(
    paths: &ConfigPaths,
    config: &AppConfig,
    provider_name: Option<&str>,
) -> Result<()> {
    let provider = config.resolve_provider_with_paths(Some(paths), provider_name, None)?;
    print_flow_header(
        "doctor models",
        Some("Model ids you can set in config or `appctl chat --model`"),
    );
    print_path_row("app directory", &paths.root);
    print_kv_block(
        "Provider",
        &[
            ("name", provider.name.as_str()),
            ("kind", &format!("{:?}", provider.kind)),
            ("base URL", provider.base_url.as_str()),
            ("current model", provider.model.as_str()),
        ],
    );
    println!();

    let models = list_models_for_provider(&provider).await?;
    if models.is_empty() {
        print_subsection("Models");
        println!(
            "  {}",
            "No models were returned (or listing is not supported for this provider).".dimmed()
        );
    } else {
        print_framed_list("Available models", &models, None);
    }
    Ok(())
}

fn verifies_route(status: u16) -> bool {
    status != 404 && status != 0
}

fn http_method_label(m: &HttpMethod) -> &'static str {
    match m {
        HttpMethod::GET => "GET",
        HttpMethod::POST => "POST",
        HttpMethod::PUT => "PUT",
        HttpMethod::PATCH => "PATCH",
        HttpMethod::DELETE => "DELETE",
    }
}

fn resolve_path_placeholders(path: &str) -> String {
    path.replace("{id}", "1")
        .replace("{Id}", "1")
        .replace("{uuid}", "00000000-0000-0000-0000-000000000001")
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

/// Probe without side effects: HEAD/OPTIONS first, then light GET for read routes.
async fn probe_http_tool(
    client: &reqwest::Client,
    tool_method: &HttpMethod,
    url: &str,
    headers: reqwest::header::HeaderMap,
    timeout: Duration,
) -> Result<u16> {
    match tool_method {
        HttpMethod::GET => {
            if let Ok(resp) = client
                .request(Method::HEAD, url)
                .headers(headers.clone())
                .timeout(timeout)
                .send()
                .await
            {
                let c = resp.status().as_u16();
                if c != 405 && c != 404 {
                    return Ok(c);
                }
            }
            let resp = client
                .get(url)
                .headers(headers)
                .timeout(timeout)
                .send()
                .await?;
            Ok(resp.status().as_u16())
        }
        HttpMethod::DELETE => {
            if let Ok(resp) = client
                .request(Method::HEAD, url)
                .headers(headers.clone())
                .timeout(timeout)
                .send()
                .await
            {
                return Ok(resp.status().as_u16());
            }
            let resp = client
                .request(Method::OPTIONS, url)
                .headers(headers)
                .timeout(timeout)
                .send()
                .await?;
            Ok(resp.status().as_u16())
        }
        HttpMethod::POST | HttpMethod::PUT | HttpMethod::PATCH => {
            if let Ok(resp) = client
                .request(Method::OPTIONS, url)
                .headers(headers.clone())
                .timeout(timeout)
                .send()
                .await
            {
                let c = resp.status().as_u16();
                if c != 404 {
                    return Ok(c);
                }
            }
            let resp = client
                .request(Method::HEAD, url)
                .headers(headers)
                .timeout(timeout)
                .send()
                .await?;
            Ok(resp.status().as_u16())
        }
    }
}

async fn list_models_for_provider(provider: &ResolvedProvider) -> Result<Vec<String>> {
    match provider.kind {
        ProviderKind::OpenAiCompatible => list_models_openai_compatible(provider).await,
        ProviderKind::GoogleGenai => list_models_google_genai(provider).await,
        ProviderKind::Anthropic => list_models_anthropic(provider).await,
        ProviderKind::Vertex => list_models_vertex(provider).await,
        ProviderKind::AzureOpenAi => {
            print_status_warn(
                "Model listing is not available for Azure OpenAI here — the `model` field is your deployment name.",
            );
            Ok(Vec::new())
        }
    }
}

async fn list_models_openai_compatible(provider: &ResolvedProvider) -> Result<Vec<String>> {
    let client = reqwest::Client::new();
    let mut request = client.get(format!(
        "{}/models",
        provider.base_url.trim_end_matches('/')
    ));
    if let Some(api_key) = provider.auth.api_key() {
        request = request.bearer_auth(api_key);
    }
    if let Some(token) = provider.auth.bearer_token() {
        request = request.bearer_auth(token);
    }
    for (name, value) in &provider.extra_headers {
        request = request.header(name, value);
    }
    let response = request
        .send()
        .await
        .context("failed to call OpenAI-compatible /models endpoint")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read OpenAI-compatible /models response")?;
    if !status.is_success() {
        anyhow::bail!(
            "OpenAI-compatible /models returned {}: {}",
            status,
            format_api_error_summary(&body)
        );
    }
    let value: Value = serde_json::from_str(&body).context("failed to parse /models JSON")?;
    let mut ids = Vec::new();
    if let Some(data) = value.get("data").and_then(Value::as_array) {
        for item in data {
            if let Some(id) = item.get("id").and_then(Value::as_str) {
                ids.push(id.to_string());
            }
        }
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

async fn list_models_google_genai(provider: &ResolvedProvider) -> Result<Vec<String>> {
    let api_key = provider
        .auth
        .api_key()
        .context("Google GenAI model listing requires an API key")?;
    let client = reqwest::Client::new();
    let response = client
        .get(format!(
            "{}/v1beta/models?key={}",
            provider.base_url.trim_end_matches('/'),
            api_key
        ))
        .send()
        .await
        .context("failed to call Google GenAI models endpoint")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read Google GenAI models response")?;
    if !status.is_success() {
        anyhow::bail!(
            "Google GenAI models endpoint returned {}: {}",
            status,
            format_api_error_summary(&body)
        );
    }
    let value: Value = serde_json::from_str(&body).context("failed to parse Google GenAI JSON")?;
    let mut ids = Vec::new();
    if let Some(models) = value.get("models").and_then(Value::as_array) {
        for model in models {
            let supported = model
                .get("supportedGenerationMethods")
                .and_then(Value::as_array)
                .map(|methods| {
                    methods
                        .iter()
                        .any(|m| m.as_str() == Some("generateContent"))
                })
                .unwrap_or(false);
            if supported && let Some(name) = model.get("name").and_then(Value::as_str) {
                ids.push(name.trim_start_matches("models/").to_string());
            }
        }
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

async fn list_models_anthropic(provider: &ResolvedProvider) -> Result<Vec<String>> {
    let api_key = provider
        .auth
        .api_key()
        .context("Anthropic model listing requires an API key")?;
    let client = reqwest::Client::new();
    let response = client
        .get(format!(
            "{}/v1/models",
            provider.base_url.trim_end_matches('/')
        ))
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .context("failed to call Anthropic models endpoint")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read Anthropic models response")?;
    if !status.is_success() {
        anyhow::bail!(
            "Anthropic models endpoint returned {}: {}",
            status,
            format_api_error_summary(&body)
        );
    }
    let value: Value = serde_json::from_str(&body).context("failed to parse Anthropic JSON")?;
    let mut ids = Vec::new();
    if let Some(data) = value.get("data").and_then(Value::as_array) {
        for item in data {
            if let Some(id) = item.get("id").and_then(Value::as_str) {
                ids.push(id.to_string());
            }
        }
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

async fn list_models_vertex(provider: &ResolvedProvider) -> Result<Vec<String>> {
    let token = provider
        .auth
        .bearer_token()
        .context("Vertex model listing requires a bearer token")?;
    let project_id = provider
        .auth_status
        .project_id
        .as_deref()
        .context("Vertex model listing requires a Google Cloud project")?;
    let region = provider
        .extra_headers
        .get("x-appctl-vertex-region")
        .cloned()
        .or_else(|| {
            url::Url::parse(&provider.base_url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_string))
                .and_then(|host| host.split('.').next().map(str::to_string))
        })
        .unwrap_or_else(|| "us-central1".to_string());
    let client = reqwest::Client::new();
    let response = client
        .get(format!(
            "{}/v1/projects/{}/locations/{}/publishers/google/models",
            provider.base_url.trim_end_matches('/'),
            project_id,
            region
        ))
        .bearer_auth(token)
        .send()
        .await
        .context("failed to call Vertex models endpoint")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read Vertex models response")?;
    if !status.is_success() {
        anyhow::bail!(
            "Vertex models endpoint returned {}: {}",
            status,
            format_api_error_summary(&body)
        );
    }
    let value: Value = serde_json::from_str(&body).context("failed to parse Vertex JSON")?;
    let mut ids = Vec::new();
    if let Some(models) = value.get("models").and_then(Value::as_array) {
        for model in models {
            if let Some(name) = model.get("name").and_then(Value::as_str) {
                ids.push(name.rsplit('/').next().unwrap_or(name).to_string());
            }
        }
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}
