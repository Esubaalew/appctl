#![cfg(feature = "live-auth")]

mod live_common;

use std::collections::BTreeMap;

use appctl::auth::provider::ProviderAuthKind;
use appctl::config::ProviderKind;

use crate::live_common::{resolved_api_key, send_smoke_prompt};

#[tokio::test]
async fn anthropic_smoke() {
    require_env!("ANTHROPIC_API_KEY");
    let key = std::env::var("ANTHROPIC_API_KEY").unwrap();
    let resolved = resolved_api_key(
        "claude",
        ProviderKind::Anthropic,
        ProviderAuthKind::ApiKey,
        "https://api.anthropic.com".to_string(),
        std::env::var("APPCTL_LIVE_ANTHROPIC_MODEL")
            .unwrap_or_else(|_| "claude-sonnet-4".to_string()),
        "ANTHROPIC_API_KEY",
        key,
        BTreeMap::new(),
    );
    let step = send_smoke_prompt(resolved)
        .await
        .expect("anthropic live call");
    assert!(step.to_lowercase().contains("ok"));
}
