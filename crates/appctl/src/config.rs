use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use keyring::Entry;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct ConfigPaths {
    pub root: PathBuf,
    pub config: PathBuf,
    pub schema: PathBuf,
    pub tools: PathBuf,
    pub history: PathBuf,
}

impl ConfigPaths {
    pub fn new(root: PathBuf) -> Self {
        Self {
            config: root.join("config.toml"),
            schema: root.join("schema.json"),
            tools: root.join("tools.json"),
            history: root.join("history.db"),
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
    pub behavior: BehaviorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedProvider {
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
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
                    api_key_ref: Some("anthropic".to_string()),
                    extra_headers: BTreeMap::new(),
                },
                ProviderConfig {
                    name: "ollama".to_string(),
                    kind: ProviderKind::OpenAiCompatible,
                    base_url: "http://localhost:11434/v1".to_string(),
                    model: "llama3.1".to_string(),
                    api_key_ref: None,
                    extra_headers: BTreeMap::new(),
                },
            ],
            target: TargetConfig::default(),
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

    pub fn resolve_provider(
        &self,
        provider_name: Option<&str>,
        model_override: Option<&str>,
    ) -> Result<ResolvedProvider> {
        let provider_name = provider_name.unwrap_or(&self.default);
        let provider = self
            .providers
            .iter()
            .find(|p| p.name == provider_name)
            .with_context(|| format!("provider '{}' not found in config", provider_name))?;

        let api_key = provider
            .api_key_ref
            .as_deref()
            .and_then(|name| load_secret(name).ok().or_else(|| std::env::var(name).ok()))
            .filter(|value| !value.is_empty());

        if let Some(secret_name) = provider.api_key_ref.as_deref() {
            if api_key.is_none() {
                bail!(
                    "provider '{}' requires secret '{}'; set the env var or run `appctl config set-secret {} --value ...`",
                    provider.name,
                    secret_name,
                    secret_name
                );
            }
        }

        Ok(ResolvedProvider {
            name: provider.name.clone(),
            kind: provider.kind.clone(),
            base_url: provider.base_url.clone(),
            model: model_override.unwrap_or(&provider.model).to_string(),
            api_key,
            extra_headers: provider.extra_headers.clone(),
        })
    }
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

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let payload = serde_json::to_string_pretty(value)?;
    fs::write(path, payload).with_context(|| format!("failed to write {}", path.display()))
}

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let payload =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&payload).with_context(|| format!("failed to parse {}", path.display()))
}
