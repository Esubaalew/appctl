//! End-to-end: sync from committed OpenAPI demo spec, then run agent with stub LLM + live HTTP tool.
use std::path::PathBuf;

use appctl::{
    ai::run_agent,
    auth::provider::ProviderAuthConfig,
    config::{AppConfig, BehaviorConfig, ConfigPaths, ProviderConfig, ProviderKind, TargetConfig},
    executor::ExecutionContext,
    safety::SafetyMode,
    sync::{SyncRequest, load_tools, run_sync},
};
use serde_json::json;
use tempfile::tempdir;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, method, path},
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
            auth: Some(ProviderAuthConfig::None),
            api_key_ref: None,
            extra_headers: Default::default(),
        }],
        target: TargetConfig::default(),
        cloud: Default::default(),
        behavior: BehaviorConfig {
            max_iterations: 4,
            history_limit: 50,
        },
    };
    config.save(&paths).unwrap();

    let schema = appctl::sync::load_schema(&paths).unwrap();
    assert_eq!(tool_name, "create_widget");

    let response = run_agent(
        &paths,
        &config,
        Some("mock"),
        None,
        "Create a widget named Demo",
        &tools,
        &schema,
        ExecutionContext {
            session_id: Uuid::new_v4().to_string(),
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

    assert_eq!(response, json!("Created widget Demo"));
}
