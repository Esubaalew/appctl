#![cfg(feature = "live-auth")]

mod live_common;

use std::collections::BTreeMap;

use appctl::auth::provider::ProviderAuthKind;
use appctl::config::ProviderKind;

use crate::live_common::{resolved_api_key, send_smoke_prompt};

#[tokio::test]
async fn openai_smoke() {
    require_env!("OPENAI_API_KEY");
    let key = std::env::var("OPENAI_API_KEY").unwrap();
    let resolved = resolved_api_key(
        "openai",
        ProviderKind::OpenAiCompatible,
        ProviderAuthKind::ApiKey,
        "https://api.openai.com/v1".to_string(),
        std::env::var("APPCTL_LIVE_OPENAI_MODEL").unwrap_or_else(|_| "gpt-4.1-mini".to_string()),
        "OPENAI_API_KEY",
        key,
        BTreeMap::new(),
    );
    let step = send_smoke_prompt(resolved).await.expect("openai live call");
    assert!(step.to_lowercase().contains("ok"));
}
