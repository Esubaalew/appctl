use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::auth::provider::McpBridgeClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpBridgeResult {
    pub client: McpBridgeClient,
    pub config_path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub launch_command: String,
}

pub fn install_bridge(client: McpBridgeClient, app_dir: &Path) -> Result<McpBridgeResult> {
    ensure_client_installed(client)?;
    let appctl_path =
        std::env::current_exe().context("failed to resolve the current appctl binary")?;
    let args = vec![
        "--app-dir".to_string(),
        app_dir.display().to_string(),
        "mcp".to_string(),
        "serve".to_string(),
    ];
    match client {
        McpBridgeClient::Codex => write_codex_config(client, &appctl_path, &args, app_dir),
        McpBridgeClient::Claude | McpBridgeClient::QwenCode | McpBridgeClient::Gemini => {
            write_json_config(client, &appctl_path, &args, app_dir)
        }
    }
}

pub fn ensure_client_installed(client: McpBridgeClient) -> Result<()> {
    let binary = client.binary_name();
    let name = client.display_name();
    let status = match Command::new(binary).arg("--version").status() {
        Ok(s) => s,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            bail!(
                "MCP bridge setup needs the `{binary}` executable on your PATH, but it was not found.\n\
                 ({name} is required for this init option.)\n\n\
                 What to do:\n  \
                 • {}\n  \
                 • Or run `appctl init` again and choose another way to connect (e.g. Gemini API key instead of the subscription bridge).\n",
                client.mcp_bridge_not_found_hint()
            );
        }
        Err(e) => {
            let io = e.to_string();
            return Err(e).with_context(|| {
                format!(
                    "could not run `{binary} --version` — check that {name} is installed and not blocked ({io})"
                )
            });
        }
    };
    if status.success() {
        Ok(())
    } else {
        bail!(
            "`{binary} --version` failed ({status:?}). Install or repair the {name} CLI, then retry `appctl init`.\n\
             If you don’t use this subscription path, pick a different provider in `appctl init`.",
        );
    }
}

fn write_codex_config(
    client: McpBridgeClient,
    appctl_path: &Path,
    args: &[String],
    app_dir: &Path,
) -> Result<McpBridgeResult> {
    let config_path = dirs::home_dir()
        .context("failed to resolve the home directory")?
        .join(".codex/config.toml");
    let backup_path = backup_if_exists(&config_path)?;
    let mut doc = if config_path.exists() {
        fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?
            .parse::<toml::Table>()
            .with_context(|| format!("failed to parse {}", config_path.display()))?
    } else {
        toml::Table::new()
    };

    let server = toml::Table::from_iter([
        (
            "command".to_string(),
            toml::Value::String(appctl_path.display().to_string()),
        ),
        (
            "args".to_string(),
            toml::Value::Array(args.iter().cloned().map(toml::Value::String).collect()),
        ),
    ]);

    let servers = doc
        .entry("mcp_servers".to_string())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    let Some(servers) = servers.as_table_mut() else {
        bail!(
            "{} has a non-table `mcp_servers` key",
            config_path.display()
        );
    };
    servers.insert("appctl".to_string(), toml::Value::Table(server));

    write_parented_file(&config_path, &toml::to_string_pretty(&doc)?)?;

    Ok(McpBridgeResult {
        client,
        config_path,
        backup_path,
        launch_command: format!(
            "codex (appctl MCP bridge now points at {})",
            app_dir.display()
        ),
    })
}

fn write_json_config(
    client: McpBridgeClient,
    appctl_path: &Path,
    args: &[String],
    app_dir: &Path,
) -> Result<McpBridgeResult> {
    let config_path = match client {
        McpBridgeClient::Claude => home_file(".claude/settings.json")?,
        McpBridgeClient::QwenCode => home_file(".qwen/settings.json")?,
        McpBridgeClient::Gemini => home_file(".gemini/settings.json")?,
        McpBridgeClient::Codex => unreachable!(),
    };
    let backup_path = backup_if_exists(&config_path)?;
    let mut root = if config_path.exists() {
        serde_json::from_str::<Value>(
            &fs::read_to_string(&config_path)
                .with_context(|| format!("failed to read {}", config_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", config_path.display()))?
    } else {
        Value::Object(Map::new())
    };

    let Some(root_object) = root.as_object_mut() else {
        bail!("{} must contain a JSON object", config_path.display());
    };
    let mcp_servers = root_object
        .entry("mcpServers".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(mcp_servers) = mcp_servers.as_object_mut() else {
        bail!(
            "{} has a non-object `mcpServers` key",
            config_path.display()
        );
    };
    mcp_servers.insert(
        "appctl".to_string(),
        json!({
            "command": appctl_path.display().to_string(),
            "args": args,
        }),
    );

    write_parented_file(&config_path, &serde_json::to_string_pretty(&root)?)?;

    Ok(McpBridgeResult {
        client,
        config_path,
        backup_path,
        launch_command: format!(
            "{} (appctl MCP bridge now points at {})",
            client.binary_name(),
            app_dir.display()
        ),
    })
}

fn home_file(relative: &str) -> Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("failed to resolve the home directory")?
        .join(relative))
}

fn backup_if_exists(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }

    let timestamp = Utc::now().format("%Y%m%d%H%M%S");
    let backup_path = path.with_extension(format!("bak.{timestamp}"));
    fs::copy(path, &backup_path).with_context(|| {
        format!(
            "failed to create backup {} -> {}",
            path.display(),
            backup_path.display()
        )
    })?;
    Ok(Some(backup_path))
}

fn write_parented_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))
}
