#![cfg(feature = "live-auth")]

mod live_common;

use std::collections::BTreeMap;

use appctl::auth::gcloud;
use appctl::auth::provider::{
    ProviderAuthKind, ProviderAuthOrigin, ProviderAuthStatus, ResolvedProviderAuth,
};
use appctl::config::{ProviderKind, ResolvedProvider};

use crate::live_common::send_smoke_prompt;

#[tokio::test]
async fn vertex_adc_smoke() {
    require_env!("GOOGLE_APPLICATION_CREDENTIALS", "VERTEX_PROJECT");

    let token = match gcloud::adc_access_token(std::env::var("VERTEX_PROJECT").ok().as_deref()) {
        Ok(token) => token,
        Err(err) => {
            eprintln!("vertex live test skipped; gcloud unavailable: {err}");
            return;
        }
    };

    let region = std::env::var("VERTEX_REGION").unwrap_or_else(|_| "us-central1".to_string());
    let base_url = format!("https://{region}-aiplatform.googleapis.com");
    let model =
        std::env::var("APPCTL_LIVE_VERTEX_MODEL").unwrap_or_else(|_| "gemini-2.5-pro".to_string());

    let status = ProviderAuthStatus {
        kind: ProviderAuthKind::GoogleAdc,
        origin: ProviderAuthOrigin::Explicit,
        configured: true,
        secret_ref: None,
        profile: None,
        expires_at: token.expires_at,
        scopes: Vec::new(),
        project_id: token.project_id.clone(),
        recovery_hint: None,
        help_url: None,
        bridge_client: None,
    };

    let mut extra_headers = BTreeMap::new();
    extra_headers.insert("x-appctl-vertex-region".to_string(), region);

    let resolved = ResolvedProvider {
        name: "vertex".to_string(),
        kind: ProviderKind::Vertex,
        base_url,
        model,
        auth: ResolvedProviderAuth::GoogleAdc {
            access_token: token.access_token,
            status: status.clone(),
        },
        auth_status: status,
        extra_headers,
    };

    let step = send_smoke_prompt(resolved).await.expect("vertex live call");
    assert!(step.to_lowercase().contains("ok"));
}
