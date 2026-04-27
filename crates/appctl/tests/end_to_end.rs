use std::collections::BTreeMap;

use appctl::{
    ai::run_agent,
    auth::provider::ProviderAuthConfig,
    config::{AppConfig, BehaviorConfig, ConfigPaths, ProviderConfig, ProviderKind, TargetConfig},
    executor::ExecutionContext,
    safety::SafetyMode,
    schema::{
        Action, AuthStrategy, DatabaseKind, Field, FieldType, HttpMethod, ParameterLocation,
        Provenance, Resource, Safety, Schema, SqlOperation, SyncSource, Transport, Verb,
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

#[tokio::test]
async fn run_agent_chains_filtered_db_list_results() {
    let llm = MockServer::start().await;
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("lookup.db");
    let connection = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&connection)
        .await
        .unwrap();

    sqlx::query(
        "create table land_record (
            id text primary key,
            parcel_id text not null,
            uic text,
            old_code text
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "create table land_right_owner (
            id text primary key,
            parcel_id text not null,
            party_id text not null
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into land_record (id, parcel_id, uic, old_code)
         values ('record-1', 'parcel-1', 'AR001023003010', 'DD001023003010')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into land_right_owner (id, parcel_id, party_id)
         values ('owner-row-1', 'parcel-1', 'owner-party-1')",
    )
    .execute(&pool)
    .await
    .unwrap();
    drop(pool);

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_string_contains("owner-party-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{
                "message": {
                    "content": "The owner party id is owner-party-1.",
                    "tool_calls": []
                }
            }]
        })))
        .with_priority(1)
        .mount(&llm)
        .await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_string_contains("parcel-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{
                "message": {
                    "tool_calls": [{
                        "id": "call_owner",
                        "type": "function",
                        "function": {
                            "name": "list_land_right_owner",
                            "arguments": "{\"filter\":{\"parcel_id\":\"parcel-1\"}}"
                        }
                    }]
                }
            }]
        })))
        .with_priority(2)
        .mount(&llm)
        .await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_string_contains("DD001023003010"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{
                "message": {
                    "tool_calls": [{
                        "id": "call_record",
                        "type": "function",
                        "function": {
                            "name": "list_land_record",
                            "arguments": "{\"filter\":{\"old_code\":\"DD001023003010\"}}"
                        }
                    }]
                }
            }]
        })))
        .with_priority(3)
        .mount(&llm)
        .await;

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
        target: TargetConfig {
            database_url: Some(connection),
            ..Default::default()
        },
        cloud: Default::default(),
        behavior: BehaviorConfig {
            max_iterations: 6,
            history_limit: 50,
            max_tool_result_chars: 0,
            ..Default::default()
        },
        tooling: Default::default(),
        display_name: None,
        description: None,
    };
    config.save(&paths).unwrap();

    let schema = Schema {
        source: SyncSource::Db,
        base_url: None,
        auth: AuthStrategy::None,
        resources: vec![
            db_resource(
                "land_record",
                "land_record",
                vec!["id", "parcel_id", "uic", "old_code"],
            ),
            db_resource(
                "land_right_owner",
                "land_right_owner",
                vec!["id", "parcel_id", "party_id"],
            ),
        ],
        metadata: Default::default(),
    };
    let tools = schema_to_tools(&schema);

    let outcome = run_agent(
        &paths,
        &config,
        "cis10",
        Some("mock"),
        None,
        "DD001023003010 for this upin what is the owner",
        &[],
        &tools,
        &schema,
        ExecutionContext {
            session_id: Uuid::new_v4().to_string(),
            session_name: None,
            safety: SafetyMode {
                read_only: true,
                dry_run: false,
                confirm: true,
                strict: false,
            },
        },
        None,
    )
    .await
    .unwrap();

    assert_eq!(
        outcome.response,
        json!("The owner party id is owner-party-1.")
    );
}

fn db_resource(name: &str, table: &str, columns: Vec<&str>) -> Resource {
    Resource {
        name: name.to_string(),
        description: Some(format!("Table {table}")),
        fields: columns
            .iter()
            .map(|column| Field {
                name: (*column).to_string(),
                description: None,
                field_type: FieldType::String,
                required: false,
                location: Some(ParameterLocation::Body),
                default: None,
                enum_values: vec![],
            })
            .collect(),
        actions: vec![Action {
            name: format!("list_{name}"),
            description: Some(format!("List rows from {table}")),
            verb: Verb::List,
            transport: Transport::Sql {
                database_kind: DatabaseKind::Sqlite,
                schema: None,
                table: table.to_string(),
                operation: SqlOperation::Select,
                primary_key: Some("id".to_string()),
            },
            parameters: vec![Field {
                name: "filter".to_string(),
                description: Some("Exact-match filters by column".to_string()),
                field_type: FieldType::Object,
                required: false,
                location: Some(ParameterLocation::Body),
                default: None,
                enum_values: vec![],
            }],
            safety: Safety::ReadOnly,
            resource: Some(name.to_string()),
            provenance: Provenance::Declared,
            metadata: Default::default(),
        }],
        metadata: Default::default(),
    }
}
