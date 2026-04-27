use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use axum::{
    Json, Router,
    body::Body,
    extract::{Query, State, WebSocketUpgrade},
    http::{HeaderMap, HeaderName, HeaderValue, Request, StatusCode, Uri, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::process::Command;
use tokio::sync::mpsc;
use url::Url;
use uuid::Uuid;

use crate::{
    ai::{Message, run_agent},
    config::{AppConfig, ConfigPaths},
    events::AgentEvent,
    executor::ExecutionContext,
    history::HistoryStore,
    safety::SafetyMode,
    sync::{load_runtime_tools, load_schema},
};

fn try_open_in_browser(url: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn();
    }
}

/// Max in-memory HTTP `/run` session transcripts. Beyond this, older ids may be evicted.
const HTTP_SESSION_BUDGET: usize = 256;

#[derive(rust_embed::RustEmbed)]
#[folder = "embedded-web/dist"]
struct WebAssets;

#[derive(Debug, Clone)]
pub struct ServeOptions {
    /// `0` means bind an ephemeral (OS-assigned) port.
    pub port: u16,
    pub bind: String,
    pub token: Option<String>,
    pub identity_header: String,
    pub tunnel: bool,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub strict: bool,
    pub read_only: bool,
    pub dry_run: bool,
    pub confirm: bool,
    /// When true, best-effort open the local UI URL in the default browser.
    pub open_browser: bool,
}

#[derive(Clone)]
struct AppState {
    app_name: String,
    paths: ConfigPaths,
    config: AppConfig,
    options: ServeOptions,
    /// In-process chat transcripts keyed by browser/client session id.
    http_transcripts: Arc<Mutex<HashMap<String, Vec<Message>>>>,
}

#[derive(Debug, Deserialize)]
struct RunPayload {
    message: String,
    #[serde(default)]
    read_only: Option<bool>,
    #[serde(default)]
    dry_run: Option<bool>,
    #[serde(default)]
    confirm: Option<bool>,
    #[serde(default)]
    strict: Option<bool>,
    /// When set, continue the same conversation as earlier `/run` requests in this process.
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WsPayload {
    message: String,
    #[serde(default)]
    read_only: Option<bool>,
    #[serde(default)]
    dry_run: Option<bool>,
    #[serde(default)]
    confirm: Option<bool>,
    #[serde(default)]
    strict: Option<bool>,
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct WsAuthQuery {
    token: Option<String>,
}

pub async fn run_server(
    app_name: String,
    paths: ConfigPaths,
    config: AppConfig,
    options: ServeOptions,
) -> Result<()> {
    let state = Arc::new(AppState {
        app_name,
        paths,
        config,
        options,
        http_transcripts: Arc::new(Mutex::new(HashMap::new())),
    });

    let app = Router::new()
        .route("/tools", get(get_tools))
        .route("/history", get(get_history))
        .route("/schema", get(get_schema))
        .route("/config/public", get(get_config_public))
        .route("/run", post(post_run))
        .route("/chat", get(ws_chat))
        .fallback(get(serve_static))
        .layer(middleware::from_fn(harden_responses))
        .layer(middleware::from_fn(validate_browser_origin))
        .with_state(state.clone());

    let addr: SocketAddr = format!("{}:{}", state.options.bind, state.options.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local = listener.local_addr().context("listener local_addr")?;
    let base_url = format!("http://{local}");
    println!("appctl serve — {base_url}  (use Ctrl+C to stop)");
    if state.options.open_browser {
        let open_url = base_url.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            try_open_in_browser(&open_url);
        });
    }
    if state.options.tunnel {
        let target = base_url.clone();
        Command::new("cloudflared")
            .args(["tunnel", "--url", &target, "--no-autoupdate"])
            .spawn()?;
        println!("cloudflared tunnel started for {target}");
    }
    axum::serve(listener, app).await?;
    Ok(())
}

async fn validate_browser_origin(request: Request<Body>, next: Next) -> Response {
    if !browser_origin_ok(request.headers()) {
        return forbidden_err("cross-origin browser request blocked");
    }
    next.run(request).await
}

async fn harden_responses(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    set_default_header(
        headers,
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    set_default_header(headers, "x-frame-options", HeaderValue::from_static("DENY"));
    set_default_header(
        headers,
        "referrer-policy",
        HeaderValue::from_static("no-referrer"),
    );
    set_default_header(
        headers,
        "content-security-policy",
        HeaderValue::from_static(
            "default-src 'self'; connect-src 'self' ws: wss:; img-src 'self' data:; style-src 'self'; script-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
        ),
    );
    response
}

fn set_default_header(headers: &mut HeaderMap, name: &'static str, value: HeaderValue) {
    let name = HeaderName::from_static(name);
    if !headers.contains_key(&name) {
        headers.insert(name, value);
    }
}

fn auth_ok(state: &AppState, headers: &HeaderMap, query_token: Option<&str>) -> bool {
    state.options.token.as_ref().is_none_or(|expected| {
        query_token == Some(expected.as_str())
            || headers.get("x-appctl-token").and_then(|v| v.to_str().ok())
                == Some(expected.as_str())
            || headers
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|a| {
                    a == expected.as_str()
                        || a.strip_prefix("Bearer ").is_some_and(|t| t == expected)
                })
    })
}

fn browser_origin_ok(headers: &HeaderMap) -> bool {
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return true;
    };
    let Some(host_header) = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(header::HOST))
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Ok(origin_url) = Url::parse(origin) else {
        return false;
    };
    if !matches!(origin_url.scheme(), "http" | "https") {
        return false;
    }
    let Some(origin_host) = origin_url.host_str() else {
        return false;
    };
    let Some((request_host, request_port)) = parse_host_header(host_header) else {
        return false;
    };
    let scheme = origin_url.scheme();
    let origin_port = origin_url
        .port_or_known_default()
        .unwrap_or_else(|| default_port_for_scheme(scheme));
    let request_port = request_port.unwrap_or_else(|| default_port_for_scheme(scheme));
    request_host.eq_ignore_ascii_case(origin_host) && request_port == origin_port
}

