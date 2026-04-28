use std::io::IsTerminal as _;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use tracing_subscriber::{EnvFilter, fmt};

use crate::{
    auth::{
        azure_ad::{
            AzureAdDeviceConfig, DEFAULT_SCOPE as AZURE_OPENAI_DEFAULT_SCOPE, device_code_login,
        },
        oauth::{
            OAuthLoginConfig, OAuthTokenNamespace, delete_provider_tokens, login as oauth_login,
        },
        provider::{ProviderAuthConfig, ProviderAuthKind},
    },
    chat::{ChatOptions, run_chat},
    config::{
        AppConfig, AppRegistry, ConfigPaths, active_app_path, app_name_from_dir, delete_secret,
        find_app_dir_from, find_registered_app_name, load_secret, normalize_app_dir,
        registry_default_looks_like_os_username,
    },
    doctor::{DoctorRunArgs, run_doctor, run_doctor_models},
    history::{HistoryCommand, run_history_command},
    init::run_init,
    mcp_server::{McpServeOptions, run_mcp_server},
    plugins,
    run::{RunOptions, run_once},
    serve::{ServeOptions, run_server},
    setup::run_setup,
    sync::{SyncRequest, run_sync},
};

#[derive(Debug, Parser)]
#[command(
    name = "appctl",
    version,
    about = "Sync APIs and data sources into LLM tools; chat, run, serve."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    #[arg(long, global = true)]
    pub app_dir: Option<PathBuf>,

    #[arg(long, global = true, default_value = "info")]
    pub log_level: String,
}

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum Command {
    /// Guided first-run setup: provider, sync source, checks, and next steps.
    Setup,
    /// Set up a `.appctl` directory (models, auth, and provider) interactively.
    Init,
    /// Introspect your stack and (re)build schema and OpenAPI tool definitions.
    Sync(SyncArgs),
    /// Interactive session with the AI for this app's tools.
    Chat(ChatArgs),
    /// Run a single prompt and exit (non-interactive).
    Run(RunArgs),
    /// Check connectivity, config, and which HTTP routes are verified.
    Doctor(DoctorArgsCli),
    /// Inspect and undo tool runs stored in the local history database.
    History(HistoryArgs),
    /// Expose the agent and tools over HTTP (MCP- and AI-friendly).
    Serve(ServeArgs),
    /// Inspect config, TOML samples, and keychain secrets.
    Config(ConfigArgs),
    /// List, install, and load appctl extension plugins.
    Plugin(PluginArgs),
    /// Log in to a target, cloud provider, or direct API.
    Auth(AuthArgs),
    /// Run a built-in MCP (Model Context Protocol) stdio server for this app.
    Mcp(McpArgs),
    /// Manage known app contexts and the global active app.
    #[command(visible_alias = "apps")]
    App(AppArgs),
}

#[derive(Debug, Args)]
pub struct DoctorArgsCli {
    /// Write provenance=verified for routes that did not return 404.
    #[arg(long)]
    pub write: bool,
    #[arg(long, default_value_t = 10)]
    pub timeout_secs: u64,
    #[command(subcommand)]
    pub command: Option<DoctorSubcommand>,
}

#[derive(Debug, Subcommand)]
pub enum DoctorSubcommand {
    /// List models available to the active or selected provider.
    Models {
        /// Name of a provider entry from `.appctl/config.toml` (defaults to the configured default).
        #[arg(long)]
        provider: Option<String>,
    },
}

#[derive(Debug, Args)]
pub struct AppArgs {
    #[command(subcommand)]
    pub command: AppSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum AppSubcommand {
    /// Register an app directory (defaults to detected local `.appctl`) and activate it.
    Add {
        /// App directory to register. Defaults to the resolved `--app-dir`.
        #[arg(long)]
        path: Option<PathBuf>,
        /// Sync an OpenAPI document immediately after registering the app.
        #[arg(long)]
        openapi: Option<String>,
        /// Base URL to store in the synced schema when `--openapi` is used.
        #[arg(long)]
        base_url: Option<String>,
        /// Override the inferred auth header when `--openapi` is used.
        #[arg(long)]
        auth_header: Option<String>,
        /// Overwrite an existing schema/tools set during the immediate sync.
        #[arg(long)]
        force: bool,
        /// Human-friendly name (stored in this app's `config.toml` as `display_name`, shown in chat/serve).
        #[arg(long)]
        display_name: Option<String>,
        /// One-line description (stored as `description` in this app's `config.toml`, shown in the chat banner).
        #[arg(long)]
        description: Option<String>,
        /// Name to register in `~/.appctl`. Defaults to the parent directory name of this `.appctl`.
        /// Put this last, after flags, e.g. `appctl app add --display-name "X" myapp` (clap requires optional positionals after flags).
        #[arg(value_name = "NAME")]
        name: Option<String>,
    },
    /// List all registered apps and show the active one.
    List,
    /// Set the global active app by name.
    Use { name: String },
    /// Remove a registered app by name.
    Remove { name: String },
}

#[derive(Debug, Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub command: AuthSubcommand,
}

#[derive(Debug)]
struct ProviderLoginRequest {
    profile: Option<String>,
    value: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    auth_url: Option<String>,
    token_url: Option<String>,
    scope: Vec<String>,
    redirect_port: u16,
}

