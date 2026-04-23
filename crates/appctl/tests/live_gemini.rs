#![cfg(feature = "live-auth")]

mod live_common;

use std::collections::BTreeMap;

use appctl::auth::provider::ProviderAuthKind;
use appctl::config::ProviderKind;

use crate::live_common::{resolved_api_key, send_smoke_prompt};

#[tokio::test]
async fn gemini_api_key_smoke() {
    require_env!("GOOGLE_API_KEY");
    let key = std::env::var("GOOGLE_API_KEY").unwrap();
    let resolved = resolved_api_key(
        "gemini",
        ProviderKind::GoogleGenai,
        ProviderAuthKind::ApiKey,
        "https://generativelanguage.googleapis.com".to_string(),
        std::env::var("APPCTL_LIVE_GEMINI_MODEL").unwrap_or_else(|_| "gemini-2.5-pro".to_string()),
        "GOOGLE_API_KEY",
        key,
        BTreeMap::new(),
    );
    let step = send_smoke_prompt(resolved).await.expect("gemini live call");
    assert!(step.to_lowercase().contains("ok"));
}
