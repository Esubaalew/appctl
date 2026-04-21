use appctl::config::{AppConfig, BehaviorConfig, ProviderConfig, ProviderKind, TargetConfig};

#[test]
fn resolve_provider_requires_configured_secret() {
    let config = AppConfig {
        default: "vertex".to_string(),
        providers: vec![ProviderConfig {
            name: "vertex".to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: "https://example.com".to_string(),
            model: "google/gemini-2.5-flash".to_string(),
            api_key_ref: Some("APPCTL_TEST_MISSING_SECRET".to_string()),
            extra_headers: Default::default(),
        }],
        target: TargetConfig::default(),
        behavior: BehaviorConfig::default(),
    };

    let error = config.resolve_provider(None, None).unwrap_err().to_string();
    assert!(error.contains("APPCTL_TEST_MISSING_SECRET"));
    assert!(error.contains("set-secret"));
}
