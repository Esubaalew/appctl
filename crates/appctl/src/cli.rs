use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use tracing_subscriber::{EnvFilter, fmt};

use crate::{
    auth::oauth::{OAuthLoginConfig, login as oauth_login},
    chat::{ChatOptions, run_chat},
    config::{AppConfig, ConfigPaths, load_secret},
    doctor::{DoctorRunArgs, run_doctor},
    history::{HistoryCommand, run_history_command},
    plugins,
    run::{RunOptions, run_once},
    serve::{ServeOptions, run_server},
    sync::{SyncRequest, run_sync},
};

#[derive(Debug, Parser)]
#[command(
    name = "appctl",
    version,
    about = "One command. Any app. Full AI control."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    #[arg(long, global = true, default_value = ".appctl")]
    pub app_dir: PathBuf,

    #[arg(long, global = true, default_value = "info")]
    pub log_level: String,
}

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum Command {
    Sync(SyncArgs),
    Chat(ChatArgs),
    Run(RunArgs),
    Doctor(DoctorArgsCli),
    History(HistoryArgs),
    Serve(ServeArgs),
    Config(ConfigArgs),
    Plugin(PluginArgs),
    Auth(AuthArgs),
}

#[derive(Debug, Args)]
pub struct DoctorArgsCli {
    /// Write provenance=verified for routes that did not return 404.
    #[arg(long)]
    pub write: bool,
    #[arg(long, default_value_t = 10)]
    pub timeout_secs: u64,
}

#[derive(Debug, Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub command: AuthSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum AuthSubcommand {
    Login {
        provider: String,
        #[arg(long)]
        client_id: Option<String>,
        #[arg(long)]
        client_secret: Option<String>,
        #[arg(long)]
        auth_url: Option<String>,
        #[arg(long)]
        token_url: Option<String>,
        #[arg(long)]
        scope: Vec<String>,
        #[arg(long, default_value_t = 8421)]
        redirect_port: u16,
    },
    Status {
        provider: String,
    },
}

#[derive(Debug, Args)]
pub struct SyncArgs {
    #[arg(long)]
    pub openapi: Option<String>,
    #[arg(long)]
    pub django: Option<PathBuf>,
    #[arg(long)]
    pub db: Option<String>,
    #[arg(long)]
    pub url: Option<String>,
    #[arg(long)]
    pub mcp: Option<String>,
    #[arg(long)]
    pub rails: Option<PathBuf>,
    #[arg(long)]
    pub laravel: Option<PathBuf>,
    #[arg(long)]
    pub aspnet: Option<PathBuf>,
    #[arg(long)]
    pub strapi: Option<PathBuf>,
    #[arg(long)]
    pub supabase: Option<String>,
    #[arg(long)]
    pub supabase_anon_ref: Option<String>,
    /// Invoke a dynamic plugin by name, e.g. `--plugin airtable`.
    #[arg(long)]
    pub plugin: Option<String>,
    #[arg(long)]
    pub auth_header: Option<String>,
    #[arg(long)]
    pub base_url: Option<String>,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub login_url: Option<String>,
    #[arg(long)]
    pub login_user: Option<String>,
    #[arg(long)]
    pub login_password: Option<String>,
    #[arg(long)]
    pub login_form_selector: Option<String>,
}

#[derive(Debug, Args)]
pub struct ChatArgs {
    #[arg(long)]
    pub provider: Option<String>,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub read_only: bool,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub confirm: bool,
    /// Block inferred HTTP tools until `appctl doctor --write` marks them verified.
    #[arg(long)]
    pub strict: bool,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    pub prompt: String,
    #[arg(long)]
    pub provider: Option<String>,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub read_only: bool,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub confirm: bool,
    #[arg(long)]
    pub strict: bool,
}

#[derive(Debug, Args)]
pub struct HistoryArgs {
    #[arg(long, default_value_t = 20)]
    pub last: usize,
    #[arg(long)]
    pub undo: Option<i64>,
}

