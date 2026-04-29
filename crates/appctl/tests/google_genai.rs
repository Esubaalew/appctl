use std::collections::BTreeMap;

use appctl::{
    ai::{LlmProvider, Message, google_genai::GoogleGenaiProvider},
    auth::provider::{
        ProviderAuthKind, ProviderAuthOrigin, ProviderAuthStatus, ResolvedProviderAuth,
    },
    config::{ProviderKind, ResolvedProvider},
    events::AgentEvent,
    tools::ToolDef,
};
use serde_json::json;
use tokio::sync::mpsc;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, header, method, path},
};

#[tokio::test]
async fn google_genai_provider_sends_api_key_and_parses_function_call() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.5-pro:streamGenerateContent"))
        .and(header("x-goog-api-key", "test-google-key"))
        .and(body_string_contains("\"functionDeclarations\""))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"id\":\"call-1\",\"name\":\"create_widget\",\"args\":{\"name\":\"Demo\"}}}]}}]}\n\n",
        ))
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
                help_url: None,
                bridge_client: None,
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
            help_url: None,
            bridge_client: None,
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
        .chat(&messages, &tools, None)
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

#[tokio::test]
async fn google_genai_provider_emits_text_deltas_from_stream() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.5-pro:streamGenerateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_string(concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hel\"}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"lo\"}]}}]}\n\n",
        )))
        .mount(&server)
        .await;

    let provider = GoogleGenaiProvider::new(test_provider(server.uri()));
    let messages = vec![Message {
        role: "user".to_string(),
        content: "Say hello".to_string(),
        tool_calls: Vec::new(),
        tool_call_id: None,
        tool_name: None,
    }];
    let (tx, mut rx) = mpsc::channel(8);

    let step = provider
        .chat(&messages, &[], Some(tx))
        .await
        .expect("gemini stream");

    match step {
        appctl::ai::AgentStep::Message { content } => assert_eq!(content, "Hello"),
        other => panic!("expected message, got {other:?}"),
    }
    let mut deltas = Vec::new();
    while let Some(event) = rx.recv().await {
        if let AgentEvent::AssistantDelta { text } = event {
            deltas.push(text);
        }
    }
    assert_eq!(deltas, vec!["Hel", "lo"]);
}

#[tokio::test]
async fn google_genai_provider_separates_thoughts_from_answer() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.5-pro:streamGenerateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            concat!(
                "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"I should inspect tools.\",\"thought\":true}]}}]}\n\n",
                "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"I am appctl, your application operations agent.\"}]}}]}\n\n",
            ),
        ))
        .mount(&server)
        .await;

    let provider = GoogleGenaiProvider::new(test_provider(server.uri()));
    let messages = vec![Message {
        role: "user".to_string(),
        content: "who are you?".to_string(),
        tool_calls: Vec::new(),
        tool_call_id: None,
        tool_name: None,
    }];
    let (tx, mut rx) = mpsc::channel(8);

    let step = provider
        .chat(&messages, &[], Some(tx))
        .await
        .expect("gemini thought stream");

    match step {
        appctl::ai::AgentStep::Message { content } => {
            assert_eq!(content, "I am appctl, your application operations agent.")
        }
        other => panic!("expected message, got {other:?}"),
    }

    let mut thoughts = Vec::new();
    let mut deltas = Vec::new();
    while let Some(event) = rx.recv().await {
        match event {
            AgentEvent::AssistantThoughtDelta { text } => thoughts.push(text),
            AgentEvent::AssistantDelta { text } => deltas.push(text),
            _ => {}
        }
    }
    assert_eq!(thoughts, vec!["I should inspect tools."]);
    assert_eq!(
        deltas,
        vec!["I am appctl, your application operations agent."]
    );
}

fn test_provider(base_url: String) -> ResolvedProvider {
    ResolvedProvider {
        name: "gemini".to_string(),
        kind: ProviderKind::GoogleGenai,
        base_url,
        model: "gemini-2.5-pro".to_string(),
        auth: ResolvedProviderAuth::None {
            status: auth_status(ProviderAuthKind::None, false),
        },
        auth_status: auth_status(ProviderAuthKind::None, false),
        extra_headers: BTreeMap::new(),
    }
}

fn auth_status(kind: ProviderAuthKind, configured: bool) -> ProviderAuthStatus {
    ProviderAuthStatus {
        kind,
        origin: ProviderAuthOrigin::Explicit,
        configured,
        secret_ref: None,
        profile: None,
        expires_at: None,
        scopes: Vec::new(),
        project_id: None,
        recovery_hint: None,
        help_url: None,
        bridge_client: None,
    }
}
