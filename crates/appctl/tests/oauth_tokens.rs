//! Round-trip stored tokens through the keychain helpers.
//!
//! Uses a random provider name so the test is isolated from any real
//! credentials that might exist on the developer's machine. Falls back to
//! a no-op if the current environment has no keychain (e.g. headless CI
//! without `keyring`'s `secret-service`).

use appctl::auth::oauth::{StoredTokens, load_tokens, save_tokens};
use uuid::Uuid;

#[test]
fn stored_tokens_round_trip() {
    let provider = format!("appctl-test-{}", Uuid::new_v4());
    let tokens = StoredTokens {
        access_token: "at-123".to_string(),
        refresh_token: Some("rt-456".to_string()),
        expires_at: Some(chrono::Utc::now().timestamp() + 3600),
        token_type: Some("bearer".to_string()),
        scopes: vec!["read".to_string(), "write".to_string()],
    };
    let Ok(()) = save_tokens(&provider, &tokens) else {
        eprintln!("keychain unavailable; skipping");
        return;
    };
    let Some(loaded) = load_tokens(&provider) else {
        panic!("save succeeded but load returned None");
    };
    assert_eq!(loaded.access_token, "at-123");
    assert_eq!(loaded.refresh_token.as_deref(), Some("rt-456"));
    assert_eq!(loaded.scopes.len(), 2);
}
