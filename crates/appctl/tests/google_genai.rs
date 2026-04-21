use std::collections::BTreeMap;

use appctl::{
    ai::{LlmProvider, Message, google_genai::GoogleGenaiProvider},
    auth::provider::{
        ProviderAuthKind, ProviderAuthOrigin, ProviderAuthStatus, ResolvedProviderAuth,
    },
    config::{ProviderKind, ResolvedProvider},
    tools::ToolDef,
};
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, header, method, path},
};

#[tokio::test]
async fn google_genai_provider_sends_api_key_and_parses_function_call() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.5-pro:generateContent"))
        .and(header("x-goog-api-key", "test-google-key"))
        .and(body_string_contains("\"functionDeclarations\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "functionCall": {
                            "id": "call-1",
                            "name": "create_widget",
                            "args": {
                                "name": "Demo"
                            }
                        }
                    }]
                }
            }]
        })))
        .mount(&server)
        .await;

    let provider = GoogleGenaiProvider::new(ResolvedProvider {
        name: "gemini".to_string(),
        kind: ProviderKind::GoogleGenai,
        base_url: server.uri(),
        model: "gemini-2.5-pro".to_string(),
        auth: ResolvedProviderAuth::ApiKey {
            value: "test-google-key".to_string(),
            status: ProviderAuthStatus {
                kind: ProviderAuthKind::ApiKey,
                origin: ProviderAuthOrigin::Explicit,
                configured: true,
                secret_ref: Some("GOOGLE_API_KEY".to_string()),
                profile: None,
                expires_at: None,
                scopes: Vec::new(),
                project_id: None,
                recovery_hint: None,
            },
        },
        auth_status: ProviderAuthStatus {
            kind: ProviderAuthKind::ApiKey,
            origin: ProviderAuthOrigin::Explicit,
            configured: true,
            secret_ref: Some("GOOGLE_API_KEY".to_string()),
            profile: None,
            expires_at: None,
            scopes: Vec::new(),
            project_id: None,
            recovery_hint: None,
        },
        extra_headers: BTreeMap::new(),
    });

    let tools = vec![ToolDef {
        name: "create_widget".to_string(),
        description: "Create a widget".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        }),
    }];
    let messages = vec![Message {
        role: "user".to_string(),
        content: "Create a widget named Demo".to_string(),
        tool_calls: Vec::new(),
        tool_call_id: None,
        tool_name: None,
    }];

    let step = provider
        .chat(&messages, &tools)
        .await
        .expect("gemini request");
    match step {
        appctl::ai::AgentStep::ToolCalls { calls } => {
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].name, "create_widget");
            assert_eq!(calls[0].arguments["name"], "Demo");
        }
        other => panic!("expected tool call, got {other:?}"),
    }
}
