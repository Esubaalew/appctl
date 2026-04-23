//! Shared helpers for the `live_*` tests.
//!
//! These tests are gated behind the `live-auth` feature and only run when the
//! required env vars are present. The goal is to let a developer re-verify a
//! single provider locally without running the full `scripts/verify-all.sh`
//! matrix.

#![cfg(feature = "live-auth")]
#![allow(dead_code, unused_imports, clippy::too_many_arguments)]

use std::collections::BTreeMap;

use appctl::ai::{LlmProvider, provider_from_config};
use appctl::auth::provider::{
    ProviderAuthKind, ProviderAuthOrigin, ProviderAuthStatus, ResolvedProviderAuth,
};
use appctl::config::{ProviderKind, ResolvedProvider};

#[macro_export]
macro_rules! require_env {
    ($($var:literal),+ $(,)?) => {{
        let mut missing: Vec<&str> = Vec::new();
        $( if std::env::var($var).is_err() { missing.push($var); } )+
        if !missing.is_empty() {
            eprintln!("live test skipped; missing env: {}", missing.join(", "));
            return;
        }
    }};
}

pub fn api_key_status(kind: ProviderAuthKind, secret_ref: &str) -> ProviderAuthStatus {
    ProviderAuthStatus {
        kind,
        origin: ProviderAuthOrigin::Explicit,
        configured: true,
        secret_ref: Some(secret_ref.to_string()),
        profile: None,
        expires_at: None,
        scopes: Vec::new(),
        project_id: None,
        recovery_hint: None,
        help_url: None,
        bridge_client: None,
    }
}

pub fn resolved_api_key(
    name: &str,
    kind: ProviderKind,
    auth_kind: ProviderAuthKind,
    base_url: String,
    model: String,
    secret_ref: &str,
    value: String,
    extra_headers: BTreeMap<String, String>,
) -> ResolvedProvider {
    ResolvedProvider {
        name: name.to_string(),
        kind,
        base_url,
        model,
        auth: ResolvedProviderAuth::ApiKey {
            value,
            status: api_key_status(auth_kind.clone(), secret_ref),
        },
        auth_status: api_key_status(auth_kind, secret_ref),
        extra_headers,
    }
}

pub async fn send_smoke_prompt(resolved: ResolvedProvider) -> anyhow::Result<String> {
    let provider = provider_from_config(resolved);
    let messages = vec![appctl::ai::Message {
        role: "user".to_string(),
        content: "Reply with the single token 'ok' and nothing else.".to_string(),
        tool_calls: Vec::new(),
        tool_call_id: None,
        tool_name: None,
    }];
    let step = provider.chat(&messages, &[]).await?;
    Ok(format!("{step:?}"))
}
