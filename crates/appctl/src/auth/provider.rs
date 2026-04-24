use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    auth::{gcloud, oauth},
    config::{ProviderConfig, load_secret},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderAuthConfig {
    None,
    ApiKey {
        secret_ref: String,
        #[serde(default)]
        help_url: Option<String>,
    },
    GoogleAdc {
        #[serde(default)]
        project: Option<String>,
    },
    QwenOAuth {
        profile: String,
    },
    AzureAd {
        tenant: String,
        client_id: String,
        #[serde(default)]
        device_code: bool,
    },
    McpBridge {
        client: McpBridgeClient,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAuthKind {
    None,
    ApiKey,
    GoogleAdc,
    QwenOAuth,
    AzureAd,
    McpBridge,
    OAuth2,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpBridgeClient {
    Codex,
    Claude,
    QwenCode,
    Gemini,
}

impl McpBridgeClient {
    pub fn binary_name(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::QwenCode => "qwen",
            Self::Gemini => "gemini",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude Code",
            Self::QwenCode => "Qwen Code",
            Self::Gemini => "Gemini CLI",
        }
    }

    /// Shown when `Command::new(binary)` fails with `NotFound` during MCP bridge setup.
    pub fn mcp_bridge_not_found_hint(self) -> &'static str {
        match self {
            Self::Codex => {
                "Install OpenAI’s Codex CLI so `codex` is on your PATH, then try again — or pick another provider in `appctl init`."
            }
            Self::Claude => {
                "Install Anthropic’s Claude Code CLI so `claude` is on your PATH, then try again — or pick another provider in `appctl init`."
            }
            Self::QwenCode => {
                "Install Alibaba’s Qwen Code CLI so `qwen` is on your PATH, then try again — or pick another provider in `appctl init`."
            }
            Self::Gemini => {
                "Install Google’s Gemini CLI so the `gemini` command is on your PATH (open a new terminal after installing); see Google’s current Gemini / AI Studio docs for the official install steps."
            }
        }
    }
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
    pub help_url: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub bridge_client: Option<McpBridgeClient>,
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
    GoogleAdc {
        access_token: String,
        status: ProviderAuthStatus,
    },
    QwenOAuth {
        access_token: String,
        status: ProviderAuthStatus,
    },
    AzureAd {
        access_token: String,
        status: ProviderAuthStatus,
    },
    McpBridge {
        status: ProviderAuthStatus,
    },
    OAuth2 {
        access_token: String,
        status: ProviderAuthStatus,
    },
}

impl ResolvedProviderAuth {
    pub fn status(&self) -> &ProviderAuthStatus {
        match self {
            Self::None { status }
            | Self::ApiKey { status, .. }
            | Self::GoogleAdc { status, .. }
            | Self::QwenOAuth { status, .. }
            | Self::AzureAd { status, .. }
            | Self::McpBridge { status }
            | Self::OAuth2 { status, .. } => status,
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
            Self::GoogleAdc { access_token, .. }
            | Self::QwenOAuth { access_token, .. }
            | Self::AzureAd { access_token, .. }
            | Self::OAuth2 { access_token, .. } => Some(access_token.as_str()),
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
        Some(auth) => inspect_auth_spec(provider_name, auth, ProviderAuthOrigin::Explicit),
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
                    help_url: None,
                    profile: None,
                    expires_at: None,
                    scopes: Vec::new(),
                    project_id: None,
                    bridge_client: None,
                    recovery_hint: (!configured).then(|| api_key_recovery_hint(secret_ref, None)),
                }
            }
            None => ProviderAuthStatus {
                kind: ProviderAuthKind::None,
                origin: ProviderAuthOrigin::LegacyApiKeyRef,
                configured: true,
                secret_ref: None,
                help_url: None,
                profile: None,
                expires_at: None,
                scopes: Vec::new(),
                project_id: None,
                bridge_client: None,
                recovery_hint: Some(format!(
                    "Provider '{}' has no auth configured. Run `appctl init` to set up a real provider path.",
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
        Some(ProviderAuthConfig::ApiKey { secret_ref, .. }) => {
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
        Some(ProviderAuthConfig::GoogleAdc { project }) => {
            let token = gcloud::adc_access_token(project.as_deref()).with_context(|| {
                format!(
                    "google ADC is not configured for '{}'; run `appctl auth provider login {}`",
                    provider_name, provider_name
                )
            })?;
            Ok(ResolvedProviderAuth::GoogleAdc {
                access_token: token.access_token,
                status: inspect_provider_auth(provider_name, provider, cloud_auth),
            })
        }
        Some(ProviderAuthConfig::QwenOAuth { profile }) => {
            let tokens = oauth::load_provider_tokens(profile).with_context(|| {
                format!(
                    "provider '{}' requires Qwen OAuth tokens for profile '{}'; run `appctl auth provider login {} --oauth`",
                    provider_name, profile, provider_name
                )
            })?;
            Ok(ResolvedProviderAuth::QwenOAuth {
                access_token: tokens.access_token,
                status: inspect_provider_auth(provider_name, provider, cloud_auth),
            })
        }
        Some(ProviderAuthConfig::AzureAd {
            tenant, client_id, ..
        }) => {
            let storage_key = azure_storage_key(tenant, client_id);
            let tokens = oauth::load_provider_tokens(&storage_key).with_context(|| {
                format!(
                    "provider '{}' requires Azure AD tokens; run `appctl auth provider login {}`",
                    provider_name, provider_name
                )
            })?;
            Ok(ResolvedProviderAuth::AzureAd {
                access_token: tokens.access_token,
                status: inspect_provider_auth(provider_name, provider, cloud_auth),
            })
        }
        Some(ProviderAuthConfig::McpBridge { client }) => Ok(ResolvedProviderAuth::McpBridge {
            status: ProviderAuthStatus {
                recovery_hint: Some(format!(
                    "Open {} and use the generated `appctl` MCP entry instead of direct `appctl run`.",
                    client.display_name()
                )),
                ..inspect_provider_auth(provider_name, provider, cloud_auth)
            },
        }),
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
    std::env::var(name)
        .ok()
        .or_else(|| load_secret(name).ok())
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
            help_url: None,
            profile: None,
            expires_at: None,
            scopes: Vec::new(),
            project_id: None,
            bridge_client: None,
            recovery_hint: None,
        },
        ProviderAuthConfig::ApiKey {
            secret_ref,
            help_url,
        } => {
            let configured = load_secret_value(secret_ref).is_some();
            ProviderAuthStatus {
                kind: ProviderAuthKind::ApiKey,
                origin,
                configured,
                secret_ref: Some(secret_ref.clone()),
                help_url: help_url.clone(),
                profile: None,
                expires_at: None,
                scopes: Vec::new(),
                project_id: None,
                bridge_client: None,
                recovery_hint: (!configured)
                    .then(|| api_key_recovery_hint(secret_ref, help_url.as_deref())),
            }
        }
        ProviderAuthConfig::GoogleAdc { project } => {
            match gcloud::adc_access_token(project.as_deref()) {
                Ok(token) => ProviderAuthStatus {
                    kind: ProviderAuthKind::GoogleAdc,
                    origin,
                    configured: true,
                    secret_ref: None,
                    help_url: None,
                    profile: None,
                    expires_at: token.expires_at,
                    scopes: Vec::new(),
                    project_id: token.project_id.or_else(|| project.clone()),
                    bridge_client: None,
                    recovery_hint: None,
                },
                Err(_) => ProviderAuthStatus {
                    kind: ProviderAuthKind::GoogleAdc,
                    origin,
                    configured: false,
                    secret_ref: None,
                    help_url: None,
                    profile: None,
                    expires_at: None,
                    scopes: Vec::new(),
                    project_id: project.clone(),
                    bridge_client: None,
                    recovery_hint: Some(format!(
                        "Run `gcloud auth application-default login`. If gcloud is missing, {}.",
                        gcloud::install_hint()
                    )),
                },
            }
        }
        ProviderAuthConfig::QwenOAuth { profile } => {
            let tokens = oauth::load_provider_tokens(profile);
            ProviderAuthStatus {
                kind: ProviderAuthKind::QwenOAuth,
                origin,
                configured: tokens.is_some(),
                secret_ref: None,
                help_url: None,
                profile: Some(profile.clone()),
                expires_at: tokens.as_ref().and_then(|t| t.expires_at),
                scopes: tokens.map(|t| t.scopes).unwrap_or_default(),
                project_id: None,
                bridge_client: None,
                recovery_hint: Some(
                    "Qwen OAuth is not wired into `appctl auth provider login` yet. Use a DashScope API key or the Qwen Code MCP bridge from `appctl init`."
                        .to_string(),
                ),
            }
        }
        ProviderAuthConfig::AzureAd {
            tenant, client_id, ..
        } => {
            let storage_key = azure_storage_key(tenant, client_id);
            let tokens = oauth::load_provider_tokens(&storage_key);
            ProviderAuthStatus {
                kind: ProviderAuthKind::AzureAd,
                origin,
                configured: tokens.is_some(),
                secret_ref: None,
                help_url: None,
                profile: Some(storage_key),
                expires_at: tokens.as_ref().and_then(|t| t.expires_at),
                scopes: tokens.map(|t| t.scopes).unwrap_or_default(),
                project_id: None,
                bridge_client: None,
                recovery_hint: Some(
                    "Run `appctl auth provider login <provider>` to start the Azure AD device-code flow."
                        .to_string(),
                ),
            }
        }
        ProviderAuthConfig::McpBridge { client } => ProviderAuthStatus {
            kind: ProviderAuthKind::McpBridge,
            origin,
            configured: true,
            secret_ref: None,
            help_url: None,
            profile: None,
            expires_at: None,
            scopes: Vec::new(),
            project_id: None,
            bridge_client: Some(*client),
            recovery_hint: Some(format!(
                "Open {} and use the generated `appctl` MCP entry.",
                client.display_name()
            )),
        },
        ProviderAuthConfig::OAuth2 {
            profile, scopes, ..
        } => {
            let tokens = oauth::load_provider_tokens(profile);
            ProviderAuthStatus {
                kind: ProviderAuthKind::OAuth2,
                origin,
                configured: tokens.is_some(),
                secret_ref: None,
                help_url: None,
                profile: Some(profile.clone()),
                expires_at: tokens.as_ref().and_then(|t| t.expires_at),
                scopes: if tokens.is_some() {
                    tokens.map(|t| t.scopes).unwrap_or_default()
                } else {
                    scopes.clone()
                },
                project_id: None,
                bridge_client: None,
                recovery_hint: None,
            }
        }
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
        ProviderAuthConfig::ApiKey { secret_ref, .. } => {
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
        ProviderAuthConfig::GoogleAdc { project } => {
            let token = gcloud::adc_access_token(project.as_deref())
                .context("google ADC not available; run `gcloud auth application-default login`")?;
            Ok(ResolvedProviderAuth::GoogleAdc {
                access_token: token.access_token,
                status: inspect_auth_spec(provider_name, auth, ProviderAuthOrigin::Cloud),
            })
        }
        ProviderAuthConfig::QwenOAuth { profile } => {
            let tokens = oauth::load_provider_tokens(profile).with_context(|| {
                format!(
                    "cloud-synced provider '{}' requires Qwen OAuth tokens for profile '{}'",
                    provider_name, profile
                )
            })?;
            Ok(ResolvedProviderAuth::QwenOAuth {
                access_token: tokens.access_token,
                status: inspect_auth_spec(provider_name, auth, ProviderAuthOrigin::Cloud),
            })
        }
        ProviderAuthConfig::AzureAd {
            tenant, client_id, ..
        } => {
            let storage_key = azure_storage_key(tenant, client_id);
            let tokens = oauth::load_provider_tokens(&storage_key).with_context(|| {
                format!(
                    "cloud-synced provider '{}' requires Azure AD tokens",
                    provider_name
                )
            })?;
            Ok(ResolvedProviderAuth::AzureAd {
                access_token: tokens.access_token,
                status: inspect_auth_spec(provider_name, auth, ProviderAuthOrigin::Cloud),
            })
        }
        ProviderAuthConfig::McpBridge { .. } => Ok(ResolvedProviderAuth::McpBridge {
            status: inspect_auth_spec(provider_name, auth, ProviderAuthOrigin::Cloud),
        }),
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
    }
}

fn api_key_recovery_hint(secret_ref: &str, help_url: Option<&str>) -> String {
    match help_url {
        Some(help_url) => format!(
            "Set `{}` in the environment or run `appctl config set-secret {} --value ...`. Get a real API key at {}.",
            secret_ref, secret_ref, help_url
        ),
        None => format!(
            "Set `{}` in the environment or run `appctl config set-secret {} --value ...`.",
            secret_ref, secret_ref
        ),
    }
}

fn azure_storage_key(tenant: &str, client_id: &str) -> String {
    format!("azure-ad::{tenant}::{client_id}")
}
