#![cfg(feature = "live-auth")]

mod live_common;

use std::collections::BTreeMap;

use appctl::auth::provider::ProviderAuthKind;
use appctl::config::ProviderKind;

use crate::live_common::{resolved_api_key, send_smoke_prompt};

#[tokio::test]
async fn qwen_dashscope_smoke() {
    require_env!("DASHSCOPE_API_KEY");
    let key = std::env::var("DASHSCOPE_API_KEY").unwrap();
    let resolved = resolved_api_key(
        "qwen",
        ProviderKind::OpenAiCompatible,
        ProviderAuthKind::ApiKey,
        "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
        std::env::var("APPCTL_LIVE_QWEN_MODEL").unwrap_or_else(|_| "qwen3-coder-plus".to_string()),
        "DASHSCOPE_API_KEY",
        key,
        BTreeMap::new(),
    );
    let step = send_smoke_prompt(resolved).await.expect("qwen live call");
    assert!(step.to_lowercase().contains("ok"));
}
