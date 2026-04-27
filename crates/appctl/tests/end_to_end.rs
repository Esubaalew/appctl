use std::collections::BTreeMap;

use appctl::{
    ai::run_agent,
    auth::provider::ProviderAuthConfig,
    config::{AppConfig, BehaviorConfig, ConfigPaths, ProviderConfig, ProviderKind, TargetConfig},
    executor::ExecutionContext,
    safety::SafetyMode,
    schema::{
        Action, AuthStrategy, Field, FieldType, HttpMethod, ParameterLocation, Provenance,
        Resource, Safety, Schema, SyncSource, Transport, Verb,
    },
    tools::schema_to_tools,
};
use serde_json::json;
use tempfile::tempdir;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, method, path},
};

#[tokio::test]
async fn run_agent_executes_tool_call_and_returns_follow_up_message() {
    let llm = MockServer::start().await;
    let target = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_string_contains("\"role\":\"tool\""))
        .and(body_string_contains("\"tool_call_id\":\"call_1\""))
        .and(body_string_contains("\"tool_calls\":["))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{
                "message": {
                    "content": "Created pet Rex",
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
                            "name": "create_pet",
                            "arguments": "{\"name\":\"Rex\"}"
                        }
                    }]
                }
            }]
        })))
        .mount(&llm)
        .await;

    Mock::given(method("POST"))
        .and(path("/pets"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 1,
            "name": "Rex"
        })))
        .mount(&target)
        .await;

    let dir = tempdir().unwrap();
    let paths = ConfigPaths::new(dir.path().join(".appctl"));
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
            extra_headers: BTreeMap::new(),
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

    let schema = Schema {
        source: SyncSource::Openapi,
        base_url: Some(target.uri()),
        auth: AuthStrategy::None,
        resources: vec![Resource {
            name: "pet".to_string(),
            description: None,
            fields: vec![],
            actions: vec![Action {
                name: "create_pet".to_string(),
                description: Some("Create a pet".to_string()),
                verb: Verb::Create,
                transport: Transport::Http {
                    method: HttpMethod::POST,
                    path: "/pets".to_string(),
                    query: vec![],
                },
                parameters: vec![Field {
                    name: "name".to_string(),
                    description: None,
                    field_type: FieldType::String,
                    required: true,
                    location: Some(ParameterLocation::Body),
                    default: None,
                    enum_values: vec![],
                }],
                safety: Safety::Mutating,
                resource: Some("pet".to_string()),
                provenance: Provenance::Declared,
                metadata: Default::default(),
            }],
            metadata: Default::default(),
        }],
        metadata: Default::default(),
    };
    let tools = schema_to_tools(&schema);

    let outcome = run_agent(
        &paths,
        &config,
        "test-app",
        Some("mock"),
        None,
        "Create a pet named Rex",
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
    .unwrap();

    assert_eq!(outcome.response, json!("Created pet Rex"));
}
