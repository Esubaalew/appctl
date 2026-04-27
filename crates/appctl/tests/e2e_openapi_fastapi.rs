//! End-to-end: sync from committed OpenAPI demo spec, then run agent with stub LLM + live HTTP tool.
use std::path::PathBuf;

use appctl::{
    ai::run_agent,
    auth::provider::ProviderAuthConfig,
    config::{AppConfig, BehaviorConfig, ConfigPaths, ProviderConfig, ProviderKind, TargetConfig},
    executor::{ExecutionContext, ExecutionRequest, Executor},
    safety::SafetyMode,
    sync::{SyncRequest, load_tools, run_sync},
};
use serde_json::json;
use tempfile::tempdir;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, header, method, path, query_param},
};

#[tokio::test]
async fn sync_openapi_demo_then_agent_calls_http_tool() {
    let llm = MockServer::start().await;
    let api = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/widgets"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 42,
            "name": "Demo"
        })))
        .mount(&api)
        .await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_string_contains("\"role\":\"tool\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{
                "message": {
                    "content": "Created widget Demo",
                    "tool_calls": []
                }
            }]
        })))
        .mount(&llm)
        .await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_string_contains("\"role\":\"user\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{
                "message": {
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "create_widget",
                            "arguments": "{\"name\":\"Demo\"}"
                        }
                    }]
                }
            }]
        })))
        .mount(&llm)
        .await;

    let dir = tempdir().unwrap();
    let root = dir.path().join(".appctl");
    let paths = ConfigPaths::new(root);
    let openapi = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demos/openapi-fastapi/openapi.json");
    run_sync(
        paths.clone(),
        SyncRequest {
            openapi: Some(openapi.to_string_lossy().to_string()),
            base_url: Some(api.uri()),
            force: true,
            ..Default::default()
        },
    )
    .await
    .expect("sync");

    let tools = load_tools(&paths).expect("tools");
    let tool_name = tools
        .iter()
        .find(|t| t.name == "create_widget")
        .map(|t| t.name.clone())
        .expect("create_widget tool");

    let config = AppConfig {
        default: "mock".to_string(),
        providers: vec![ProviderConfig {
            name: "mock".to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: format!("{}/", llm.uri()),
            model: "mock-model".to_string(),
            verified: true,
            auth: Some(ProviderAuthConfig::None),
            api_key_ref: None,
            extra_headers: Default::default(),
        }],
        target: TargetConfig::default(),
        cloud: Default::default(),
        behavior: BehaviorConfig {
            max_iterations: 4,
            history_limit: 50,
            ..Default::default()
        },
        tooling: Default::default(),
        display_name: None,
        description: None,
    };
    config.save(&paths).unwrap();

    let schema = appctl::sync::load_schema(&paths).unwrap();
    assert_eq!(tool_name, "create_widget");

    let outcome = run_agent(
        &paths,
        &config,
        "test-app",
        Some("mock"),
        None,
        "Create a widget named Demo",
        &[],
        &tools,
        &schema,
        ExecutionContext {
            session_id: Uuid::new_v4().to_string(),
            session_name: None,
            safety: SafetyMode {
                read_only: false,
                dry_run: false,
                confirm: true,
                strict: false,
            },
        },
        None,
    )
    .await
    .expect("agent");

    assert_eq!(outcome.response, json!("Created widget Demo"));
}

#[tokio::test]
async fn sync_openapi_protected_route_sends_runtime_auth_query_and_header_params() {
    let api = MockServer::start().await;
    unsafe {
        std::env::set_var("APPCTL_TEST_QUERY_API_KEY", "query-secret");
    }

    Mock::given(method("GET"))
        .and(path("/protected"))
        .and(query_param("api_key", "query-secret"))
        .and(header("Authorization", "Bearer runtime-token"))
        .and(header("X-Trace", "trace-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true
        })))
        .mount(&api)
        .await;

    let dir = tempdir().unwrap();
    let root = dir.path().join(".appctl");
    let paths = ConfigPaths::new(root);
    let spec_path = dir.path().join("openapi.json");
    std::fs::write(
        &spec_path,
        serde_json::to_string(&json!({
            "openapi": "3.0.0",
            "info": { "title": "Protected", "version": "1.0.0" },
            "servers": [{ "url": api.uri() }],
            "security": [{ "APPCTL_TEST_QUERY_API_KEY": [] }],
            "components": {
                "securitySchemes": {
                    "APPCTL_TEST_QUERY_API_KEY": {
                        "type": "apiKey",
                        "in": "query",
                        "name": "api_key"
                    }
                }
            },
            "paths": {
                "/protected": {
                    "get": {
                        "operationId": "getProtected",
                        "tags": ["protected"],
                        "parameters": [{
                            "name": "X-Trace",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string" }
                        }],
                        "responses": { "200": { "description": "ok" } }
                    }
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();

    run_sync(
        paths.clone(),
        SyncRequest {
            openapi: Some(spec_path.to_string_lossy().to_string()),
            auth_header: Some("Authorization: Bearer runtime-token".to_string()),
            force: true,
            ..Default::default()
        },
    )
    .await
    .expect("sync");
    AppConfig::default().save(&paths).unwrap();

    let schema = appctl::sync::load_schema(&paths).unwrap();
    let executor = Executor::new(&paths).unwrap();
    let result = executor
        .execute(
            &schema,
            ExecutionContext {
                session_id: Uuid::new_v4().to_string(),
                session_name: None,
                safety: SafetyMode {
                    read_only: false,
                    dry_run: false,
                    confirm: true,
                    strict: false,
                },
            },
            ExecutionRequest::new("getprotected".to_string(), json!({ "X-Trace": "trace-1" })),
        )
        .await
        .expect("execute");

    assert_eq!(result.output["ok"], true);
    assert_eq!(result.output["status"], 200);
}
