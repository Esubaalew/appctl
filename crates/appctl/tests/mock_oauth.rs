//! Deterministic OAuth tests using wiremock.
//!
//! These tests do not hit the real internet. They mount a wiremock instance
//! at an ephemeral port and point `auth::azure_ad` at it via the
//! `authority_base` override.

use appctl::auth::azure_ad::{AzureAdDeviceConfig, DEFAULT_SCOPE, device_code_login};
use serde_json::json;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn azure_cfg(server: &MockServer, storage_key: &str) -> AzureAdDeviceConfig {
    AzureAdDeviceConfig {
        tenant: "contoso".to_string(),
        client_id: "fake-client".to_string(),
        scope: DEFAULT_SCOPE.to_string(),
        storage_key: storage_key.to_string(),
        authority_base: Some(server.uri()),
        suppress_browser: true,
    }
}

#[tokio::test]
async fn azure_device_flow_succeeds_after_authorization_pending() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/contoso/oauth2/v2.0/devicecode"))
        .and(body_string_contains("client_id=fake-client"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user_code": "ABCD1234",
            "device_code": "dev-code-123",
            "verification_uri": format!("{}/activate", server.uri()),
            "expires_in": 900,
            "interval": 1,
            "message": "Enter the code to sign in"
        })))
        .expect(1)
        .mount(&server)
        .await;

    // First token call: still signing in
    Mock::given(method("POST"))
        .and(path("/contoso/oauth2/v2.0/token"))
        .and(body_string_contains(
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code",
        ))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "authorization_pending"
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    // Second token call: success
    Mock::given(method("POST"))
        .and(path("/contoso/oauth2/v2.0/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "access-token-final",
            "refresh_token": "refresh-token-final",
            "expires_in": 3600,
            "token_type": "Bearer",
            "scope": DEFAULT_SCOPE
        })))
        .mount(&server)
        .await;

    let tokens = device_code_login(azure_cfg(&server, "mock-azure-device"))
        .await
        .expect("device-code flow should succeed");

    assert_eq!(tokens.access_token, "access-token-final");
    assert_eq!(tokens.refresh_token.as_deref(), Some("refresh-token-final"));
    assert!(tokens.expires_at.is_some());
    assert!(tokens.scopes.iter().any(|scope| scope == DEFAULT_SCOPE));
}

#[tokio::test]
async fn azure_device_flow_surfaces_expired_token() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/contoso/oauth2/v2.0/devicecode"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user_code": "ZZZ",
            "device_code": "dev",
            "verification_uri": format!("{}/activate", server.uri()),
            "expires_in": 1,
            "interval": 1,
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/contoso/oauth2/v2.0/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "expired_token",
            "error_description": "Device code expired"
        })))
        .mount(&server)
        .await;

    let err = device_code_login(azure_cfg(&server, "mock-azure-expired"))
        .await
        .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.to_lowercase().contains("expired"),
        "expected expired error, got: {msg}"
    );
}
