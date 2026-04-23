//! End-to-end: Django+DRF fixture sync, then stub LLM drives an HTTP tool against a mock API.
use std::path::PathBuf;

use appctl::{
    ai::run_agent,
    auth::provider::ProviderAuthConfig,
    config::{AppConfig, BehaviorConfig, ConfigPaths, ProviderConfig, ProviderKind, TargetConfig},
    executor::ExecutionContext,
    safety::SafetyMode,
    sync::{SyncRequest, load_schema, load_tools, run_sync},
};
use serde_json::json;
use tempfile::tempdir;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, method, path},
};

#[tokio::test]
async fn sync_django_fixture_then_agent_calls_create_parcel() {
    let llm = MockServer::start().await;
    let api = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/parcel/"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 1,
            "tracking_number": "TRK-1"
        })))
        .mount(&api)
        .await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_string_contains("\"role\":\"tool\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{
                "message": {
                    "content": "Parcel created",
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
                            "name": "create_parcel",
                            "arguments": "{\"tracking_number\":\"TRK-1\"}"
                        }
                    }]
                }
            }]
        })))
        .mount(&llm)
        .await;

    let dir = tempdir().unwrap();
    let paths = ConfigPaths::new(dir.path().join(".appctl"));
    let django_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/django_app");

    run_sync(
        paths.clone(),
        SyncRequest {
            django: Some(django_root),
            base_url: Some(api.uri()),
            auth_header: Some("Bearer test-token-for-e2e".to_string()),
            force: true,
            ..Default::default()
        },
    )
    .await
    .expect("sync");

    let schema = load_schema(&paths).expect("schema");
    let tools = load_tools(&paths).expect("tools");
    assert!(
        tools.iter().any(|t| t.name == "create_parcel"),
        "expected create_parcel in {:?}",
        tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

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
        },
    };
    config.save(&paths).unwrap();

    let outcome = run_agent(
        &paths,
        &config,
        Some("mock"),
        None,
        "Create a parcel with tracking number TRK-1",
        &[],
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

    assert_eq!(outcome.response, json!("Parcel created"));
}
