use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use keyring::Entry;
use serde::{Deserialize, Serialize};

use crate::auth::provider::{
    ProviderAuthConfig, ProviderAuthStatus, ResolvedProviderAuth, inspect_provider_auth,
    resolve_provider_auth,
};
use crate::cloud::load_synced_provider_connection;

#[derive(Debug, Clone)]
pub struct ConfigPaths {
    pub root: PathBuf,
    pub config: PathBuf,
    pub schema: PathBuf,
    pub tools: PathBuf,
    pub history: PathBuf,
    pub provider_connections: PathBuf,
}

impl ConfigPaths {
    pub fn new(root: PathBuf) -> Self {
        Self {
            config: root.join("config.toml"),
            schema: root.join("schema.json"),
            tools: root.join("tools.json"),
            history: root.join("history.db"),
            provider_connections: root.join("provider-connections.json"),
            root,
        }
    }

    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.root)
            .with_context(|| format!("failed to create {}", self.root.display()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub default: String,
    #[serde(default, rename = "provider")]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub target: TargetConfig,
    #[serde(default)]
    pub cloud: CloudConfig,
    #[serde(default)]
    pub behavior: BehaviorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub auth: Option<ProviderAuthConfig>,
    #[serde(default)]
    pub api_key_ref: Option<String>,
    #[serde(default)]
    pub extra_headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Anthropic,
    OpenAiCompatible,
    GoogleGenai,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TargetConfig {
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub auth_header: Option<String>,
    #[serde(default)]
    pub database_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorConfig {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    #[serde(default = "default_history_limit")]
    pub history_limit: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CloudConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub sync_token_ref: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedProvider {
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: String,
    pub auth: ResolvedProviderAuth,
    pub auth_status: ProviderAuthStatus,
    pub extra_headers: BTreeMap<String, String>,
}

fn default_max_iterations() -> usize {
    8
}

fn default_history_limit() -> usize {
    100
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            max_iterations: default_max_iterations(),
            history_limit: default_history_limit(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default: "ollama".to_string(),
            providers: vec![
                ProviderConfig {
                    name: "claude".to_string(),
                    kind: ProviderKind::Anthropic,
                    base_url: "https://api.anthropic.com".to_string(),
                    model: "claude-sonnet-4".to_string(),
                    auth: None,
                    api_key_ref: Some("anthropic".to_string()),
                    extra_headers: BTreeMap::new(),
                },
                ProviderConfig {
                    name: "ollama".to_string(),
                    kind: ProviderKind::OpenAiCompatible,
                    base_url: "http://localhost:11434/v1".to_string(),
                    model: "llama3.1".to_string(),
                    auth: Some(ProviderAuthConfig::None),
                    api_key_ref: None,
                    extra_headers: BTreeMap::new(),
                },
            ],
            target: TargetConfig::default(),
            cloud: CloudConfig::default(),
            behavior: BehaviorConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn load_or_init(paths: &ConfigPaths) -> Result<Self> {
        paths.ensure()?;
        if !paths.config.exists() {
            let config = Self::default();
            config.save(paths)?;
            return Ok(config);
        }
        Self::load(paths)
    }

    pub fn load(paths: &ConfigPaths) -> Result<Self> {
        let raw = fs::read_to_string(&paths.config)
            .with_context(|| format!("failed to read {}", paths.config.display()))?;
        toml::from_str(&raw).with_context(|| format!("failed to parse {}", paths.config.display()))
    }

    pub fn save(&self, paths: &ConfigPaths) -> Result<()> {
        paths.ensure()?;
        let raw = toml::to_string_pretty(self)?;
        fs::write(&paths.config, raw)
            .with_context(|| format!("failed to write {}", paths.config.display()))
    }

    pub fn sample_toml() -> Result<String> {
        Ok(toml::to_string_pretty(&Self::default())?)
    }

    pub fn provider_statuses(&self) -> Vec<ResolvedProviderSummary> {
        self.providers
            .iter()
            .map(|provider| ResolvedProviderSummary {
                name: provider.name.clone(),
                kind: provider.kind.clone(),
                base_url: provider.base_url.clone(),
                model: provider.model.clone(),
                auth_status: inspect_provider_auth(&provider.name, provider, None),
            })
            .collect()
    }

    pub fn provider_statuses_with_paths(
        &self,
        paths: &ConfigPaths,
    ) -> Vec<ResolvedProviderSummary> {
        self.providers
            .iter()
            .map(|provider| {
                let cloud_auth = if self.cloud.enabled {
                    load_synced_provider_connection(paths, &provider.name)
                        .ok()
                        .flatten()
                        .map(|connection| connection.auth)
                } else {
                    None
                };

                ResolvedProviderSummary {
                    name: provider.name.clone(),
                    kind: provider.kind.clone(),
                    base_url: provider.base_url.clone(),
                    model: provider.model.clone(),
                    auth_status: inspect_provider_auth(
                        &provider.name,
                        provider,
                        cloud_auth.as_ref(),
                    ),
                }
            })
            .collect()
    }

    pub fn resolve_provider(
        &self,
        provider_name: Option<&str>,
        model_override: Option<&str>,
    ) -> Result<ResolvedProvider> {
        self.resolve_provider_with_paths(None, provider_name, model_override)
    }

    pub fn resolve_provider_with_paths(
        &self,
        paths: Option<&ConfigPaths>,
        provider_name: Option<&str>,
        model_override: Option<&str>,
    ) -> Result<ResolvedProvider> {
        let provider_name = provider_name.unwrap_or(&self.default);
        let provider = self
            .providers
            .iter()
            .find(|p| p.name == provider_name)
            .with_context(|| format!("provider '{}' not found in config", provider_name))?;
        let cloud_auth = if self.cloud.enabled {
            paths
                .and_then(|paths| load_synced_provider_connection(paths, provider_name).ok())
                .flatten()
                .map(|connection| connection.auth)
        } else {
            None
        };
        let auth = resolve_provider_auth(provider_name, provider, cloud_auth.as_ref())?;
        let auth_status = inspect_provider_auth(provider_name, provider, cloud_auth.as_ref());

        Ok(ResolvedProvider {
            name: provider.name.clone(),
            kind: provider.kind.clone(),
            base_url: provider.base_url.clone(),
            model: model_override.unwrap_or(&provider.model).to_string(),
            auth,
            auth_status,
            extra_headers: provider.extra_headers.clone(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedProviderSummary {
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: String,
    pub auth_status: ProviderAuthStatus,
}

pub fn load_secret(name: &str) -> Result<String> {
    Entry::new("appctl", name)?
        .get_password()
        .with_context(|| format!("failed to load secret '{}' from keychain", name))
}

pub fn save_secret(name: &str, value: &str) -> Result<()> {
    Entry::new("appctl", name)?
        .set_password(value)
        .with_context(|| format!("failed to save secret '{}' to keychain", name))
}

pub fn delete_secret(name: &str) -> Result<()> {
    Entry::new("appctl", name)?
        .delete_credential()
        .with_context(|| format!("failed to delete secret '{}' from keychain", name))
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let payload = serde_json::to_string_pretty(value)?;
    fs::write(path, payload).with_context(|| format!("failed to write {}", path.display()))
}

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let payload =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&payload).with_context(|| format!("failed to parse {}", path.display()))
}
