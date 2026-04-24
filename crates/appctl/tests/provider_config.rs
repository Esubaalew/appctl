use appctl::{
    auth::provider::ProviderAuthConfig,
    cloud::{CloudProviderConnections, SyncedProviderConnection, save_synced_connections},
    config::{
        AppConfig, BehaviorConfig, CloudConfig, ConfigPaths, ProviderConfig, ProviderKind,
        TargetConfig,
    },
};
use tempfile::tempdir;

#[test]
fn resolve_provider_requires_configured_secret() {
    let config = AppConfig {
        default: "vertex".to_string(),
        providers: vec![ProviderConfig {
            name: "vertex".to_string(),
            kind: ProviderKind::GoogleGenai,
            base_url: "https://example.com".to_string(),
            model: "gemini-2.5-flash".to_string(),
            verified: true,
            auth: Some(ProviderAuthConfig::ApiKey {
                secret_ref: "APPCTL_TEST_MISSING_SECRET".to_string(),
                help_url: None,
            }),
            api_key_ref: Some("APPCTL_TEST_MISSING_SECRET".to_string()),
            extra_headers: Default::default(),
        }],
        target: TargetConfig::default(),
        cloud: Default::default(),
        behavior: BehaviorConfig::default(),
        tooling: Default::default(),
        display_name: None,
        description: None,
    };

    let error = config.resolve_provider(None, None).unwrap_err().to_string();
    assert!(error.contains("APPCTL_TEST_MISSING_SECRET"));
    assert!(error.contains("set-secret"));
}

#[test]
fn parse_provider_auth_block_from_toml() {
    let config: AppConfig = toml::from_str(
        r#"
default = "gemini"

[[provider]]
name = "gemini"
kind = "google_genai"
base_url = "https://generativelanguage.googleapis.com"
model = "gemini-2.5-pro"
auth = { kind = "oauth2", profile = "gemini-default", scopes = ["scope-a"] }
"#,
    )
    .expect("config should parse");

    assert_eq!(config.providers.len(), 1);
    let provider = &config.providers[0];
    assert!(matches!(provider.kind, ProviderKind::GoogleGenai));
    assert!(matches!(
        provider.auth.as_ref(),
        Some(ProviderAuthConfig::OAuth2 { profile, .. }) if profile == "gemini-default"
    ));
}

#[test]
fn resolve_provider_uses_cloud_connection_when_enabled() {
    let dir = tempdir().unwrap();
    let paths = ConfigPaths::new(dir.path().join(".appctl"));
    let config = AppConfig {
        default: "gemini".to_string(),
        providers: vec![ProviderConfig {
            name: "gemini".to_string(),
            kind: ProviderKind::GoogleGenai,
            base_url: "https://generativelanguage.googleapis.com".to_string(),
            model: "gemini-2.5-pro".to_string(),
            verified: true,
            auth: None,
            api_key_ref: None,
            extra_headers: Default::default(),
        }],
        target: TargetConfig::default(),
        cloud: CloudConfig {
            enabled: true,
            ..Default::default()
        },
        behavior: BehaviorConfig::default(),
        tooling: Default::default(),
        display_name: None,
        description: None,
    };
    config.save(&paths).unwrap();

    save_synced_connections(
        &paths,
        &CloudProviderConnections {
            connections: vec![SyncedProviderConnection {
                provider: "gemini".to_string(),
                auth: ProviderAuthConfig::ApiKey {
                    secret_ref: "APPCTL_TEST_MISSING_SECRET".to_string(),
                    help_url: None,
                },
                synced_at: None,
            }],
        },
    )
    .unwrap();

    let statuses = config.provider_statuses_with_paths(&paths);
    assert!(matches!(
        statuses[0].auth_status.origin,
        appctl::auth::provider::ProviderAuthOrigin::Cloud
    ));
}
