use std::collections::BTreeMap;

use appctl::{
    ai::{AgentStep, LlmProvider, Message, openai_compat::OpenAiCompatProvider},
    auth::provider::{
        ProviderAuthKind, ProviderAuthOrigin, ProviderAuthStatus, ResolvedProviderAuth,
    },
    config::{ProviderKind, ResolvedProvider},
    events::AgentEvent,
};
use tokio::sync::mpsc;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, method, path},
};

#[tokio::test]
async fn openai_compatible_provider_emits_text_deltas() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_string_contains("\"stream\":true"))
        .respond_with(ResponseTemplate::new(200).set_body_string(concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n",
            "data: [DONE]\n\n",
        )))
        .mount(&server)
        .await;

    let provider = OpenAiCompatProvider::new(test_provider(server.uri()));
    let messages = vec![user_message("Say hello")];
    let (tx, mut rx) = mpsc::channel(8);

    let step = provider
        .chat(&messages, &[], Some(tx))
        .await
        .expect("openai stream");

    match step {
        AgentStep::Message { content } => assert_eq!(content, "Hello"),
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
async fn openai_compatible_provider_accumulates_streamed_tool_arguments() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(concat!(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call-1\",\"type\":\"function\",\"function\":{\"name\":\"create_widget\",\"arguments\":\"{\\\"na\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"me\\\":\\\"Demo\\\"}\"}}]}}]}\n\n",
            "data: [DONE]\n\n",
        )))
        .mount(&server)
        .await;

    let provider = OpenAiCompatProvider::new(test_provider(server.uri()));
    let messages = vec![user_message("Create a widget")];

    let step = provider
        .chat(&messages, &[], None)
        .await
        .expect("openai tool stream");

    match step {
        AgentStep::ToolCalls { calls } => {
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].id, "call-1");
            assert_eq!(calls[0].name, "create_widget");
            assert_eq!(calls[0].arguments["name"], "Demo");
        }
        other => panic!("expected tool calls, got {other:?}"),
    }
}

fn user_message(content: &str) -> Message {
    Message {
        role: "user".to_string(),
        content: content.to_string(),
        tool_calls: Vec::new(),
        tool_call_id: None,
        tool_name: None,
    }
}

fn test_provider(base_url: String) -> ResolvedProvider {
    ResolvedProvider {
        name: "openai-compatible".to_string(),
        kind: ProviderKind::OpenAiCompatible,
        base_url,
        model: "test-model".to_string(),
        auth: ResolvedProviderAuth::None {
            status: auth_status(),
        },
        auth_status: auth_status(),
        extra_headers: BTreeMap::new(),
    }
}

fn auth_status() -> ProviderAuthStatus {
    ProviderAuthStatus {
        kind: ProviderAuthKind::None,
        origin: ProviderAuthOrigin::Explicit,
        configured: false,
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
