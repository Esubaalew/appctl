use anyhow::{Context, Result, bail};
use reqwest::StatusCode;
use serde_json::{Value, json};
use url::Url;

use crate::config::{ProviderKind, ResolvedProvider};

pub async fn verify_provider(provider: &ResolvedProvider) -> Result<()> {
    let client = reqwest::Client::new();
    match provider.kind {
        ProviderKind::Anthropic => verify_anthropic(&client, provider).await,
        ProviderKind::OpenAiCompatible => verify_openai_compatible(&client, provider).await,
        ProviderKind::GoogleGenai => verify_google_genai(&client, provider).await,
        ProviderKind::Vertex => verify_vertex(&client, provider).await,
        ProviderKind::AzureOpenAi => verify_azure_openai(&client, provider).await,
    }
}

async fn verify_anthropic(client: &reqwest::Client, provider: &ResolvedProvider) -> Result<()> {
    let mut request = client
        .post(format!(
            "{}/v1/messages",
            provider.base_url.trim_end_matches('/')
        ))
        .header("anthropic-version", "2023-06-01");
    if let Some(api_key) = provider.auth.api_key() {
        request = request.header("x-api-key", api_key);
    }
    if let Some(token) = provider.auth.bearer_token() {
        request = request.bearer_auth(token);
    }
    for (name, value) in &provider.extra_headers {
        request = request.header(name, value);
    }
    ensure_success(
        request.json(&json!({
            "model": provider.model,
            "max_tokens": 16,
            "messages": [{"role": "user", "content": "Reply with ok."}]
        })),
        "Anthropic verify call",
        provider.kind,
    )
    .await
}

