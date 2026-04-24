use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use anyhow::{Context, Result, bail};
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppRegistry {
    #[serde(default)]
    pub active: Option<String>,
    #[serde(default)]
    pub apps: BTreeMap<String, PathBuf>,
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

    pub fn has_synced_artifacts(&self) -> bool {
        self.schema.exists() || self.tools.exists()
    }

    fn runtime_setup_message(&self, command: &str) -> String {
        format!(
            "No provider configured for app dir {}.\n\
This app dir already has synced schema/tools, but it does not have a usable {}.\n\
Run `appctl --app-dir {} init` to configure a provider, or copy a working config.toml into this folder, then retry `appctl --app-dir {} {}`.",
            self.root.display(),
            self.config.display(),
            self.root.display(),
            self.root.display(),
            command
        )
    }
}

impl AppRegistry {
    pub fn file_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("failed to locate home directory")?;
        Ok(home.join(".appctl").join("apps.toml"))
    }

    pub fn load_or_default() -> Result<Self> {
        let path = Self::file_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read app registry {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::file_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let raw = toml::to_string_pretty(self)?;
        fs::write(&path, raw).with_context(|| format!("failed to write {}", path.display()))
    }

    pub fn register_and_activate(&mut self, name: String, app_dir: PathBuf) {
        self.apps.insert(name.clone(), app_dir);
        self.active = Some(name);
    }

    pub fn remove(&mut self, name: &str) -> Option<PathBuf> {
        let removed = self.apps.remove(name);
        if self.active.as_deref() == Some(name) {
            self.active = None;
        }
        removed
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub default: String,
    #[serde(default, rename = "provider")]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub target: TargetConfig,
    #[serde(default)]
    pub cloud: CloudConfig,
    #[serde(default)]
    pub behavior: BehaviorConfig,
    #[serde(default)]
    pub tooling: ToolingConfig,
    /// Human-friendly name for chat, `serve` UI, and logs. If unset, the global registry name
    /// (or parent folder, for unregistered apps) is shown.
    #[serde(default)]
    pub display_name: Option<String>,
    /// One-line blurb in the chat banner and `/config/public` (set during `appctl init` or in TOML).
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: String,
    #[serde(default = "default_provider_verified")]
    pub verified: bool,
    #[serde(default)]
    pub auth: Option<ProviderAuthConfig>,
    #[serde(default)]
    pub api_key_ref: Option<String>,
    #[serde(default)]
    pub extra_headers: BTreeMap<String, String>,
}

fn default_provider_verified() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Anthropic,
    OpenAiCompatible,
    GoogleGenai,
    Vertex,
    AzureOpenAi,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TargetConfig {
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub base_url_env: Option<String>,
    #[serde(default)]
    pub auth_header: Option<String>,
    /// Default query parameters for HTTP tools. Keys that appear in a tool’s OpenAPI query
    /// list are filled from here when the model does not pass them; tool arguments still win.
    /// Use `env:VAR` as the value to read from the process environment.
    #[serde(default)]
    pub default_query: BTreeMap<String, String>,
    #[serde(default)]
    pub database_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolingConfig {
    #[serde(default)]
    pub pin: Vec<String>,
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
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

impl AppConfig {
    pub fn resolve_tool_name<'a>(&'a self, tool_name: &'a str) -> &'a str {
        self.tooling
            .aliases
            .get(tool_name)
            .map(String::as_str)
            .unwrap_or(tool_name)
    }

    /// Label for chat/serve banners. Prefer `display_name` when set, otherwise the registry
    /// (or folder) name for this app.
    pub fn banner_label<'a>(&'a self, registry_name: &'a str) -> &'a str {
        self.display_name
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(registry_name)
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

    pub fn load_for_runtime(paths: &ConfigPaths, command: &str) -> Result<Self> {
        paths.ensure()?;
        if !paths.config.exists() {
            if paths.has_synced_artifacts() {
                bail!(paths.runtime_setup_message(command));
            }
            let config = Self::default();
            config.save(paths)?;
            return Ok(config);
        }

        let config = Self::load(paths)?;
        if config.providers.is_empty() && paths.has_synced_artifacts() {
            bail!(paths.runtime_setup_message(command));
        }
        Ok(config)
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
                kind: provider.kind,
                base_url: provider.base_url.clone(),
                model: provider.model.clone(),
                verified: provider.verified,
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
                    kind: provider.kind,
                    base_url: provider.base_url.clone(),
                    model: provider.model.clone(),
                    verified: provider.verified,
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
        if self.providers.is_empty() {
            bail!("No provider configured. Run `appctl init`.")
        }
        let provider_name = provider_name.unwrap_or(&self.default);
        if provider_name.is_empty() {
            bail!("No provider configured. Run `appctl init`.")
        }
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
        if matches!(auth, ResolvedProviderAuth::McpBridge { .. }) {
            bail!(
                "Provider '{}' is configured as an MCP bridge. Launch the external client instead, or run `appctl init` to pick a direct API provider.",
                provider_name
            )
        }

        Ok(ResolvedProvider {
            name: provider.name.clone(),
            kind: provider.kind,
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
    pub verified: bool,
    pub auth_status: ProviderAuthStatus,
}

pub fn load_secret(name: &str) -> Result<String> {
    if let Some(value) = secret_cache()
        .lock()
        .expect("secret cache poisoned")
        .get(name)
        .cloned()
    {
        return Ok(value);
    }
    let value = Entry::new("appctl", name)?
        .get_password()
        .with_context(|| format!("failed to load secret '{}' from keychain", name))?;
    secret_cache()
        .lock()
        .expect("secret cache poisoned")
        .insert(name.to_string(), value.clone());
    Ok(value)
}

pub fn save_secret(name: &str, value: &str) -> Result<()> {
    Entry::new("appctl", name)?
        .set_password(value)
        .with_context(|| format!("failed to save secret '{}' to keychain", name))?;
    secret_cache()
        .lock()
        .expect("secret cache poisoned")
        .insert(name.to_string(), value.to_string());
    Ok(())
}

pub fn delete_secret(name: &str) -> Result<()> {
    Entry::new("appctl", name)?
        .delete_credential()
        .with_context(|| format!("failed to delete secret '{}' from keychain", name))?;
    secret_cache()
        .lock()
        .expect("secret cache poisoned")
        .remove(name);
    Ok(())
}

fn secret_cache() -> &'static Mutex<BTreeMap<String, String>> {
    static SECRET_CACHE: OnceLock<Mutex<BTreeMap<String, String>>> = OnceLock::new();
    SECRET_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
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

pub fn normalize_app_dir(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn app_name_from_dir(app_dir: &Path) -> String {
    app_dir
        .parent()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "app".to_string())
}

/// `true` when the default registry label would be the home directory basename (e.g. `~/.appctl` →
/// `esubalew`), which is often a poor global name in `appctl app list`.
pub fn registry_default_looks_like_os_username(suggested: &str, app_dir: &Path) -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let home_appctl = home.join(".appctl");
    if normalize_app_dir(app_dir) != normalize_app_dir(&home_appctl) {
        return false;
    }
    home.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|h| h == suggested)
}

pub fn find_registered_app_name(registry: &AppRegistry, app_dir: &Path) -> Option<String> {
    let normalized = normalize_app_dir(app_dir);
    registry.apps.iter().find_map(|(name, registered)| {
        if normalize_app_dir(registered) == normalized {
            Some(name.clone())
        } else {
            None
        }
    })
}

pub fn find_app_dir_from(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        let candidate = current.join(".appctl");
        if candidate.exists() {
            return Some(normalize_app_dir(&candidate));
        }
        if !current.pop() {
            return None;
        }
    }
}

pub fn active_app_path(registry: &AppRegistry) -> Option<(String, PathBuf)> {
    let active = registry.active.as_ref()?;
    let path = registry.apps.get(active)?;
    Some((active.clone(), normalize_app_dir(path)))
}

#[cfg(test)]
mod tests {
    use super::{
        AppConfig, AppRegistry, ConfigPaths, active_app_path, app_name_from_dir, find_app_dir_from,
        find_registered_app_name, normalize_app_dir, registry_default_looks_like_os_username,
        write_json,
    };
    use serde_json::json;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    #[test]
    fn load_for_runtime_explains_missing_provider_for_synced_app_dir() {
        let dir = tempdir().unwrap();
        let paths = ConfigPaths::new(dir.path().join(".appctl"));
        paths.ensure().unwrap();
        write_json(&paths.schema, &json!({"resources": []})).unwrap();

        let err = AppConfig::load_for_runtime(&paths, "chat").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("No provider configured for app dir"));
        assert!(msg.contains("appctl --app-dir"));
        assert!(msg.contains("init"));
        assert!(msg.contains("chat"));
    }

    #[test]
    fn load_for_runtime_rejects_empty_config_for_synced_app_dir() {
        let dir = tempdir().unwrap();
        let paths = ConfigPaths::new(dir.path().join(".appctl"));
        paths.ensure().unwrap();
        write_json(&paths.tools, &json!([])).unwrap();
        AppConfig::default().save(&paths).unwrap();

        let err = AppConfig::load_for_runtime(&paths, "run").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("No provider configured for app dir"));
        assert!(msg.contains("run"));
    }

    #[test]
    fn app_name_from_dir_uses_parent_folder() {
        let app_dir = PathBuf::from("/tmp/botlink/.appctl");
        assert_eq!(app_name_from_dir(&app_dir), "botlink");
    }

    #[test]
    fn banner_label_prefers_display_name() {
        let mut c = AppConfig::default();
        assert_eq!(c.banner_label("esubalew"), "esubalew");
        c.display_name = Some("Home APIs".to_string());
        assert_eq!(c.banner_label("esubalew"), "Home APIs");
    }

    #[test]
    fn registry_username_heuristic_not_for_random_paths() {
        let tmp = std::env::temp_dir().join("appctl-regtest").join(".appctl");
        assert!(!registry_default_looks_like_os_username("anything", &tmp));
    }

    #[test]
    fn find_registered_app_name_matches_normalized_paths() {
        let mut registry = AppRegistry::default();
        registry
            .apps
            .insert("playground".to_string(), PathBuf::from("./.appctl"));
        let found = find_registered_app_name(&registry, &normalize_app_dir(Path::new("./.appctl")));
        assert_eq!(found.as_deref(), Some("playground"));
    }

    #[test]
    fn find_app_dir_from_walks_upward() {
        let dir = tempdir().unwrap();
        let app_dir = dir.path().join(".appctl");
        let nested = dir.path().join("src").join("ui");
        std::fs::create_dir_all(&app_dir).unwrap();
        std::fs::create_dir_all(&nested).unwrap();

        let found = find_app_dir_from(&nested).unwrap();
        assert_eq!(normalize_app_dir(&found), normalize_app_dir(&app_dir));
    }

    #[test]
    fn active_app_path_returns_active_registration() {
        let registry = AppRegistry {
            active: Some("ordering".to_string()),
            apps: std::iter::once(("ordering".to_string(), PathBuf::from("./.appctl"))).collect(),
        };

        let (name, path) = active_app_path(&registry).unwrap();
        assert_eq!(name, "ordering");
        assert_eq!(
            normalize_app_dir(&path),
            normalize_app_dir(Path::new("./.appctl"))
        );
    }
}
