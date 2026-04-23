use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Query, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{
    ai::{Message, run_agent},
    config::{AppConfig, ConfigPaths},
    events::AgentEvent,
    executor::ExecutionContext,
    history::HistoryStore,
    safety::SafetyMode,
    sync::{load_schema, load_tools},
};

/// Max in-memory HTTP `/run` session transcripts. Beyond this, older ids may be evicted.
const HTTP_SESSION_BUDGET: usize = 256;

#[derive(rust_embed::RustEmbed)]
#[folder = "embedded-web/dist"]
struct WebAssets;

#[derive(Debug, Clone)]
pub struct ServeOptions {
    pub port: u16,
    pub bind: String,
    pub token: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub strict: bool,
    pub read_only: bool,
    pub dry_run: bool,
    pub confirm: bool,
}

#[derive(Clone)]
struct AppState {
    app_name: String,
    paths: ConfigPaths,
    config: AppConfig,
    options: ServeOptions,
    /// `POST /run` sessions only (key = client-supplied or server-issued `session_id`).
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
        .with_state(state.clone());

    let addr: SocketAddr = format!("{}:{}", state.options.bind, state.options.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("appctl serve listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
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

fn auth_err() -> Response {
    StatusCode::UNAUTHORIZED.into_response()
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
    let tools = load_tools(&state.paths).map_err(internal_error)?;
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
    Ok(Json(json!({
        "app_name": state.app_name,
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
    let tools = load_tools(&state.paths).map_err(internal_error)?;
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

    let sid_run = session_id.clone();
    let agent = tokio::spawn(async move {
        run_agent(
            &paths,
            &config,
            prov.as_deref(),
            model.as_deref(),
            &msg,
            &prior,
            &tools,
            &schema,
            ExecutionContext {
                session_id: sid_run,
                safety,
            },
            Some(tx),
        )
        .await
    });

    let mut events = Vec::new();
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
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state)))
}

async fn handle_socket(socket: axum::extract::ws::WebSocket, state: Arc<AppState>) {
    let session_id = Uuid::new_v4().to_string();
    let mut transcript: Vec<Message> = Vec::new();
    let (mut sink, mut stream) = socket.split();
    while let Some(Ok(msg)) = stream.next().await {
        let axum::extract::ws::Message::Text(text) = msg else {
            continue;
        };
        let raw = text.as_str();
        let (message, safety) = merge_safety_ws(raw, &state.options);
        let (tx, mut rx) = mpsc::channel::<AgentEvent>(128);
        let paths = state.paths.clone();
        let config = state.config.clone();
        let prov = state.options.provider.clone();
        let model = state.options.model.clone();
        let prior = transcript.clone();
        let sid = session_id.clone();
        let agent = tokio::spawn(async move {
            let schema = match load_schema(&paths) {
                Ok(s) => s,
                Err(e) => return Err(e),
            };
            let tools = match load_tools(&paths) {
                Ok(t) => t,
                Err(e) => return Err(e),
            };
            run_agent(
                &paths,
                &config,
                prov.as_deref(),
                model.as_deref(),
                &message,
                &prior,
                &tools,
                &schema,
                ExecutionContext {
                    session_id: sid,
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
                transcript = outcome.transcript;
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

fn merge_safety(
    opts: &ServeOptions,
    read_only: Option<bool>,
    dry_run: Option<bool>,
    confirm: Option<bool>,
    strict: Option<bool>,
) -> SafetyMode {
    SafetyMode {
        read_only: read_only.unwrap_or(opts.read_only),
        dry_run: dry_run.unwrap_or(opts.dry_run),
        confirm: confirm.unwrap_or(opts.confirm),
        strict: strict.unwrap_or(opts.strict),
    }
}

/// Plain string prompts are accepted; JSON `{"message":"...","read_only":true}` overrides safety for that turn.
fn merge_safety_ws(raw: &str, opts: &ServeOptions) -> (String, SafetyMode) {
    if let Ok(v) = serde_json::from_str::<Value>(raw) {
        if let Some(obj) = v.as_object() {
            if let Some(msg) = obj.get("message").and_then(|x| x.as_str()) {
                let read_only = obj.get("read_only").and_then(|x| x.as_bool());
                let dry_run = obj.get("dry_run").and_then(|x| x.as_bool());
                let confirm = obj.get("confirm").and_then(|x| x.as_bool());
                let strict = obj.get("strict").and_then(|x| x.as_bool());
                return (
                    msg.to_string(),
                    merge_safety(opts, read_only, dry_run, confirm, strict),
                );
            }
        }
    }
    (raw.to_string(), merge_safety(opts, None, None, None, None))
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