#[derive(Debug, Args)]
pub struct ServeArgs {
    #[arg(long, default_value_t = 4242)]
    pub port: u16,
    #[arg(long, default_value = "127.0.0.1")]
    pub bind: String,
    #[arg(long)]
    pub token: Option<String>,
    #[arg(long)]
    pub provider: Option<String>,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub strict: bool,
    #[arg(long)]
    pub read_only: bool,
    #[arg(long)]
    pub dry_run: bool,
    /// Auto-approve mutating tools (on by default for non-interactive `serve`).
    #[arg(long, default_value_t = true)]
    pub confirm: bool,
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigSubcommand {
    Init,
    Show,
    ProviderSample,
    /// Store a secret in the OS keychain (service `appctl`). Env vars still override at runtime.
    SetSecret {
        name: String,
        /// Value to store; if omitted, read from stdin (TTY prompt in future).
        #[arg(long)]
        value: Option<String>,
    },
}

#[derive(Debug, Args)]
pub struct PluginArgs {
    #[command(subcommand)]
    pub command: PluginSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum PluginSubcommand {
    List,
    Install { name: String },
}

impl Cli {
    pub async fn run(self) -> Result<()> {
        init_tracing(&self.log_level)?;

        let paths = ConfigPaths::new(self.app_dir.clone());

        match self.command {
            Command::Sync(args) => {
                if let Some(name) = args.plugin.as_deref() {
                    run_dynamic_sync(paths, name, args.base_url.as_deref())?;
                } else {
                    let request = SyncRequest {
                        openapi: args.openapi,
                        django: args.django,
                        db: args.db,
                        url: args.url,
                        mcp: args.mcp,
                        rails: args.rails,
                        laravel: args.laravel,
                        aspnet: args.aspnet,
                        strapi: args.strapi,
                        supabase: args.supabase,
                        supabase_anon_ref: args.supabase_anon_ref,
                        auth_header: args.auth_header,
                        base_url: args.base_url,
                        force: args.force,
                        login_url: args.login_url,
                        login_user: args.login_user,
                        login_password: args.login_password,
                        login_form_selector: args.login_form_selector,
                    };
                    run_sync(paths, request).await?;
                }
            }
            Command::Chat(args) => {
                let config = AppConfig::load_or_init(&paths)?;
                run_chat(
                    &paths,
                    &config,
                    ChatOptions {
                        provider: args.provider,
                        model: args.model,
                        read_only: args.read_only,
                        dry_run: args.dry_run,
                        confirm: args.confirm,
                        strict: args.strict,
                    },
                )
                .await?;
            }
            Command::Run(args) => {
                let config = AppConfig::load_or_init(&paths)?;
                run_once(
                    &paths,
                    &config,
                    RunOptions {
                        prompt: args.prompt,
                        provider: args.provider,
                        model: args.model,
                        read_only: args.read_only,
                        dry_run: args.dry_run,
                        confirm: args.confirm,
                        strict: args.strict,
                    },
                )
                .await?;
            }
            Command::Doctor(args) => {
                run_doctor(
                    &paths,
                    DoctorRunArgs {
                        write: args.write,
                        timeout_secs: args.timeout_secs,
                    },
                )
                .await?;
            }
            Command::History(args) => {
                run_history_command(
                    &paths,
                    HistoryCommand {
                        last: args.last,
                        undo: args.undo,
                    },
                )
                .await?;
            }
            Command::Serve(args) => {
                let config = AppConfig::load_or_init(&paths)?;
                run_server(
                    paths,
                    config,
                    ServeOptions {
                        port: args.port,
                        bind: args.bind,
                        token: args.token,
                        provider: args.provider,
                        model: args.model,
                        strict: args.strict,
                        read_only: args.read_only,
                        dry_run: args.dry_run,
                        confirm: args.confirm,
                    },
                )
                .await?;
            }
            Command::Config(args) => match args.command {
                ConfigSubcommand::Init => {
                    let config = AppConfig::default();
                    config.save(&paths)?;
                    println!("Initialized {}", paths.config.display());
                }
                ConfigSubcommand::Show => {
                    let config = AppConfig::load_or_init(&paths)?;
                    println!("{}", toml::to_string_pretty(&config)?);
                }
                ConfigSubcommand::ProviderSample => {
                    println!("{}", AppConfig::sample_toml()?);
                }
                ConfigSubcommand::SetSecret { name, value } => {
                    let v = match value {
                        Some(s) => s,
                        None => dialoguer::Password::new()
                            .with_prompt(format!("Enter secret `{name}`"))
                            .interact()?,
                    };
                    crate::config::save_secret(&name, &v)?;
                    println!("stored secret '{}' in keychain", name);
                }
            },
            Command::Plugin(args) => match args.command {
                PluginSubcommand::List => {
                    println!(
                        "Built-in sync plugins: openapi, django, db, url, mcp, rails, laravel, aspnet, strapi, supabase"
                    );
                    let dir = plugins::plugin_dir()?;
                    println!("Dynamic plugin directory: {}", dir.display());
                    match plugins::discover() {
                        Ok(found) if found.is_empty() => {
                            println!("(no dynamic plugins installed)");
                        }
                        Ok(found) => {
                            println!("Dynamic plugins:");
                            for plugin in found {
                                println!(
                                    "  - {} v{} ({})",
                                    plugin.name,
                                    plugin.version,
                                    plugin.source_path.display()
                                );
                            }
                        }
                        Err(err) => tracing::warn!("failed to enumerate plugins: {err:#}"),
                    }
                }
                PluginSubcommand::Install { name } => {
                    install_plugin(&name)?;
                }
            },
            Command::Auth(args) => match args.command {
                AuthSubcommand::Login {
                    provider,
                    client_id,
                    client_secret,
                    auth_url,
                    token_url,
                    scope,
                    redirect_port,
                } => {
                    let client_id = client_id
                        .or_else(|| std::env::var(format!("{provider}_CLIENT_ID")).ok())
                        .context("--client-id is required (or set <provider>_CLIENT_ID)")?;
                    let auth_url = auth_url.context(
                        "--auth-url is required (the provider's authorization endpoint)",
                    )?;
                    let token_url = token_url
                        .context("--token-url is required (the provider's token endpoint)")?;
                    let config = OAuthLoginConfig {
                        provider: provider.clone(),
                        client_id,
                        client_secret: client_secret
                            .or_else(|| std::env::var(format!("{provider}_CLIENT_SECRET")).ok()),
                        auth_url,
                        token_url,
                        scopes: scope,
                        redirect_port,
                    };
                    let tokens = oauth_login(config).await?;
                    println!(
                        "Logged in as '{}'. Access token stored in keychain ({} scopes).",
                        provider,
                        tokens.scopes.len()
                    );
                }
                AuthSubcommand::Status { provider } => {
                    match load_secret(&format!("appctl_oauth::{provider}")) {
                        Ok(raw) if !raw.is_empty() => {
                            println!(
                                "provider '{}' has stored tokens ({} bytes)",
                                provider,
                                raw.len()
                            );
                        }
                        _ => println!("no tokens stored for '{}'", provider),
                    }
                }
            },
        }

        Ok(())
    }
}

fn run_dynamic_sync(paths: ConfigPaths, name: &str, base_url: Option<&str>) -> Result<()> {
    paths.ensure()?;
    let plugins = plugins::discover()?;
    let plugin = plugins
        .into_iter()
        .find(|p| p.name == name)
        .with_context(|| {
            format!(
                "no dynamic plugin named '{}' installed in {:?}",
                name,
                plugins::plugin_dir().ok()
            )
        })?;
    let input = appctl_plugin_sdk::SyncInput {
        base_url: base_url.map(|s| s.to_string()),
        ..Default::default()
    };
    let mut schema = plugin.introspect(&input)?;
    if let Some(b) = base_url {
        schema.base_url = Some(b.to_string());
    }
    let tools = crate::tools::schema_to_tools(&schema);
    crate::config::write_json(&paths.schema, &schema)?;
    crate::config::write_json(&paths.tools, &tools)?;
    println!(
        "Synced via dynamic plugin '{}': {} resources, {} tools",
        plugin.name,
        schema.resources.len(),
        tools.len()
    );
    Ok(())
}

fn install_plugin(source: &str) -> Result<()> {
    use std::process::Command;

    let dir = plugins::plugin_dir()?;
    std::fs::create_dir_all(&dir)?;

    // If `source` points at an existing file, just copy it.
    let src_path = std::path::PathBuf::from(source);
    if src_path.exists() && src_path.is_file() {
        let dest = dir.join(src_path.file_name().context("no file name")?);
        std::fs::copy(&src_path, &dest)?;
        println!("Installed {} -> {}", src_path.display(), dest.display());
        return Ok(());
    }

    // Otherwise try to `cargo install` the plugin from a git url or crate name.
    let staging = tempfile::TempDir::new()?;
    let target_dir = staging.path().join("target");
    let status = if source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("git@")
    {
        Command::new("cargo")
            .args([
                "install",
                "--git",
                source,
                "--target-dir",
                target_dir.to_str().unwrap(),
                "--force",
            ])
            .status()
    } else {
        Command::new("cargo")
            .args([
                "install",
                source,
                "--target-dir",
                target_dir.to_str().unwrap(),
                "--force",
            ])
            .status()
    }
    .context("failed to spawn cargo install")?;
    if !status.success() {
        bail!(
            "cargo install for '{}' failed; build it manually as a cdylib and drop the library into {}",
            source,
            dir.display()
        );
    }

    // Walk the target dir and copy any cdylib artifact into ~/.appctl/plugins/
    let mut installed = 0;
    for entry in walkdir::WalkDir::new(&target_dir) {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        if matches!(ext, "dylib" | "so" | "dll")
            && let Some(name) = path.file_name()
        {
            let dest = dir.join(name);
            std::fs::copy(path, &dest)?;
            println!("Installed {} -> {}", path.display(), dest.display());
            installed += 1;
        }
    }
    if installed == 0 {
        bail!(
            "no cdylib artifacts produced; ensure the plugin's Cargo.toml has `crate-type = [\"cdylib\"]`"
        );
    }
    Ok(())
}

fn init_tracing(log_level: &str) -> Result<()> {
    let filter = EnvFilter::try_new(log_level)
        .or_else(|_| EnvFilter::try_new("info"))
        .context("invalid log filter")?;

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init()
        .ok();

    Ok(())
}