async fn verify_openai_compatible(
    client: &reqwest::Client,
    provider: &ResolvedProvider,
) -> Result<()> {
    let mut request = client.post(format!(
        "{}/chat/completions",
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
    ensure_success(
        request.json(&json!({
            "model": provider.model,
            "messages": [{"role": "user", "content": "Reply with ok."}]
        })),
        "OpenAI-compatible verify call",
        provider.kind,
    )
    .await
}

async fn verify_google_genai(client: &reqwest::Client, provider: &ResolvedProvider) -> Result<()> {
    let mut request = client.post(format!(
        "{}/v1beta/models/{}:generateContent",
        provider.base_url.trim_end_matches('/'),
        provider.model
    ));
    if let Some(api_key) = provider.auth.api_key() {
        request = request.header("x-goog-api-key", api_key);
    }
    if let Some(token) = provider.auth.bearer_token() {
        request = request.bearer_auth(token);
    }
    for (name, value) in &provider.extra_headers {
        request = request.header(name, value);
    }
    ensure_success(
        request.json(&json!({
            "contents": [{
                "role": "user",
                "parts": [{"text": "Reply with ok."}]
            }]
        })),
        "Google GenAI verify call",
        provider.kind,
    )
    .await
}

async fn verify_vertex(client: &reqwest::Client, provider: &ResolvedProvider) -> Result<()> {
    let access_token = provider
        .auth
        .bearer_token()
        .context("Vertex verify requires a bearer token")?;
    let project_id = provider
        .auth_status
        .project_id
        .clone()
        .context("Vertex verify requires a Google Cloud project")?;
    let region = provider
        .extra_headers
        .get("x-appctl-vertex-region")
        .cloned()
        .or_else(|| {
            Url::parse(&provider.base_url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_string))
                .and_then(|host| host.split('.').next().map(str::to_string))
        })
        .unwrap_or_else(|| "us-central1".to_string());

    let mut request = client
        .post(format!(
            "{}/v1/projects/{}/locations/{}/publishers/google/models/{}:generateContent",
            provider.base_url.trim_end_matches('/'),
            project_id,
            region,
            provider.model
        ))
        .bearer_auth(access_token);
    for (name, value) in &provider.extra_headers {
        if name != "x-appctl-vertex-region" {
            request = request.header(name, value);
        }
    }
    ensure_success(
        request.json(&json!({
            "contents": [{
                "role": "user",
                "parts": [{"text": "Reply with ok."}]
            }]
        })),
        "Vertex verify call",
        provider.kind,
    )
    .await
}

async fn verify_azure_openai(client: &reqwest::Client, provider: &ResolvedProvider) -> Result<()> {
    let mut request = client.post(format!(
        "{}/openai/deployments/{}/chat/completions?api-version=2024-10-21",
        provider.base_url.trim_end_matches('/'),
        provider.model
    ));
    if let Some(api_key) = provider.auth.api_key() {
        request = request.header("api-key", api_key);
    }
    if let Some(token) = provider.auth.bearer_token() {
        request = request.bearer_auth(token);
    }
    for (name, value) in &provider.extra_headers {
        request = request.header(name, value);
    }
    ensure_success(
        request.json(&json!({
            "messages": [{"role": "user", "content": "Reply with ok."}]
        })),
        "Azure OpenAI verify call",
        provider.kind,
    )
    .await
}

async fn ensure_success(
    builder: reqwest::RequestBuilder,
    label: &str,
    kind: ProviderKind,
) -> Result<()> {
    let response = builder
        .send()
        .await
        .with_context(|| format!("could not send {label} (network)"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .with_context(|| format!("could not read the response for {label}"))?;
    if status.is_success() {
        return Ok(());
    }
    bail!("{}", format_verify_failure(status, &body, label, kind))
}

/// Single multiline string; no raw JSON spew, concrete next steps.
fn format_verify_failure(
    status: StatusCode,
    body: &str,
    label: &str,
    kind: ProviderKind,
) -> String {
    let api_line = openai_style_error_line(body);
    let mut out = String::new();
    out.push_str("Connection check did not succeed.\n\n");
    out.push_str(&format!("  • Step:  {}\n  • HTTP:  {}\n", label, status));
    if let Some(line) = &api_line {
        out.push_str(&format!("  • API:   {}\n", line));
    }
    out.push('\n');
    out.push_str(&verify_failure_hints(status, &api_line, kind));
    out
}

fn openai_style_error_line(body: &str) -> Option<String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        if let Some(s) = v.pointer("/error/message").and_then(|x| x.as_str()) {
            return Some(compact_line(s, 220));
        }
        if let Some(s) = v.get("message").and_then(|x| x.as_str()) {
            return Some(compact_line(s, 220));
        }
        if let Some(e) = v.get("error") {
            if let Some(s) = e.as_str() {
                return Some(compact_line(s, 220));
            }
            if let Some(s) = e
                .as_object()
                .and_then(|o| o.get("message"))
                .and_then(|x| x.as_str())
            {
                return Some(compact_line(s, 220));
            }
        }
    }
    let c = compact_line(trimmed, 220);
    if c.is_empty() {
        return None;
    }
    Some(c)
}

fn compact_line(input: &str, max_chars: usize) -> String {
    let single = input.split_whitespace().collect::<Vec<_>>().join(" ");
    let c: String = single.chars().take(max_chars).collect();
    if c.len() < single.len() {
        format!("{c}…")
    } else {
        c
    }
}

fn verify_failure_hints(
    status: StatusCode,
    api_line: &Option<String>,
    kind: ProviderKind,
) -> String {
    let text = api_line.as_deref().unwrap_or("").to_ascii_lowercase();
    let is_model_missing = (status == StatusCode::NOT_FOUND || status == StatusCode::BAD_REQUEST)
        && (text.contains("model")
            && (text.contains("not found") || text.contains("unknown model")));
    match kind {
        ProviderKind::OpenAiCompatible if is_model_missing => {
            "What usually fixes this:\n  • The model name must match what the server actually serves.\n  • Ollama: run `ollama list` and use that exact name (e.g. `llama3.2:latest`), or `ollama pull <name>` first.\n  • Same base URL: `GET {your-base}/models` should list model ids; use one of them as `model` in config.\n  • Rerun: `appctl --app-dir <.appctl> init` and pick a listed model, or set `appctl run --model …` to override.\n".to_string()
        }
        ProviderKind::OpenAiCompatible if status == StatusCode::UNAUTHORIZED => {
            "What usually fixes this:\n  • This endpoint expects an API key. Add `Authorization: Bearer …` via config or a provider that stores a key, then verify again.\n".to_string()
        }
        ProviderKind::OpenAiCompatible if !status.is_success() => {
            "What usually fixes this:\n  • Confirm the local server is running and the base URL is the OpenAI-compatible root ending in `/v1`.\n  • Open `GET {base}/models` in a browser or with curl; if that fails, fix the URL or the server first.\n".to_string()
        }
        ProviderKind::Vertex if !status.is_success() => {
            "What usually fixes this:\n  • Check the model id and region in Vertex AI, and that billing/APIs are enabled for the project.\n  • In config, `x-appctl-vertex-region` must match the host region you use in the base URL.\n".to_string()
        }
        ProviderKind::GoogleGenai if status == StatusCode::TOO_MANY_REQUESTS && text.contains("quota") => {
            "What usually fixes this:\n  • Google quota is exhausted for this model/key (not an invalid key).\n  • `gemini-2.5-pro` may require paid billing for your account.\n  • Try a lower-tier model like `gemini-1.5-flash`, or upgrade quotas:\n    https://ai.google.dev/gemini-api/docs/rate-limits\n".to_string()
        }
        ProviderKind::GoogleGenai if status == StatusCode::TOO_MANY_REQUESTS => {
            "What usually fixes this:\n  • You are being rate-limited. Wait a short time, then retry.\n  • If this repeats, reduce request volume or use a lower-cost model.\n".to_string()
        }
        ProviderKind::GoogleGenai if status == StatusCode::UNAUTHORIZED => {
            "What usually fixes this:\n  • The API key is invalid or disabled. Regenerate/check it in Google AI Studio and store it again.\n".to_string()
        }
        ProviderKind::GoogleGenai if is_model_missing => {
            "What usually fixes this:\n  • The model id is not available for this key/account. Pick a listed model from `appctl init` or Google AI Studio.\n".to_string()
        }
        ProviderKind::GoogleGenai if !status.is_success() => {
            "What usually fixes this:\n  • Confirm key, model access, and project billing in Google AI Studio.\n".to_string()
        }
        ProviderKind::AzureOpenAi if !status.is_success() => {
            "What usually fixes this:\n  • The `model` field must be your deployment name, and the key must be for that resource.\n".to_string()
        }
        ProviderKind::Anthropic if !status.is_success() => {
            "What usually fixes this:\n  • Check the API key, model id, and that the Anthropic account has access to that model.\n".to_string()
        }
        _ if !status.is_success() => {
            "Check the API message above, fix credentials, model id, or base URL, then run `appctl init` or `appctl auth provider login …` again.\n"
                .to_string()
        }
        _ => String::new(),
    }
}