fn parse_host_header(host: &str) -> Option<(String, Option<u16>)> {
    let candidate = host.split(',').next()?.trim();
    let parsed = Url::parse(&format!("http://{candidate}")).ok()?;
    Some((parsed.host_str()?.to_string(), parsed.port()))
}

fn default_port_for_scheme(scheme: &str) -> u16 {
    match scheme {
        "https" => 443,
        _ => 80,
    }
}

fn auth_err() -> Response {
    StatusCode::UNAUTHORIZED.into_response()
}

fn forbidden_err(message: &str) -> Response {
    (StatusCode::FORBIDDEN, Json(json!({ "error": message }))).into_response()
}

async fn serve_static(uri: Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/').to_string();
    if path.is_empty() {
        path = "index.html".into();
    }
    let file = WebAssets::get(&path).or_else(|| WebAssets::get("index.html"));
    match file {
        Some(c) => {
            let ct = content_type(&path);
            ([(header::CONTENT_TYPE, ct)], c.data).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn content_type(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "application/octet-stream"
    }
}

async fn get_tools(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, Response> {
    if !auth_ok(&state, &headers, None) {
        return Err(auth_err());
    }
    let tools = load_runtime_tools(&state.paths, &state.config).map_err(internal_error)?;
    Ok(Json(serde_json::to_value(tools).map_err(internal_error)?))
}

async fn get_schema(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, Response> {
    if !auth_ok(&state, &headers, None) {
        return Err(auth_err());
    }
    let schema = load_schema(&state.paths).map_err(internal_error)?;
    Ok(Json(serde_json::to_value(&schema).map_err(internal_error)?))
}

async fn get_config_public(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, Response> {
    if !auth_ok(&state, &headers, None) {
        return Err(auth_err());
    }
    let schema = load_schema(&state.paths).map_err(internal_error)?;
    let active_provider = state
        .options
        .provider
        .clone()
        .unwrap_or_else(|| state.config.default.clone());
    let banner_label = state.config.banner_label(&state.app_name);
    Ok(Json(json!({
        "app_name": state.app_name,
        "banner_label": banner_label,
        "display_name": state.config.display_name,
        "description": state.config.description,
        "default_provider": state.config.default,
        "active_provider": active_provider,
        "provider_statuses": state.config.provider_statuses_with_paths(&state.paths),
        "sync_source": schema.source,
        "base_url": schema.base_url,
        "read_only": state.options.read_only,
        "dry_run": state.options.dry_run,
        "strict": state.options.strict,
        "confirm_default": state.options.confirm,
    })))
}

async fn get_history(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Value>, Response> {
    if !auth_ok(&state, &headers, None) {
        return Err(auth_err());
    }
    let store = HistoryStore::open(&state.paths).map_err(internal_error)?;
    let entries = store
        .list(query.limit.unwrap_or(20))
        .map_err(internal_error)?;
    Ok(Json(serde_json::to_value(entries).map_err(internal_error)?))
}

async fn post_run(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<RunPayload>,
) -> Result<Json<Value>, Response> {
    if !auth_ok(&state, &headers, None) {
        return Err(auth_err());
    }
    let schema = load_schema(&state.paths).map_err(internal_error)?;
    let tools = load_runtime_tools(&state.paths, &state.config).map_err(internal_error)?;
    let (tx, mut rx) = mpsc::channel::<AgentEvent>(128);
    let paths = state.paths.clone();
    let config = state.config.clone();
    let prov = state.options.provider.clone();
    let model = state.options.model.clone();
    let safety = merge_safety(
        &state.options,
        payload.read_only,
        payload.dry_run,
        payload.confirm,
        payload.strict,
    );
    let msg = payload.message.clone();
    let client_id = client_id_from_headers(&state, &headers);
    let session_id = payload
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let prior = {
        let map = state
            .http_transcripts
            .lock()
            .map_err(|e| internal_error(format!("http session store: {e}")))?;
        map.get(&session_id).cloned().unwrap_or_default()
    };
    let resumed = !prior.is_empty();
    let prior_len = prior.len();

    let app_name = state.app_name.clone();
    let sid_run = session_id.clone();
    let prior_for_agent = prior.clone();
    let agent = tokio::spawn(async move {
        run_agent(
            &paths,
            &config,
            &app_name,
            prov.as_deref(),
            model.as_deref(),
            &msg,
            &prior_for_agent,
            &tools,
            &schema,
            ExecutionContext {
                session_id: sid_run,
                session_name: client_id.clone(),
                safety,
            },
            Some(tx),
        )
        .await
    });

    let mut events = vec![
        serde_json::to_value(AgentEvent::SessionState {
            session_id: session_id.clone(),
            transcript_len: prior_len,
            resumed,
        })
        .unwrap_or(Value::Null),
    ];
    while let Some(ev) = rx.recv().await {
        if let Ok(v) = serde_json::to_value(&ev) {
            events.push(v);
        }
    }

    let inner = match agent.await {
        Ok(r) => r,
        Err(e) => return Err(internal_error(e)),
    };
    let outcome = inner.map_err(internal_error)?;
    let response = outcome.response;
    let new_transcript = outcome.transcript;

    {
        let mut map = state
            .http_transcripts
            .lock()
            .map_err(|e| internal_error(format!("http session store: {e}")))?;
        evict_http_sessions_if_needed(&mut map);
        map.insert(session_id.clone(), new_transcript);
    }

    Ok(Json(
        json!({ "result": response, "events": events, "session_id": session_id }),
    ))
}

async fn ws_chat(
    ws: WebSocketUpgrade,
    Query(q): Query<WsAuthQuery>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, Response> {
    if !auth_ok(&state, &headers, q.token.as_deref()) {
        return Err(auth_err());
    }
    let client_id = client_id_from_headers(&state, &headers);
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state, client_id)))
}

async fn handle_socket(
    socket: axum::extract::ws::WebSocket,
    state: Arc<AppState>,
    client_id: Option<String>,
) {
    let mut current_session_id: Option<String> = None;
    let (mut sink, mut stream) = socket.split();
    while let Some(Ok(msg)) = stream.next().await {
        let axum::extract::ws::Message::Text(text) = msg else {
            continue;
        };
        let raw = text.as_str();
        let (message, safety, requested_session_id) = merge_safety_ws(raw, &state.options);
        let session_id = requested_session_id
            .or_else(|| current_session_id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        current_session_id = Some(session_id.clone());
        let prior = match state
            .http_transcripts
            .lock()
            .map(|map| map.get(&session_id).cloned().unwrap_or_default())
            .map_err(|error| format!("chat session store: {error}"))
        {
            Ok(prior) => prior,
            Err(message) => {
                let line = serde_json::to_string(&AgentEvent::Error { message });
                if let Ok(line) = line {
                    let _ = sink
                        .send(axum::extract::ws::Message::Text(line.into()))
                        .await;
                }
                continue;
            }
        };
        let resumed = !prior.is_empty();
        if let Ok(line) = serde_json::to_string(&AgentEvent::SessionState {
            session_id: session_id.clone(),
            transcript_len: prior.len(),
            resumed,
        }) {
            if sink
                .send(axum::extract::ws::Message::Text(line.into()))
                .await
                .is_err()
            {
                break;
            }
        }
        let (tx, mut rx) = mpsc::channel::<AgentEvent>(128);
        let paths = state.paths.clone();
        let config = state.config.clone();
        let prov = state.options.provider.clone();
        let model = state.options.model.clone();
        let sid = session_id.clone();
        let request_client_id = client_id.clone();
        let app_name = state.app_name.clone();
        let agent = tokio::spawn(async move {
            let schema = match load_schema(&paths) {
                Ok(s) => s,
                Err(e) => return Err(e),
            };
            let tools = match load_runtime_tools(&paths, &config) {
                Ok(t) => t,
                Err(e) => return Err(e),
            };
            run_agent(
                &paths,
                &config,
                &app_name,
                prov.as_deref(),
                model.as_deref(),
                &message,
                &prior,
                &tools,
                &schema,
                ExecutionContext {
                    session_id: sid,
                    session_name: request_client_id,
                    safety,
                },
                Some(tx),
            )
            .await
        });

        while let Some(ev) = rx.recv().await {
            let line = match serde_json::to_string(&ev) {
                Ok(s) => s,
                Err(_) => continue,
            };
            if sink
                .send(axum::extract::ws::Message::Text(line.into()))
                .await
                .is_err()
            {
                break;
            }
        }
        match agent.await {
            Ok(Ok(outcome)) => {
                let store_result = state
                    .http_transcripts
                    .lock()
                    .map_err(|error| format!("chat session store: {error}"))
                    .map(|mut map| {
                        evict_http_sessions_if_needed(&mut map);
                        map.insert(session_id.clone(), outcome.transcript);
                    });
                if let Err(message) = store_result {
                    let line = serde_json::to_string(&AgentEvent::Error { message });
                    if let Ok(line) = line {
                        let _ = sink
                            .send(axum::extract::ws::Message::Text(line.into()))
                            .await;
                    }
                }
            }
            Ok(Err(error)) => {
                let line = serde_json::to_string(&AgentEvent::Error {
                    message: error.to_string(),
                });
                if let Ok(line) = line {
                    let _ = sink
                        .send(axum::extract::ws::Message::Text(line.into()))
                        .await;
                }
            }
            Err(error) => {
                let line = serde_json::to_string(&AgentEvent::Error {
                    message: error.to_string(),
                });
                if let Ok(line) = line {
                    let _ = sink
                        .send(axum::extract::ws::Message::Text(line.into()))
                        .await;
                }
            }
        }
    }
}

fn client_id_from_headers(state: &AppState, headers: &HeaderMap) -> Option<String> {
    headers
        .get(state.options.identity_header.as_str())
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn merge_safety(
    opts: &ServeOptions,
    read_only: Option<bool>,
    dry_run: Option<bool>,
    confirm: Option<bool>,
    strict: Option<bool>,
) -> SafetyMode {
    SafetyMode {
        // Clients may ask for extra safety, but they may not weaken the server policy.
        read_only: opts.read_only || read_only.unwrap_or(false),
        dry_run: opts.dry_run || dry_run.unwrap_or(false),
        confirm: opts.confirm && confirm.unwrap_or(true),
        strict: opts.strict || strict.unwrap_or(false),
    }
}

/// Plain string prompts are accepted; JSON can override safety and carry a resumable `session_id`.
fn merge_safety_ws(raw: &str, opts: &ServeOptions) -> (String, SafetyMode, Option<String>) {
    if let Ok(payload) = serde_json::from_str::<WsPayload>(raw) {
        return (
            payload.message,
            merge_safety(
                opts,
                payload.read_only,
                payload.dry_run,
                payload.confirm,
                payload.strict,
            ),
            payload
                .session_id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        );
    }
    (
        raw.to_string(),
        merge_safety(opts, None, None, None, None),
        None,
    )
}

fn evict_http_sessions_if_needed(sessions: &mut HashMap<String, Vec<Message>>) {
    if sessions.len() < HTTP_SESSION_BUDGET {
        return;
    }
    // Drop a batch of keys to make room. HashMap iteration order is not meaningful here.
    let to_remove: Vec<String> = sessions.keys().take(64).cloned().collect();
    for k in to_remove {
        sessions.remove(&k);
    }
}

fn internal_error(error: impl ToString) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": error.to_string() })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_origin_allows_non_browser_clients() {
        let headers = HeaderMap::new();
        assert!(browser_origin_ok(&headers));
    }

    #[test]
    fn browser_origin_allows_same_host() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:4242"));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://127.0.0.1:4242"),
        );
        assert!(browser_origin_ok(&headers));
    }

    #[test]
    fn browser_origin_rejects_cross_site_requests() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:4242"));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://evil.example"),
        );
        assert!(!browser_origin_ok(&headers));
    }

    #[test]
    fn merge_safety_never_relaxes_server_policy() {
        let opts = ServeOptions {
            port: 4242,
            bind: "127.0.0.1".to_string(),
            token: None,
            identity_header: "x-appctl-client-id".to_string(),
            tunnel: false,
            provider: None,
            model: None,
            strict: true,
            read_only: true,
            dry_run: true,
            confirm: false,
            open_browser: false,
        };
        let merged = merge_safety(&opts, Some(false), Some(false), Some(true), Some(false));
        assert!(merged.read_only);
        assert!(merged.dry_run);
        assert!(merged.strict);
        assert!(!merged.confirm);
    }

    #[test]
    fn merge_safety_allows_extra_client_restrictions() {
        let opts = ServeOptions {
            port: 4242,
            bind: "127.0.0.1".to_string(),
            token: None,
            identity_header: "x-appctl-client-id".to_string(),
            tunnel: false,
            provider: None,
            model: None,
            strict: false,
            read_only: false,
            dry_run: false,
            confirm: true,
            open_browser: false,
        };
        let merged = merge_safety(&opts, Some(true), Some(true), Some(false), Some(true));
        assert!(merged.read_only);
        assert!(merged.dry_run);
        assert!(merged.strict);
        assert!(!merged.confirm);
    }
}
