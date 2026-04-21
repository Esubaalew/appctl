use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::{
    auth::oauth,
    config::{ProviderConfig, load_secret},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderAuthConfig {
    None,
    ApiKey {
        secret_ref: String,
    },
    #[serde(rename = "oauth2")]
    OAuth2 {
        profile: String,
        #[serde(default)]
        scopes: Vec<String>,
        #[serde(default)]
        client_id_ref: Option<String>,
        #[serde(default)]
        client_secret_ref: Option<String>,
        #[serde(default)]
        auth_url: Option<String>,
        #[serde(default)]
        token_url: Option<String>,
    },
    GoogleAdc {
        #[serde(default)]
        profile: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAuthKind {
    None,
    ApiKey,
    OAuth2,
    GoogleAdc,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAuthOrigin {
    Explicit,
    Cloud,
    LegacyApiKeyRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAuthStatus {
    pub kind: ProviderAuthKind,
    pub origin: ProviderAuthOrigin,
    pub configured: bool,
    #[serde(default)]
    pub secret_ref: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub recovery_hint: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ResolvedProviderAuth {
    None {
        status: ProviderAuthStatus,
    },
    ApiKey {
        value: String,
        status: ProviderAuthStatus,
    },
    OAuth2 {
        access_token: String,
        status: ProviderAuthStatus,
    },
    GoogleAdc {
        access_token: String,
        status: ProviderAuthStatus,
    },
}

impl ResolvedProviderAuth {
    pub fn status(&self) -> &ProviderAuthStatus {
        match self {
            Self::None { status }
            | Self::ApiKey { status, .. }
            | Self::OAuth2 { status, .. }
            | Self::GoogleAdc { status, .. } => status,
        }
    }

    pub fn api_key(&self) -> Option<&str> {
        match self {
            Self::ApiKey { value, .. } => Some(value.as_str()),
            _ => None,
        }
    }

    pub fn bearer_token(&self) -> Option<&str> {
        match self {
            Self::OAuth2 { access_token, .. } | Self::GoogleAdc { access_token, .. } => {
                Some(access_token.as_str())
            }
            _ => None,
        }
    }
}

pub fn inspect_provider_auth(
    provider_name: &str,
    provider: &ProviderConfig,
    cloud_auth: Option<&ProviderAuthConfig>,
) -> ProviderAuthStatus {
    match provider.auth.as_ref() {
        Some(ProviderAuthConfig::None) => ProviderAuthStatus {
            kind: ProviderAuthKind::None,
            origin: ProviderAuthOrigin::Explicit,
            configured: true,
            secret_ref: None,
            profile: None,
            expires_at: None,
            scopes: Vec::new(),
            project_id: None,
            recovery_hint: None,
        },
        Some(ProviderAuthConfig::ApiKey { secret_ref }) => {
            let configured = load_secret_value(secret_ref).is_some();
            ProviderAuthStatus {
                kind: ProviderAuthKind::ApiKey,
                origin: ProviderAuthOrigin::Explicit,
                configured,
                secret_ref: Some(secret_ref.clone()),
                profile: None,
                expires_at: None,
                scopes: Vec::new(),
                project_id: None,
                recovery_hint: (!configured).then(|| {
                    format!(
                        "Set `{}` in the environment or run `appctl config set-secret {} --value ...`.",
                        secret_ref, secret_ref
                    )
                }),
            }
        }
        Some(ProviderAuthConfig::OAuth2 { profile, scopes, .. }) => {
            let tokens = oauth::load_provider_tokens(profile);
            ProviderAuthStatus {
                kind: ProviderAuthKind::OAuth2,
                origin: ProviderAuthOrigin::Explicit,
                configured: tokens.is_some(),
                secret_ref: None,
                profile: Some(profile.clone()),
                expires_at: tokens.as_ref().and_then(|t| t.expires_at),
                scopes: if tokens.is_some() {
                    tokens.map(|t| t.scopes).unwrap_or_default()
                } else {
                    scopes.clone()
                },
                project_id: None,
                recovery_hint: None,
            }
        }
        Some(ProviderAuthConfig::GoogleAdc { profile }) => match load_google_adc_access_token() {
            Ok((_, expires_at, project_id)) => ProviderAuthStatus {
                kind: ProviderAuthKind::GoogleAdc,
                origin: ProviderAuthOrigin::Explicit,
                configured: true,
                secret_ref: None,
                profile: profile.clone(),
                expires_at,
                scopes: Vec::new(),
                project_id,
                recovery_hint: None,
            },
            Err(_) => ProviderAuthStatus {
                kind: ProviderAuthKind::GoogleAdc,
                origin: ProviderAuthOrigin::Explicit,
                configured: false,
                secret_ref: None,
                profile: profile.clone(),
                expires_at: None,
                scopes: Vec::new(),
                project_id: None,
                recovery_hint: Some(
                    "Run `gcloud auth application-default login` or switch this provider to OAuth2."
                        .to_string(),
                ),
            },
        },
        None if cloud_auth.is_some() => inspect_auth_spec(
            provider_name,
            cloud_auth.expect("checked above"),
            ProviderAuthOrigin::Cloud,
        ),
        None => match provider.api_key_ref.as_deref() {
            Some(secret_ref) => {
                let configured = load_secret_value(secret_ref).is_some();
                ProviderAuthStatus {
                    kind: ProviderAuthKind::ApiKey,
                    origin: ProviderAuthOrigin::LegacyApiKeyRef,
                    configured,
                    secret_ref: Some(secret_ref.to_string()),
                    profile: None,
                    expires_at: None,
                    scopes: Vec::new(),
                    project_id: None,
                    recovery_hint: (!configured).then(|| {
                        format!(
                            "Set `{}` in the environment or run `appctl config set-secret {} --value ...`.",
                            secret_ref, secret_ref
                        )
                    }),
                }
            }
            None => ProviderAuthStatus {
                kind: ProviderAuthKind::None,
                origin: ProviderAuthOrigin::LegacyApiKeyRef,
                configured: true,
                secret_ref: None,
                profile: None,
                expires_at: None,
                scopes: Vec::new(),
                project_id: None,
                recovery_hint: Some(format!(
                    "Provider '{}' has no auth configured. Add an `auth` block or legacy `api_key_ref` if the backend requires credentials.",
                    provider_name
                )),
            },
        },
    }
}

pub fn resolve_provider_auth(
    provider_name: &str,
    provider: &ProviderConfig,
    cloud_auth: Option<&ProviderAuthConfig>,
) -> Result<ResolvedProviderAuth> {
    match provider.auth.as_ref() {
        Some(ProviderAuthConfig::None) => Ok(ResolvedProviderAuth::None {
            status: inspect_provider_auth(provider_name, provider, cloud_auth),
        }),
        Some(ProviderAuthConfig::ApiKey { secret_ref }) => {
            let value = load_secret_value(secret_ref).with_context(|| {
                format!(
                    "provider '{}' requires secret '{}'; set the env var or run `appctl config set-secret {} --value ...`",
                    provider_name, secret_ref, secret_ref
                )
            })?;
            Ok(ResolvedProviderAuth::ApiKey {
                value,
                status: inspect_provider_auth(provider_name, provider, cloud_auth),
            })
        }
        Some(ProviderAuthConfig::OAuth2 { profile, .. }) => {
            let tokens = oauth::load_provider_tokens(profile).with_context(|| {
                format!(
                    "provider '{}' requires OAuth tokens for profile '{}'; run `appctl auth provider login {}`",
                    provider_name, profile, provider_name
                )
            })?;
            Ok(ResolvedProviderAuth::OAuth2 {
                access_token: tokens.access_token,
                status: inspect_provider_auth(provider_name, provider, cloud_auth),
            })
        }
        Some(ProviderAuthConfig::GoogleAdc { .. }) => {
            let (access_token, expires_at, project_id) = load_google_adc_access_token()
                .context(
                    "google ADC not available; run `gcloud auth application-default login` or switch the provider to OAuth2",
                )?;
            Ok(ResolvedProviderAuth::GoogleAdc {
                access_token,
                status: ProviderAuthStatus {
                    kind: ProviderAuthKind::GoogleAdc,
                    origin: ProviderAuthOrigin::Explicit,
                    configured: true,
                    secret_ref: None,
                    profile: provider.auth.as_ref().and_then(|auth| match auth {
                        ProviderAuthConfig::GoogleAdc { profile } => profile.clone(),
                        _ => None,
                    }),
                    expires_at,
                    scopes: Vec::new(),
                    project_id,
                    recovery_hint: None,
                },
            })
        }
        None if cloud_auth.is_some() => {
            resolve_cloud_auth(provider_name, cloud_auth.expect("checked above"))
        }
        None => match provider.api_key_ref.as_deref() {
            Some(secret_ref) => {
                let value = load_secret_value(secret_ref).with_context(|| {
                    format!(
                        "provider '{}' requires secret '{}'; set the env var or run `appctl config set-secret {} --value ...`",
                        provider_name, secret_ref, secret_ref
                    )
                })?;
                Ok(ResolvedProviderAuth::ApiKey {
                    value,
                    status: inspect_provider_auth(provider_name, provider, cloud_auth),
                })
            }
            None => Ok(ResolvedProviderAuth::None {
                status: inspect_provider_auth(provider_name, provider, cloud_auth),
            }),
        },
    }
}

fn load_secret_value(name: &str) -> Option<String> {
    load_secret(name)
        .ok()
        .or_else(|| std::env::var(name).ok())
        .filter(|value| !value.is_empty())
}

fn inspect_auth_spec(
    _provider_name: &str,
    auth: &ProviderAuthConfig,
    origin: ProviderAuthOrigin,
) -> ProviderAuthStatus {
    match auth {
        ProviderAuthConfig::None => ProviderAuthStatus {
            kind: ProviderAuthKind::None,
            origin,
            configured: true,
            secret_ref: None,
            profile: None,
            expires_at: None,
            scopes: Vec::new(),
            project_id: None,
            recovery_hint: None,
        },
        ProviderAuthConfig::ApiKey { secret_ref } => {
            let configured = load_secret_value(secret_ref).is_some();
            ProviderAuthStatus {
                kind: ProviderAuthKind::ApiKey,
                origin,
                configured,
                secret_ref: Some(secret_ref.clone()),
                profile: None,
                expires_at: None,
                scopes: Vec::new(),
                project_id: None,
                recovery_hint: (!configured).then(|| {
                    format!(
                        "Set `{}` in the environment or run `appctl config set-secret {} --value ...`.",
                        secret_ref, secret_ref
                    )
                }),
            }
        }
        ProviderAuthConfig::OAuth2 { profile, scopes, .. } => {
            let tokens = oauth::load_provider_tokens(profile);
            ProviderAuthStatus {
                kind: ProviderAuthKind::OAuth2,
                origin,
                configured: tokens.is_some(),
                secret_ref: None,
                profile: Some(profile.clone()),
                expires_at: tokens.as_ref().and_then(|t| t.expires_at),
                scopes: if tokens.is_some() {
                    tokens.map(|t| t.scopes).unwrap_or_default()
                } else {
                    scopes.clone()
                },
                project_id: None,
                recovery_hint: None,
            }
        }
        ProviderAuthConfig::GoogleAdc { profile } => match load_google_adc_access_token() {
            Ok((_, expires_at, project_id)) => ProviderAuthStatus {
                kind: ProviderAuthKind::GoogleAdc,
                origin,
                configured: true,
                secret_ref: None,
                profile: profile.clone(),
                expires_at,
                scopes: Vec::new(),
                project_id,
                recovery_hint: None,
            },
            Err(_) => ProviderAuthStatus {
                kind: ProviderAuthKind::GoogleAdc,
                origin,
                configured: false,
                secret_ref: None,
                profile: profile.clone(),
                expires_at: None,
                scopes: Vec::new(),
                project_id: None,
                recovery_hint: Some(
                    "Run `gcloud auth application-default login` or switch this provider to OAuth2."
                        .to_string(),
                ),
            },
        },
    }
}

fn resolve_cloud_auth(
    provider_name: &str,
    auth: &ProviderAuthConfig,
) -> Result<ResolvedProviderAuth> {
    match auth {
        ProviderAuthConfig::None => Ok(ResolvedProviderAuth::None {
            status: inspect_auth_spec(provider_name, auth, ProviderAuthOrigin::Cloud),
        }),
        ProviderAuthConfig::ApiKey { secret_ref } => {
            let value = load_secret_value(secret_ref).with_context(|| {
                format!(
                    "cloud-synced provider '{}' requires secret '{}'; set the env var or run `appctl config set-secret {} --value ...`",
                    provider_name, secret_ref, secret_ref
                )
            })?;
            Ok(ResolvedProviderAuth::ApiKey {
                value,
                status: inspect_auth_spec(provider_name, auth, ProviderAuthOrigin::Cloud),
            })
        }
        ProviderAuthConfig::OAuth2 { profile, .. } => {
            let tokens = oauth::load_provider_tokens(profile).with_context(|| {
                format!(
                    "cloud-synced provider '{}' requires OAuth tokens for profile '{}'; run `appctl auth provider login {}`",
                    provider_name, profile, provider_name
                )
            })?;
            Ok(ResolvedProviderAuth::OAuth2 {
                access_token: tokens.access_token,
                status: inspect_auth_spec(provider_name, auth, ProviderAuthOrigin::Cloud),
            })
        }
        ProviderAuthConfig::GoogleAdc { profile } => {
            let (access_token, expires_at, project_id) = load_google_adc_access_token()
                .context(
                    "google ADC not available; run `gcloud auth application-default login` or switch the provider to OAuth2",
                )?;
            Ok(ResolvedProviderAuth::GoogleAdc {
                access_token,
                status: ProviderAuthStatus {
                    kind: ProviderAuthKind::GoogleAdc,
                    origin: ProviderAuthOrigin::Cloud,
                    configured: true,
                    secret_ref: None,
                    profile: profile.clone(),
                    expires_at,
                    scopes: Vec::new(),
                    project_id,
                    recovery_hint: None,
                },
            })
        }
    }
}

fn load_google_adc_access_token() -> Result<(String, Option<i64>, Option<String>)> {
    let access = Command::new("gcloud")
        .args(["auth", "application-default", "print-access-token"])
        .output()
        .context("failed to spawn gcloud for ADC access token")?;
    if !access.status.success() {
        bail!(
            "gcloud auth application-default print-access-token failed: {}",
            String::from_utf8_lossy(&access.stderr).trim()
        );
    }

    let token = String::from_utf8_lossy(&access.stdout).trim().to_string();
    if token.is_empty() {
        bail!("gcloud returned an empty ADC access token");
    }

    let project = Command::new("gcloud")
        .args(["config", "get-value", "project"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
                (!value.is_empty() && value != "(unset)").then_some(value)
            } else {
                None
            }
        });

    Ok((token, None, project))
}
