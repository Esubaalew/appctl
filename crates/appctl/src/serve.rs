use std::{net::SocketAddr, sync::Arc};

use anyhow::Result;
use axum::{
    Json, Router,
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    ai::run_agent,
    config::{AppConfig, ConfigPaths},
    executor::ExecutionContext,
    history::HistoryStore,
    safety::SafetyMode,
    sync::{load_schema, load_tools},
};

#[derive(Debug, Clone)]
pub struct ServeOptions {
    pub port: u16,
    pub bind: String,
    pub token: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[derive(Clone)]
struct AppState {
    paths: ConfigPaths,
    config: AppConfig,
    options: ServeOptions,
}

#[derive(Debug, Deserialize)]
struct RunPayload {
    message: String,
}

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    limit: Option<usize>,
}

pub async fn run_server(
    paths: ConfigPaths,
    config: AppConfig,
    options: ServeOptions,
) -> Result<()> {
    let state = Arc::new(AppState {
        paths,
        config,
        options,
    });

    let app = Router::new()
        .route("/tools", get(get_tools))
        .route("/history", get(get_history))
        .route("/run", post(post_run))
        .route("/chat", get(ws_chat))
        .with_state(state.clone());

    let addr: SocketAddr = format!("{}:{}", state.options.bind, state.options.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("appctl serve listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn get_tools(State(state): State<Arc<AppState>>) -> Result<Json<Value>, Response> {
    let tools = load_tools(&state.paths).map_err(internal_error)?;
    Ok(Json(serde_json::to_value(tools).map_err(internal_error)?))
}

async fn get_history(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Value>, Response> {
    let store = HistoryStore::open(&state.paths).map_err(internal_error)?;
    let entries = store
        .list(query.limit.unwrap_or(20))
        .map_err(internal_error)?;
    Ok(Json(serde_json::to_value(entries).map_err(internal_error)?))
}

async fn post_run(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RunPayload>,
) -> Result<Json<Value>, Response> {
    let schema = load_schema(&state.paths).map_err(internal_error)?;
    let tools = load_tools(&state.paths).map_err(internal_error)?;
    let response = run_agent(
        &state.paths,
        &state.config,
        state.options.provider.as_deref(),
        state.options.model.as_deref(),
        &payload.message,
        &tools,
        &schema,
        ExecutionContext {
            session_id: Uuid::new_v4().to_string(),
            safety: SafetyMode {
                read_only: false,
                dry_run: false,
                confirm: true,
            },
        },
    )
    .await
    .map_err(internal_error)?;

    Ok(Json(json!({ "result": response })))
}

async fn ws_chat(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    while let Some(Ok(message)) = socket.next().await {
        match message {
            Message::Text(text) => {
                let reply = match handle_ws_prompt(&state, &text).await {
                    Ok(value) => serde_json::to_string(&json!({ "result": value })).unwrap(),
                    Err(err) => {
                        serde_json::to_string(&json!({ "error": err.to_string() })).unwrap()
                    }
                };
                let _ = socket.send(Message::Text(reply.into())).await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}

async fn handle_ws_prompt(state: &AppState, prompt: &str) -> Result<Value> {
    let schema = load_schema(&state.paths)?;
    let tools = load_tools(&state.paths)?;
    run_agent(
        &state.paths,
        &state.config,
        state.options.provider.as_deref(),
        state.options.model.as_deref(),
        prompt,
        &tools,
        &schema,
        ExecutionContext {
            session_id: Uuid::new_v4().to_string(),
            safety: SafetyMode {
                read_only: false,
                dry_run: false,
                confirm: true,
            },
        },
    )
    .await
}

fn internal_error(error: impl ToString) -> Response {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": error.to_string() })),
    )
        .into_response()
}
