//! Opt-in smoke test against NVIDIA NIM OpenAI-compatible API (requires `NVIDIA_API_KEY`).
use serde_json::json;

#[tokio::test]
#[ignore = "run with: NVIDIA_API_KEY=... cargo test nim_openai_compatible_smoke -- --ignored"]
async fn nim_openai_compatible_smoke() {
    let key = std::env::var("NVIDIA_API_KEY").expect("NVIDIA_API_KEY");
    let client = reqwest::Client::new();
    let url = std::env::var("APPCTL_NIM_BASE_URL")
        .unwrap_or_else(|_| "https://integrate.api.nvidia.com/v1/chat/completions".to_string());
    let model = std::env::var("APPCTL_NIM_MODEL")
        .unwrap_or_else(|_| "meta/llama-3.1-8b-instruct".to_string());

    let res = client
        .post(&url)
        .header("Authorization", format!("Bearer {key}"))
        .json(&json!({
            "model": model,
            "messages": [{"role": "user", "content": "Reply with exactly: pong"}],
            "max_tokens": 8
        }))
        .send()
        .await
        .expect("request");

    assert!(res.status().is_success(), "NIM returned {}", res.status());
}