#[derive(Debug, Subcommand)]
pub enum AuthSubcommand {
    /// Deprecated alias for `appctl auth target login`.
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
    /// Deprecated alias for `appctl auth target status`.
    Status { provider: String },
    Target {
        #[command(subcommand)]
        command: TargetAuthSubcommand,
    },
    Provider {
        #[command(subcommand)]
        command: ProviderAuthSubcommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum TargetAuthSubcommand {
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
    Logout {
        provider: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProviderAuthSubcommand {
    Login {
        provider: String,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        value: Option<String>,
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
        provider: Option<String>,
    },
    Logout {
        provider: String,
    },
    List,
}

#[derive(Debug, Args)]
pub struct SyncArgs {
    /// OpenAPI document URL or file path.
    #[arg(long)]
    pub openapi: Option<String>,
    /// Django project root.
    #[arg(long)]
    pub django: Option<PathBuf>,
    /// Flask project root.
    #[arg(long)]
    pub flask: Option<PathBuf>,
    /// Database or datastore connection string.
    #[arg(long)]
    pub db: Option<String>,
    /// Browser-login site root.
    #[arg(long)]
    pub url: Option<String>,
    /// Remote MCP server URL.
    #[arg(long)]
    pub mcp: Option<String>,
    /// Rails project root.
    #[arg(long)]
    pub rails: Option<PathBuf>,
    /// Laravel project root.
    #[arg(long)]
    pub laravel: Option<PathBuf>,
    /// ASP.NET project root.
    #[arg(long)]
    pub aspnet: Option<PathBuf>,
    /// Strapi project root.
    #[arg(long)]
    pub strapi: Option<PathBuf>,
    /// Supabase project URL.
    #[arg(long)]
    pub supabase: Option<String>,
    /// Secret name that holds the Supabase anon key.
    #[arg(long)]
    pub supabase_anon_ref: Option<String>,
    /// Invoke a dynamic plugin by name, e.g. `--plugin airtable`.
    #[arg(long)]
    pub plugin: Option<String>,
    /// Force a specific Authorization header into the synced schema.
    #[arg(long)]
    pub auth_header: Option<String>,
    /// Override the base URL written to the schema.
    #[arg(long)]
    pub base_url: Option<String>,
    /// Overwrite existing schema and tools output.
    #[arg(long)]
    pub force: bool,
    /// Keep polling an OpenAPI source and re-sync when it changes.
    #[arg(long)]
    pub watch: bool,
    /// Polling interval for `--watch`, in seconds.
    #[arg(long, default_value_t = 2)]
    pub watch_interval_secs: u64,
    /// Run `appctl doctor --write` after a successful sync.
    #[arg(long)]
    pub doctor_write: bool,
    /// Login page URL for `--url` sync.
    #[arg(long)]
    pub login_url: Option<String>,
    /// Login username for `--url` sync.
    #[arg(long)]
    pub login_user: Option<String>,
    /// Login password for `--url` sync.
    #[arg(long)]
    pub login_password: Option<String>,
    /// CSS selector used to target the login form during `--url` sync.
    #[arg(long)]
    pub login_form_selector: Option<String>,
    /// `sync --db` (Postgres): only these schemas. Repeat the flag for each. Empty = all non-system.
    #[arg(long = "db-schema", value_name = "SCHEMA", action = clap::ArgAction::Append)]
    pub db_schemas: Vec<String>,
    /// Exclude a table: `name` (any schema) or `schema.table`
    #[arg(long = "db-exclude", value_name = "PATTERN", action = clap::ArgAction::Append)]
    pub db_exclude: Vec<String>,
    /// Opt-in: skip `__EFMigrationsHistory` and `spatial_ref_sys` from tools
    #[arg(long = "db-skip-infra")]
    pub db_skip_infra: bool,
}

#[derive(Debug, Args)]
pub struct ChatArgs {
    #[arg(long)]
    pub provider: Option<String>,
    #[arg(long)]
    pub model: Option<String>,
    /// Human-readable session label for history and the web UI.
    #[arg(long)]
    pub session: Option<String>,
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
    /// Emit machine-readable JSON instead of the terminal renderer.
    #[arg(long)]
    pub json: bool,
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
    /// TCP port. Use `0` to let the OS choose a free port (printed in the listening line).
    #[arg(long, default_value_t = 4242)]
    #[arg(env = "APPCTL_PORT")]
    pub port: u16,
    #[arg(long, default_value = "127.0.0.1")]
    #[arg(env = "APPCTL_BIND")]
    pub bind: String,
    /// Require this token on HTTP and WebSocket requests.
    #[arg(long)]
    pub token: Option<String>,
    /// Request header used to tag callers in the activity log.
    #[arg(long, default_value = "x-appctl-client-id")]
    pub identity_header: String,
    /// Start a local `cloudflared` tunnel for this server.
    #[arg(long)]
    pub tunnel: bool,
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
    /// Explicitly open the default web browser to the local UI (this is the default).
    #[arg(long, conflicts_with = "no_open")]
    pub open: bool,
    /// Do not open the default web browser to the local UI when the server is ready.
    #[arg(long)]
    pub no_open: bool,
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
    ProviderSample {
        #[arg(long)]
        preset: Option<String>,
    },
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

#[derive(Debug, Args)]
pub struct McpArgs {
    #[command(subcommand)]
    pub command: McpSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum McpSubcommand {
    Serve {
        #[arg(long)]
        read_only: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        strict: bool,
        #[arg(long, default_value_t = true)]
        confirm: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum PluginSubcommand {
    List,
    Install { name: String },
}

impl Cli {
    pub async fn run(self) -> Result<()> {
        let Cli {
            command,
            app_dir,
            log_level,
        } = self;
        init_tracing(&log_level)?;

        match command {
            Command::Setup => {
                let paths = resolve_init_paths(app_dir.as_ref())?;
                run_setup(&paths).await?;
            }
            Command::Init => {
                let paths = resolve_init_paths(app_dir.as_ref())?;
                run_init(&paths).await?;
            }
            Command::App(args) => {
                run_app_command(app_dir.as_ref(), args.command).await?;
            }
            Command::Sync(args) => {
                let paths = resolve_init_paths(app_dir.as_ref())?;
                if let Some(name) = args.plugin.as_deref() {
                    if args.watch {
                        bail!("`appctl sync --watch` is not supported for dynamic plugins yet");
                    }
                    run_dynamic_sync(paths, name, args.base_url.as_deref())?;
                } else {
                    let request = SyncRequest {
                        openapi: args.openapi,
                        django: args.django,
                        flask: args.flask,
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
                        watch: args.watch,
                        watch_interval_secs: args.watch_interval_secs,
                        doctor_write: args.doctor_write,
                        login_url: args.login_url,
                        login_user: args.login_user,
                        login_password: args.login_password,
                        login_form_selector: args.login_form_selector,
                        db_schemas: args.db_schemas,
                        db_exclude: args.db_exclude,
                        db_skip_infra: args.db_skip_infra,
                    };
                    run_sync(paths, request).await?;
                }
            }
            Command::Chat(args) => {
                let app = resolve_runtime_app_context(app_dir.as_ref())?;
                let config = AppConfig::load_for_runtime(&app.paths, "chat")?;
                run_chat(
                    &app.paths,
                    &config,
                    &app.app_name,
                    ChatOptions {
                        provider: args.provider,
                        model: args.model,
                        session: args.session,
                        read_only: args.read_only,
                        dry_run: args.dry_run,
                        confirm: args.confirm,
                        strict: args.strict,
                        context_note: app.context_note,
                    },
                )
                .await?;
            }
            Command::Run(args) => {
                let app = resolve_runtime_app_context(app_dir.as_ref())?;
                let config = AppConfig::load_for_runtime(&app.paths, "run")?;
                run_once(
                    &app.paths,
                    &config,
                    &app.app_name,
                    RunOptions {
                        prompt: args.prompt,
                        provider: args.provider,
                        model: args.model,
                        json: args.json,
                        read_only: args.read_only,
                        dry_run: args.dry_run,
                        confirm: args.confirm,
                        strict: args.strict,
                        context_note: app.context_note,
                    },
                )
                .await?;
            }
            Command::Doctor(args) => match args.command {
                Some(DoctorSubcommand::Models { provider }) => {
                    let app = resolve_runtime_app_context(app_dir.as_ref())?;
                    let config = AppConfig::load_for_runtime(&app.paths, "doctor models")?;
                    run_doctor_models(&app.paths, &config, provider.as_deref()).await?;
                }
                None => {
                    let app = resolve_runtime_app_context(app_dir.as_ref())?;
                    run_doctor(
                        &app.paths,
                        DoctorRunArgs {
                            write: args.write,
                            timeout_secs: args.timeout_secs,
                        },
                    )
                    .await?;
                }
            },
            Command::History(args) => {
                let app = resolve_runtime_app_context(app_dir.as_ref())?;
                run_history_command(
                    &app.paths,
                    HistoryCommand {
                        last: args.last,
                        undo: args.undo,
                    },
                )
                .await?;
            }
            Command::Serve(args) => {
                let app = resolve_runtime_app_context(app_dir.as_ref())?;
                let config = AppConfig::load_for_runtime(&app.paths, "serve")?;
                run_server(
                    app.app_name,
                    app.paths,
                    config,
                    ServeOptions {
                        port: args.port,
                        bind: args.bind,
                        token: args.token,
                        identity_header: args.identity_header,
                        tunnel: args.tunnel,
                        provider: args.provider,
                        model: args.model,
                        strict: args.strict,
                        read_only: args.read_only,
                        dry_run: args.dry_run,
                        confirm: args.confirm,
                        open_browser: args.open || !args.no_open,
                    },
                )
                .await?;
            }
            Command::Config(args) => match args.command {
                ConfigSubcommand::Init => {
                    let paths = resolve_init_paths(app_dir.as_ref())?;
                    run_init(&paths).await?;
                }
                ConfigSubcommand::Show => {
                    let app = resolve_runtime_app_context(app_dir.as_ref())?;
                    let config = AppConfig::load_or_init(&app.paths)?;
                    println!("{}", toml::to_string_pretty(&config)?);
                }
                ConfigSubcommand::ProviderSample { preset } => {
                    println!("{}", provider_sample_toml(preset.as_deref())?);
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
                    login_target_auth(
                        &provider,
                        client_id,
                        client_secret,
                        auth_url,
                        token_url,
                        scope,
                        redirect_port,
                    )
                    .await?;
                }
                AuthSubcommand::Status { provider } => {
                    print_target_auth_status(&provider);
                }
                AuthSubcommand::Target { command } => match command {
                    TargetAuthSubcommand::Login {
                        provider,
                        client_id,
                        client_secret,
                        auth_url,
                        token_url,
                        scope,
                        redirect_port,
                    } => {
                        login_target_auth(
                            &provider,
                            client_id,
                            client_secret,
                            auth_url,
                            token_url,
                            scope,
                            redirect_port,
                        )
                        .await?;
                    }
                    TargetAuthSubcommand::Status { provider } => {
                        print_target_auth_status(&provider);
                    }
                    TargetAuthSubcommand::Logout { provider } => {
                        logout_target_auth(&provider)?;
                    }
                },
                AuthSubcommand::Provider { command } => match command {
                    ProviderAuthSubcommand::Login {
                        provider,
                        profile,
                        value,
                        client_id,
                        client_secret,
                        auth_url,
                        token_url,
                        scope,
                        redirect_port,
                    } => {
                        let app = resolve_runtime_app_context(app_dir.as_ref())?;
                        let config = AppConfig::load_or_init(&app.paths)?;
                        login_provider_auth(
                            &config,
                            &provider,
                            ProviderLoginRequest {
                                profile,
                                value,
                                client_id,
                                client_secret,
                                auth_url,
                                token_url,
                                scope,
                                redirect_port,
                            },
                        )
                        .await?;
                    }
                    ProviderAuthSubcommand::Status { provider } => {
                        let app = resolve_runtime_app_context(app_dir.as_ref())?;
                        let config = AppConfig::load_or_init(&app.paths)?;
                        print_provider_auth_status(&app.paths, &config, provider.as_deref())?;
                    }
                    ProviderAuthSubcommand::Logout { provider } => {
                        let app = resolve_runtime_app_context(app_dir.as_ref())?;
                        let config = AppConfig::load_or_init(&app.paths)?;
                        logout_provider_auth(&config, &provider)?;
                    }
                    ProviderAuthSubcommand::List => {
                        let app = resolve_runtime_app_context(app_dir.as_ref())?;
                        let config = AppConfig::load_or_init(&app.paths)?;
                        print_provider_auth_status(&app.paths, &config, None)?;
                    }
                },
            },
            Command::Mcp(args) => match args.command {
                McpSubcommand::Serve {
                    read_only,
                    dry_run,
                    strict,
                    confirm,
                } => {
                    let app = resolve_runtime_app_context(app_dir.as_ref())?;
                    run_mcp_server(
                        app.paths,
                        McpServeOptions {
                            read_only,
                            dry_run,
                            strict,
                            confirm,
                        },
                    )
                    .await?;
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

async fn login_target_auth(
    provider: &str,
    client_id: Option<String>,
    client_secret: Option<String>,
    auth_url: Option<String>,
    token_url: Option<String>,
    scope: Vec<String>,
    redirect_port: u16,
) -> Result<()> {
    let client_id = client_id
        .or_else(|| std::env::var(format!("{provider}_CLIENT_ID")).ok())
        .context("--client-id is required (or set <provider>_CLIENT_ID)")?;
    let auth_url =
        auth_url.context("--auth-url is required (the provider's authorization endpoint)")?;
    let token_url = token_url.context("--token-url is required (the provider's token endpoint)")?;
    let config = OAuthLoginConfig {
        provider: provider.to_string(),
        storage_key: provider.to_string(),
        namespace: OAuthTokenNamespace::Target,
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
        "Logged in for target provider '{}'. Access token stored in keychain ({} scopes).",
        provider,
        tokens.scopes.len()
    );
    Ok(())
}

async fn login_provider_auth(
    config: &AppConfig,
    provider_name: &str,
    request: ProviderLoginRequest,
) -> Result<()> {
    let ProviderLoginRequest {
        profile,
        value,
        client_id,
        client_secret,
        auth_url,
        token_url,
        scope,
        redirect_port,
    } = request;
    let provider = config
        .providers
        .iter()
        .find(|provider| provider.name == provider_name);

    let auth = provider
        .and_then(|provider| provider.auth.clone())
        .or_else(|| provider_auth_preset(provider_name));

    match auth {
        Some(ProviderAuthConfig::None) => {
            println!(
                "provider '{}' does not require credentials; nothing to log in",
                provider_name
            );
            Ok(())
        }
        Some(ProviderAuthConfig::ApiKey { secret_ref, .. }) => {
            let secret = match value {
                Some(value) => value,
                None => dialoguer::Password::new()
                    .with_prompt(format!("Enter API key for `{provider_name}`"))
                    .interact()?,
            };
            crate::config::save_secret(&secret_ref, &secret)?;
            println!(
                "stored provider secret for '{}' in keychain under '{}'",
                provider_name, secret_ref
            );
            Ok(())
        }
        Some(ProviderAuthConfig::OAuth2 {
            profile: configured_profile,
            scopes,
            client_id_ref,
            client_secret_ref,
            auth_url: configured_auth_url,
            token_url: configured_token_url,
        }) => {
            let storage_key = profile.unwrap_or(configured_profile);
            let requested_scopes = if scope.is_empty() { scopes } else { scope };
            let client_id = client_id
                .or_else(|| client_id_ref.as_deref().and_then(|name| std::env::var(name).ok()))
                .or_else(|| client_id_ref.as_deref().and_then(|name| load_secret(name).ok()))
                .or_else(|| {
                    if provider_name == "gemini" {
                        std::env::var("GOOGLE_CLIENT_ID")
                            .ok()
                            .or_else(|| load_secret("GOOGLE_CLIENT_ID").ok())
                    } else {
                        None
                    }
                })
                .context("provider auth is missing a client id; set it in the auth block, set GOOGLE_CLIENT_ID, or pass --client-id")?;
            let client_secret = client_secret
                .or_else(|| {
                    client_secret_ref
                        .as_deref()
                        .and_then(|name| std::env::var(name).ok())
                })
                .or_else(|| {
                    client_secret_ref
                        .as_deref()
                        .and_then(|name| load_secret(name).ok())
                })
                .or_else(|| {
                    if provider_name == "gemini" {
                        std::env::var("GOOGLE_CLIENT_SECRET")
                            .ok()
                            .or_else(|| load_secret("GOOGLE_CLIENT_SECRET").ok())
                    } else {
                        None
                    }
                });
            let auth_url = auth_url.or(configured_auth_url).context(
                "provider auth is missing auth_url; set it in the auth block or pass --auth-url",
            )?;
            let token_url = token_url.or(configured_token_url).context(
                "provider auth is missing token_url; set it in the auth block or pass --token-url",
            )?;

            let login = OAuthLoginConfig {
                provider: provider_name.to_string(),
                storage_key: storage_key.clone(),
                namespace: OAuthTokenNamespace::Provider,
                client_id,
                client_secret,
                auth_url,
                token_url,
                scopes: requested_scopes,
                redirect_port,
            };
            let tokens = oauth_login(login).await?;
            println!(
                "Logged in provider '{}' using profile '{}'. Stored {} scope entries.",
                provider_name,
                storage_key,
                tokens.scopes.len()
            );
            Ok(())
        }
        Some(ProviderAuthConfig::GoogleAdc { .. }) => {
            let status = config
                .provider_statuses()
                .into_iter()
                .find(|provider| provider.name == provider_name)
                .map(|provider| provider.auth_status)
                .context("provider not found while checking ADC status")?;
            if status.configured {
                println!(
                    "provider '{}' can use Google ADC{}",
                    provider_name,
                    status
                        .project_id
                        .as_deref()
                        .map(|project| format!(" (project {project})"))
                        .unwrap_or_default()
                );
                Ok(())
            } else {
                bail!(
                    "{}",
                    status.recovery_hint.unwrap_or_else(|| {
                        "Google ADC is not configured for this provider.".to_string()
                    })
                )
            }
        }
        Some(ProviderAuthConfig::QwenOAuth { profile }) => {
            if crate::auth::oauth::load_provider_tokens(&profile).is_some() {
                println!(
                    "provider '{}' already has Qwen OAuth tokens for profile '{}'",
                    provider_name, profile
                );
                Ok(())
            } else {
                bail!(
                    "Qwen OAuth is not wired into `appctl auth provider login` yet. Use a DashScope API key or configure the Qwen Code MCP bridge through `appctl init`."
                )
            }
        }
        Some(ProviderAuthConfig::AzureAd {
            tenant,
            client_id: configured_client_id,
            ..
        }) => {
            let client_id = client_id.unwrap_or(configured_client_id);
            let scope = if scope.is_empty() {
                AZURE_OPENAI_DEFAULT_SCOPE.to_string()
            } else {
                scope.join(" ")
            };
            let storage_key = format!("azure-ad::{tenant}::{client_id}");
            let tokens = device_code_login(AzureAdDeviceConfig {
                tenant,
                client_id,
                scope,
                storage_key,
                authority_base: None,
                suppress_browser: false,
            })
            .await?;
            println!(
                "Logged in provider '{}' via Azure AD device code. Stored {} scope entries.",
                provider_name,
                tokens.scopes.len()
            );
            Ok(())
        }
        Some(ProviderAuthConfig::McpBridge { client }) => {
            println!(
                "provider '{}' uses the {} MCP bridge. Launch `{}` instead of authenticating through `appctl auth provider login`.",
                provider_name,
                client.display_name(),
                client.binary_name()
            );
            Ok(())
        }
        None => bail!(
            "provider '{}' is not configured and has no built-in auth preset",
            provider_name
        ),
    }
}

fn print_target_auth_status(provider: &str) {
    match load_secret(&format!("appctl_oauth::{provider}")) {
        Ok(raw) if !raw.is_empty() => {
            println!(
                "target auth '{}' has stored OAuth tokens ({} bytes)",
                provider,
                raw.len()
            );
        }
        _ => println!("no target OAuth tokens stored for '{}'", provider),
    }
}

fn logout_target_auth(provider: &str) -> Result<()> {
    let key = format!("appctl_oauth::{provider}");
    match delete_secret(&key) {
        Ok(()) => {
            println!("cleared target OAuth tokens for '{provider}'");
            Ok(())
        }
        Err(err) => {
            println!("no target OAuth tokens cleared for '{provider}' ({err:#})");
            Ok(())
        }
    }
}

fn print_provider_auth_status(
    paths: &ConfigPaths,
    config: &AppConfig,
    provider_name: Option<&str>,
) -> Result<()> {
    let statuses = config.provider_statuses_with_paths(paths);
    if let Some(provider_name) = provider_name {
        let provider = statuses
            .into_iter()
            .find(|provider| provider.name == provider_name)
            .with_context(|| format!("provider '{}' not found in config", provider_name))?;
        print_single_provider_status(&provider);
        return Ok(());
    }

    for provider in statuses {
        print_single_provider_status(&provider);
    }
    Ok(())
}

fn print_single_provider_status(provider: &crate::config::ResolvedProviderSummary) {
    println!(
        "{} kind={} model={} auth={} configured={}",
        provider.name,
        provider_kind_label(provider.kind),
        provider.model,
        provider_auth_kind_label(&provider.auth_status.kind),
        provider.auth_status.configured
    );
    if let Some(profile) = &provider.auth_status.profile {
        println!("  profile: {profile}");
    }
    if let Some(secret_ref) = &provider.auth_status.secret_ref {
        println!("  secret_ref: {secret_ref}");
    }
    if let Some(expires_at) = provider.auth_status.expires_at {
        println!("  expires_at: {expires_at}");
    }
    if let Some(project_id) = &provider.auth_status.project_id {
        println!("  project_id: {project_id}");
    }
    if let Some(recovery_hint) = &provider.auth_status.recovery_hint {
        println!("  hint: {recovery_hint}");
    }
}

fn provider_kind_label(kind: crate::config::ProviderKind) -> &'static str {
    match kind {
        crate::config::ProviderKind::Anthropic => "anthropic",
        crate::config::ProviderKind::OpenAiCompatible => "open_ai_compatible",
        crate::config::ProviderKind::GoogleGenai => "google_genai",
        crate::config::ProviderKind::Vertex => "vertex",
        crate::config::ProviderKind::AzureOpenAi => "azure_open_ai",
    }
}

fn provider_auth_kind_label(kind: &ProviderAuthKind) -> &'static str {
    match kind {
        ProviderAuthKind::None => "none",
        ProviderAuthKind::ApiKey => "api_key",
        ProviderAuthKind::GoogleAdc => "google_adc",
        ProviderAuthKind::QwenOAuth => "qwen_oauth",
        ProviderAuthKind::AzureAd => "azure_ad",
        ProviderAuthKind::McpBridge => "mcp_bridge",
        ProviderAuthKind::OAuth2 => "oauth2",
    }
}

fn logout_provider_auth(config: &AppConfig, provider_name: &str) -> Result<()> {
    let provider = config
        .providers
        .iter()
        .find(|provider| provider.name == provider_name)
        .with_context(|| format!("provider '{}' not found in config", provider_name))?;
    let auth = provider
        .auth
        .clone()
        .or_else(|| provider_auth_preset(provider_name))
        .with_context(|| format!("provider '{}' has no auth configuration", provider_name))?;

    match auth {
        ProviderAuthConfig::None => {
            println!(
                "provider '{}' does not store credentials in appctl",
                provider_name
            );
            Ok(())
        }
        ProviderAuthConfig::ApiKey { secret_ref, .. } => {
            if load_secret(&secret_ref).is_ok() {
                delete_secret(&secret_ref)?;
                println!(
                    "deleted provider secret for '{}' from keychain entry '{}'",
                    provider_name, secret_ref
                );
            } else {
                println!(
                    "no keychain secret stored for '{}' under '{}'",
                    provider_name, secret_ref
                );
            }
            Ok(())
        }
        ProviderAuthConfig::OAuth2 { profile, .. } | ProviderAuthConfig::QwenOAuth { profile } => {
            if crate::auth::oauth::load_provider_tokens(&profile).is_some() {
                delete_provider_tokens(&profile)?;
                println!(
                    "deleted provider auth tokens for '{}' (profile '{}')",
                    provider_name, profile
                );
            } else {
                println!(
                    "no stored provider auth tokens found for '{}' (profile '{}')",
                    provider_name, profile
                );
            }
            Ok(())
        }
        ProviderAuthConfig::AzureAd {
            tenant, client_id, ..
        } => {
            let storage_key = format!("azure-ad::{tenant}::{client_id}");
            if crate::auth::oauth::load_provider_tokens(&storage_key).is_some() {
                delete_provider_tokens(&storage_key)?;
                println!(
                    "deleted Azure AD tokens for '{}' (profile '{}')",
                    provider_name, storage_key
                );
            } else {
                println!(
                    "no stored Azure AD tokens found for '{}' (profile '{}')",
                    provider_name, storage_key
                );
            }
            Ok(())
        }
        ProviderAuthConfig::GoogleAdc { .. } => {
            println!(
                "provider '{}' uses Google ADC. appctl does not own those credentials; run `gcloud auth application-default revoke` if you want to clear them.",
                provider_name
            );
            Ok(())
        }
        ProviderAuthConfig::McpBridge { client } => {
            println!(
                "provider '{}' uses the {} MCP bridge. appctl does not store credentials for it.",
                provider_name,
                client.display_name()
            );
            Ok(())
        }
    }
}

fn provider_sample_toml(preset: Option<&str>) -> Result<String> {
    let preset = preset.unwrap_or("default");
    let sample = match preset {
        "gemini" => {
            r#"default = "gemini"

[[provider]]
name = "gemini"
kind = "google_genai"
base_url = "https://generativelanguage.googleapis.com"
model = "gemini-2.5-pro"
auth = { kind = "oauth2", profile = "gemini-default", scopes = ["https://www.googleapis.com/auth/generative-language"] }
"#
        }
        "vertex" => {
            r#"default = "vertex"

[[provider]]
name = "vertex"
kind = "vertex"
base_url = "https://us-central1-aiplatform.googleapis.com"
model = "gemini-2.5-pro"
auth = { kind = "google_adc", project = "your-gcp-project" }
extra_headers = { x-appctl-vertex-region = "us-central1" }
"#
        }
        "qwen" => {
            r#"default = "qwen"

[[provider]]
name = "qwen"
kind = "open_ai_compatible"
base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"
model = "qwen3-coder-plus"
auth = { kind = "api_key", secret_ref = "DASHSCOPE_API_KEY" }
"#
        }
        "claude" => {
            r#"default = "claude"

[[provider]]
name = "claude"
kind = "anthropic"
base_url = "https://api.anthropic.com"
model = "claude-sonnet-4"
auth = { kind = "api_key", secret_ref = "ANTHROPIC_API_KEY" }
"#
        }
        "openai" => {
            r#"default = "openai"

[[provider]]
name = "openai"
kind = "open_ai_compatible"
base_url = "https://api.openai.com/v1"
model = "gpt-5"
auth = { kind = "api_key", secret_ref = "OPENAI_API_KEY" }
"#
        }
        "ollama" => {
            r#"default = "ollama"

[[provider]]
name = "ollama"
kind = "open_ai_compatible"
base_url = "http://localhost:11434/v1"
model = "llama3.1"
auth = { kind = "none" }
"#
        }
        "default" => return AppConfig::sample_toml(),
        other => bail!("unknown preset '{}'", other),
    };
    Ok(sample.to_string())
}

fn provider_auth_preset(provider_name: &str) -> Option<ProviderAuthConfig> {
    match provider_name {
        "gemini" => Some(ProviderAuthConfig::OAuth2 {
            profile: "gemini-default".to_string(),
            scopes: vec!["https://www.googleapis.com/auth/generative-language".to_string()],
            client_id_ref: Some("GOOGLE_CLIENT_ID".to_string()),
            client_secret_ref: Some("GOOGLE_CLIENT_SECRET".to_string()),
            auth_url: Some("https://accounts.google.com/o/oauth2/v2/auth".to_string()),
            token_url: Some("https://oauth2.googleapis.com/token".to_string()),
        }),
        "qwen" => Some(ProviderAuthConfig::ApiKey {
            secret_ref: "DASHSCOPE_API_KEY".to_string(),
            help_url: None,
        }),
        "claude" => Some(ProviderAuthConfig::ApiKey {
            secret_ref: "ANTHROPIC_API_KEY".to_string(),
            help_url: None,
        }),
        "openai" => Some(ProviderAuthConfig::ApiKey {
            secret_ref: "OPENAI_API_KEY".to_string(),
            help_url: None,
        }),
        "vertex" => Some(ProviderAuthConfig::GoogleAdc { project: None }),
        "ollama" => Some(ProviderAuthConfig::None),
        _ => None,
    }
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

#[derive(Debug, Clone)]
struct ResolvedAppContext {
    app_name: String,
    paths: ConfigPaths,
    /// Shown in the chat banner when the session is not the global active app, or from `--app-dir`.
    context_note: Option<String>,
}

async fn run_app_command(app_dir_override: Option<&PathBuf>, command: AppSubcommand) -> Result<()> {
    use crate::term::{
        print_flow_header, print_section_title, print_status_error, print_status_success, print_tip,
    };

    let mut registry = AppRegistry::load_or_default()?;

    match command {
        AppSubcommand::Add {
            path,
            openapi,
            base_url,
            auth_header,
            force,
            display_name,
            description,
            name,
        } => {
            let app_dir = path
                .map(|p| {
                    std::fs::canonicalize(&p)
                        .with_context(|| format!("failed to canonicalize {}", p.display()))
                })
                .or_else(|| resolve_local_app_dir(app_dir_override).transpose())
                .unwrap_or_else(|| {
                    default_app_dir()
                        .and_then(|path| std::fs::canonicalize(&path).or(Ok(path)))
                        .with_context(|| "failed to resolve app directory".to_string())
                })?;

            if !app_dir.exists() {
                bail!(
                    "app directory {} does not exist — run `appctl init` first",
                    app_dir.display()
                );
            }

            let default_registry = app_name_from_dir(&app_dir);
            print_flow_header("app add", Some("Register an app and set it active"));
            print_tip(
                "Put `--display-name` / `--description` before the registry name, e.g. `appctl app add --display-name \"OrderHub\" ordering`.",
            );
            print_tip(
                "Registry name: used in `appctl app use`, `app list`, and the chat title. It is not your server URL.",
            );
            if registry_default_looks_like_os_username(&default_registry, &app_dir) {
                print_tip(
                    "Default matches your home folder name (common for ~/.appctl). Pick a name that describes this app if the default is confusing.",
                );
            }

            let chosen: String = match name {
                Some(n) => n,
                None if std::io::stdin().is_terminal() => {
                    use dialoguer::Input;
                    Input::new()
                        .with_prompt("Registry name for this app")
                        .default(default_registry.clone())
                        .interact_text()
                        .context("failed to read registry name")?
                }
                None => default_registry,
            };

            registry.register_and_activate(chosen.clone(), app_dir.clone());
            registry.save()?;
            print_status_success(&format!("Registered '{}' -> {}", chosen, app_dir.display()));
            print_tip("Use `appctl app use <name>` later to switch the global active app.");
            if let Some(source) = openapi {
                print_tip("Syncing OpenAPI source immediately after registration.");
                run_sync(
                    ConfigPaths::new(app_dir.clone()),
                    SyncRequest {
                        openapi: Some(source),
                        auth_header,
                        base_url,
                        force,
                        ..SyncRequest::default()
                    },
                )
                .await?;
            }
            if display_name.is_some() || description.is_some() {
                let paths = ConfigPaths::new(app_dir);
                let mut cfg = AppConfig::load_for_runtime(&paths, "app add")?;
                if let Some(ref dn) = display_name {
                    let d = dn.trim();
                    if !d.is_empty() {
                        cfg.display_name = Some(d.to_string());
                    }
                }
                if let Some(ref desc) = description {
                    let t = desc.trim();
                    cfg.description = if t.is_empty() {
                        None
                    } else {
                        Some(t.to_string())
                    };
                }
                cfg.save(&paths)?;
                print_status_success(&format!(
                    "Updated app metadata in {}",
                    paths.config.display()
                ));
            }
        }
        AppSubcommand::List => {
            print_flow_header(
                "app list",
                Some("Global app contexts (~/.appctl/apps.toml)"),
            );
            if registry.apps.is_empty() {
                print_tip(
                    "No apps registered yet. Run `appctl app add` in an `.appctl` directory.",
                );
                return Ok(());
            }
            let active = registry.active.clone();
            print_section_title("Registered apps");
            for (name, path) in &registry.apps {
                let marker = if active.as_deref() == Some(name) {
                    "*"
                } else {
                    " "
                };
                println!("  {marker} {name} -> {}", path.display());
            }
        }
        AppSubcommand::Use { name } => {
            if !registry.apps.contains_key(&name) {
                print_status_error(&format!(
                    "No registered app named '{name}'. Run `appctl app list` to see known apps."
                ));
                bail!("unknown app '{}'", name);
            }
            registry.active = Some(name.clone());
            registry.save()?;
            print_status_success(&format!("Active app set to '{name}'"));
        }
        AppSubcommand::Remove { name } => match registry.remove(&name) {
            Some(path) => {
                registry.save()?;
                print_status_success(&format!(
                    "Removed '{name}' (directory untouched: {})",
                    path.display()
                ));
            }
            None => {
                print_status_error(&format!("No registered app named '{name}'"));
                bail!("unknown app '{}'", name);
            }
        },
    }

    Ok(())
}

fn resolve_runtime_app_context(app_dir_override: Option<&PathBuf>) -> Result<ResolvedAppContext> {
    if let Some(path) = app_dir_override {
        return app_context_from_path(
            normalize_app_dir(path),
            Some("Use `appctl init` to create it."),
            AppContextFrom::CliArg,
        );
    }

    if let Some(path) = resolve_local_app_dir(None)? {
        return app_context_from_path(path, None, AppContextFrom::LocalCwd);
    }

    let registry = AppRegistry::load_or_default()?;
    if let Some((name, path)) = active_app_path(&registry) {
        if !path.exists() {
            bail!(
                "Active app '{}' points to {} but that directory no longer exists. Run `appctl app list` and `appctl app remove {}` or re-register it.",
                name,
                path.display(),
                name
            );
        }
        return Ok(ResolvedAppContext {
            app_name: name,
            paths: ConfigPaths::new(path),
            context_note: None,
        });
    }

    bail!(
        "No app context found.\nRun this next: appctl setup\nAdvanced: use `appctl app add` / `appctl app use <name>` to select a global app, or pass `--app-dir <path>`."
    )
}

#[derive(Debug, Clone, Copy)]
enum AppContextFrom {
    /// `appctl --app-dir=...` was set.
    CliArg,
    /// A `.appctl` was found walking up from the current working directory.
    LocalCwd,
}

fn app_context_from_path(
    path: PathBuf,
    not_found_hint: Option<&str>,
    from: AppContextFrom,
) -> Result<ResolvedAppContext> {
    if !path.exists() {
        let hint = not_found_hint.unwrap_or("Run `appctl init` or pick a different app context.");
        bail!("App directory {} does not exist. {}", path.display(), hint);
    }

    let registry = AppRegistry::load_or_default()?;
    let app_name =
        find_registered_app_name(&registry, &path).unwrap_or_else(|| app_name_from_dir(&path));

    let context_note = match from {
        AppContextFrom::CliArg => Some(
            "Context: --app-dir (ignores cwd and the global `app use` app for this command.)"
                .to_string(),
        ),
        AppContextFrom::LocalCwd => {
            if let Some((gname, gpath)) = active_app_path(&registry) {
                if normalize_app_dir(&path) != normalize_app_dir(&gpath) {
                    Some(format!(
                        "Not the global `app use` app (that is \"{gname}\" in `app list`). A .appctl in this tree wins. To use that app instead: appctl --app-dir {} chat",
                        gpath.display()
                    ))
                } else {
                    None
                }
            } else {
                None
            }
        }
    };

    Ok(ResolvedAppContext {
        app_name,
        paths: ConfigPaths::new(path),
        context_note,
    })
}

fn resolve_init_paths(app_dir_override: Option<&PathBuf>) -> Result<ConfigPaths> {
    if let Some(path) = app_dir_override {
        return Ok(ConfigPaths::new(normalize_app_dir(path)));
    }

    if let Some(path) = resolve_local_app_dir(None)? {
        return Ok(ConfigPaths::new(path));
    }

    Ok(ConfigPaths::new(default_app_dir()?))
}

fn resolve_local_app_dir(app_dir_override: Option<&PathBuf>) -> Result<Option<PathBuf>> {
    if let Some(path) = app_dir_override {
        return Ok(Some(normalize_app_dir(path)));
    }

    let cwd = std::env::current_dir().context("failed to read current working directory")?;
    Ok(find_app_dir_from(&cwd))
}

fn default_app_dir() -> Result<PathBuf> {
    Ok(std::env::current_dir()
        .context("failed to read current working directory")?
        .join(".appctl"))
}
