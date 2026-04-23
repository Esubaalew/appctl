use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use dialoguer::{Confirm, FuzzySelect, Input, Password, Select};
use serde_json::Value;

use crate::{
    auth::{
        gcloud,
        mcp_bridge::install_bridge,
        provider::{McpBridgeClient, ProviderAuthConfig},
        verify::verify_provider,
    },
    config::{AppConfig, ConfigPaths, ProviderConfig, ProviderKind, save_secret},
    term::{
        print_flow_header, print_section_title, print_status_error, print_status_success, print_tip,
    },
};

pub async fn run_init(paths: &ConfigPaths) -> Result<()> {
    paths.ensure()?;

    print_flow_header(
        "init",
        Some("Configure provider, model, and secure storage for this app directory"),
    );

    let mut config = if paths.config.exists() {
        if Confirm::new()
            .with_prompt(format!(
                "{} already exists. Replace it instead of augmenting it?",
                paths.config.display()
            ))
            .default(false)
            .interact()?
        {
            AppConfig::default()
        } else {
            AppConfig::load(paths)?
        }
    } else {
        AppConfig::default()
    };

    let items = [
        "Vertex AI via Google ADC (real browser)",
        "Gemini API key",
        "OpenAI-compatible API (guided: OpenRouter, NVIDIA, custom)",
        "Local OpenAI-compatible (Ollama, LM Studio, vLLM, llama.cpp)",
        "Anthropic Claude API key",
        "Qwen DashScope API key",
        "Azure OpenAI API key",
        "OpenAI subscription via Codex MCP bridge",
        "Claude subscription via Claude Code MCP bridge",
        "Qwen subscription via Qwen Code MCP bridge",
        "Gemini subscription via Gemini CLI MCP bridge",
    ];
    let choice = Select::new()
        .with_prompt("Choose how appctl should talk to an AI provider")
        .items(items)
        .default(0)
        .interact()?;

    let (provider, next_step, direct_api) = match choice {
        0 => {
            let provider = configure_vertex_adc().await?;
            (provider, String::new(), true)
        }
        1 => (
            configure_api_key_interactive(ApiKeyProviderSpec {
                default_name: "gemini",
                kind: ProviderKind::GoogleGenai,
                default_base_url: "https://generativelanguage.googleapis.com",
                default_model: "gemini-2.5-pro",
                default_secret_ref: "GOOGLE_API_KEY",
                help_url: Some("https://aistudio.google.com/app/apikey"),
                prompt_extra_headers: false,
            })
            .await?,
            String::new(),
            true,
        ),
        2 => (
            configure_openai_compatible_api().await?,
            String::new(),
            true,
        ),
        3 => (
            configure_local_openai_compatible().await?,
            String::new(),
            true,
        ),
        4 => (
            configure_api_key_interactive(ApiKeyProviderSpec {
                default_name: "claude",
                kind: ProviderKind::Anthropic,
                default_base_url: "https://api.anthropic.com",
                default_model: "claude-sonnet-4",
                default_secret_ref: "ANTHROPIC_API_KEY",
                help_url: Some("https://console.anthropic.com/settings/keys"),
                prompt_extra_headers: false,
            })
            .await?,
            String::new(),
            true,
        ),
        5 => (
            configure_api_key_interactive(ApiKeyProviderSpec {
                default_name: "qwen",
                kind: ProviderKind::OpenAiCompatible,
                default_base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
                default_model: "qwen3-coder-plus",
                default_secret_ref: "DASHSCOPE_API_KEY",
                help_url: Some("https://bailian.console.aliyun.com/"),
                prompt_extra_headers: false,
            })
            .await?,
            String::new(),
            true,
        ),
        6 => (configure_azure_api_key()?, String::new(), true),
        7 => {
            let (provider, next_step) =
                configure_bridge_provider("openai-subscription", McpBridgeClient::Codex, paths)?;
            (provider, next_step, false)
        }
        8 => {
            let (provider, next_step) =
                configure_bridge_provider("claude-subscription", McpBridgeClient::Claude, paths)?;
            (provider, next_step, false)
        }
        9 => {
            let (provider, next_step) =
                configure_bridge_provider("qwen-subscription", McpBridgeClient::QwenCode, paths)?;
            (provider, next_step, false)
        }
        _ => {
            let (provider, next_step) =
                configure_bridge_provider("gemini-subscription", McpBridgeClient::Gemini, paths)?;
            (provider, next_step, false)
        }
    };

    let mut candidate = config.clone();
    upsert_provider(&mut candidate, provider);

    if direct_api {
        let resolved =
            candidate.resolve_provider_with_paths(Some(paths), Some(&candidate.default), None)?;
        match verify_provider(&resolved).await {
            Ok(()) => {
                config = candidate;
                if let Some(provider) = config
                    .providers
                    .iter_mut()
                    .find(|provider| provider.name == config.default)
                {
                    provider.verified = true;
                }
                config.save(paths)?;
                println!();
                print_section_title("Done");
                print_status_success("config saved");
                print_status_success("API key stored in keychain");
                print_status_success("connection verified");
                print_tip("Next:  appctl chat");
                print_tip(
                    "  or:  appctl run \"Summarize the synced app and suggest a safe first action.\"",
                );
            }
            Err(err) => {
                if let Some(provider) = candidate
                    .providers
                    .iter_mut()
                    .find(|provider| provider.name == candidate.default)
                {
                    provider.verified = false;
                }
                candidate.save(paths)?;
                println!();
                print_section_title("Done (with issues)");
                print_status_success("config saved");
                print_status_success("API key stored in keychain");
                print_status_error("connection not confirmed — provider marked as unverified");
                eprintln!("\n{err:#}\n");
                print_tip("Run `appctl auth provider login` after fixing quota/limits to verify.");
            }
        }
    } else {
        config = candidate;
        config.save(paths)?;
        print_section_title("Next step");
        println!("{}", next_step);
    }

    Ok(())
}

async fn configure_vertex_adc() -> Result<ProviderConfig> {
    let name: String = Input::new()
        .with_prompt("Provider name to save this as")
        .default("vertex".to_string())
        .interact_text()?;
    println!("Opening the real Google browser flow through gcloud ADC...");
    let detected_project = gcloud::detect_project();
    let token = gcloud::login_application_default(detected_project.as_deref())?;
    let project = prompt_project(token.project_id.clone())?
        .ok_or_else(|| anyhow::anyhow!("Vertex requires a Google Cloud project"))?;
    let region = prompt_vertex_region()?;
    let model: String = Input::new()
        .with_prompt("Vertex model id")
        .default("gemini-2.5-pro".to_string())
        .interact_text()?;
    let base_url = format!("https://{region}-aiplatform.googleapis.com");
    println!("Using Vertex base URL: {base_url}");

    let mut extra_headers = BTreeMap::new();
    extra_headers.insert("x-appctl-vertex-region".to_string(), region);
    Ok(ProviderConfig {
        name,
        kind: ProviderKind::Vertex,
        base_url,
        model,
        verified: true,
        auth: Some(ProviderAuthConfig::GoogleAdc {
            project: Some(project),
        }),
        api_key_ref: None,
        extra_headers,
    })
}

#[derive(Clone)]
struct ApiKeyProviderSpec {
    default_name: &'static str,
    kind: ProviderKind,
    default_base_url: &'static str,
    default_model: &'static str,
    default_secret_ref: &'static str,
    help_url: Option<&'static str>,
    prompt_extra_headers: bool,
}

#[derive(Clone, Debug)]
struct ModelOption {
    id: String,
    label: String,
    recommended: bool,
}

async fn configure_api_key_interactive(spec: ApiKeyProviderSpec) -> Result<ProviderConfig> {
    if let Some(help_url) = spec.help_url {
        print_tip(&format!("Get a real API key at: {help_url}"));
    }
    let name: String = Input::new()
        .with_prompt("Provider name to save this as")
        .default(spec.default_name.to_string())
        .interact_text()?;
    let base_url: String = Input::new()
        .with_prompt("API base URL")
        .default(spec.default_base_url.to_string())
        .interact_text()?;
    let secret_ref: String = Input::new()
        .with_prompt("Name for the key in the OS keychain")
        .default(spec.default_secret_ref.to_string())
        .interact_text()?;
    let secret = Password::new()
        .with_prompt(format!("Paste the API key for `{name}`"))
        .interact()?;
    if secret.trim().is_empty() {
        bail!("No API key provided");
    }
    save_secret(&secret_ref, &secret)?;
    let mut extra_headers = BTreeMap::new();
    if spec.prompt_extra_headers {
        prompt_extra_headers(&mut extra_headers)?;
    }
    let discovered =
        discover_models_for_api_provider(&spec, &base_url, &secret, &extra_headers).await;
    let model = select_or_prompt_model(&discovered, spec.default_model)?;
    Ok(ProviderConfig {
        name,
        kind: spec.kind,
        base_url,
        model,
        verified: true,
        auth: Some(ProviderAuthConfig::ApiKey {
            secret_ref,
            help_url: spec.help_url.map(str::to_string),
        }),
        api_key_ref: None,
        extra_headers,
    })
}

async fn configure_openai_compatible_api() -> Result<ProviderConfig> {
    let variants = [
        "OpenRouter",
        "NVIDIA NIM",
        "Custom OpenAI-compatible endpoint",
    ];
    let choice = Select::new()
        .with_prompt("Choose the compatible provider you want to start from")
        .items(variants)
        .default(0)
        .interact()?;

    let spec = match choice {
        0 => ApiKeyProviderSpec {
            default_name: "openrouter",
            kind: ProviderKind::OpenAiCompatible,
            default_base_url: "https://openrouter.ai/api/v1",
            default_model: "openai/gpt-4.1-mini",
            default_secret_ref: "OPENROUTER_API_KEY",
            help_url: Some("https://openrouter.ai/keys"),
            prompt_extra_headers: true,
        },
        1 => ApiKeyProviderSpec {
            default_name: "nvidia",
            kind: ProviderKind::OpenAiCompatible,
            default_base_url: "https://integrate.api.nvidia.com/v1",
            default_model: "meta/llama-3.1-70b-instruct",
            default_secret_ref: "NVIDIA_API_KEY",
            help_url: Some("https://build.nvidia.com/"),
            prompt_extra_headers: true,
        },
        _ => ApiKeyProviderSpec {
            default_name: "compatible",
            kind: ProviderKind::OpenAiCompatible,
            default_base_url: "https://api.example.com/v1",
            default_model: "your-model-id",
            default_secret_ref: "COMPATIBLE_API_KEY",
            help_url: None,
            prompt_extra_headers: true,
        },
    };

    print_tip(
        "This path covers OpenAI-compatible providers (OpenRouter, NVIDIA NIM, Together, Groq, gateways, or your own endpoint).",
    );
    configure_api_key_interactive(spec).await
}

/// OpenAI-compatible base is usually `http://host:port/v1`. Ollama’s tag listing uses the root.
fn ollama_native_base_url(openai_compatible_base: &str) -> String {
    let mut s = openai_compatible_base
        .trim()
        .trim_end_matches('/')
        .to_string();
    if s.to_ascii_lowercase().ends_with("/v1") {
        s.truncate(s.len() - 3);
        return s.trim_end_matches('/').to_string();
    }
    s
}

/// Returns sorted model names from `GET {ollama}/api/tags` (local pulls only).
async fn list_ollama_pulled_models(ollama_base: &str) -> Result<Vec<String>> {
    let url = format!("{}/api/tags", ollama_base.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(3))
        .build()
        .context("build reqwest client for Ollama model list")?;
    let response =
        client.get(&url).send().await.with_context(|| {
            format!("failed to reach Ollama at {url} (is `ollama serve` running?)")
        })?;
    if !response.status().is_success() {
        bail!(
            "Ollama returned {} from {} — check the server and base URL",
            response.status(),
            url
        );
    }
    let value: Value = response
        .json()
        .await
        .context("parse Ollama /api/tags JSON")?;
    let mut names = Vec::new();
    if let Some(models) = value.get("models").and_then(|m| m.as_array()) {
        for m in models {
            if let Some(name) = m.get("name").and_then(|n| n.as_str()) {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}

/// Standard OpenAI `GET {base}/v1/models` — supported by Ollama, LM Studio, many local stacks.
async fn list_models_openai_v1(base_v1_url: &str) -> Vec<String> {
    list_models_openai_v1_with_auth(base_v1_url, None, &BTreeMap::new()).await
}

async fn list_models_openai_v1_with_auth(
    base_v1_url: &str,
    api_key: Option<&str>,
    extra_headers: &BTreeMap<String, String>,
) -> Vec<String> {
    let url = format!("{}/models", base_v1_url.trim_end_matches('/'));
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut request = client.get(&url);
    if let Some(api_key) = api_key {
        request = request.bearer_auth(api_key);
    }
    for (name, value) in extra_headers {
        request = request.header(name, value);
    }
    let Ok(response) = request.send().await else {
        return Vec::new();
    };
    if !response.status().is_success() {
        return Vec::new();
    }
    let Ok(value) = response.json::<Value>().await else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    if let Some(data) = value.get("data").and_then(|d| d.as_array()) {
        for item in data {
            if let Some(id) = item.get("id").and_then(|x| x.as_str()) {
                ids.push(id.to_string());
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

async fn list_models_google_genai(base_url: &str, api_key: &str) -> Vec<String> {
    let url = format!(
        "{}/v1beta/models?key={}",
        base_url.trim_end_matches('/'),
        api_key
    );
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let Ok(response) = client.get(&url).send().await else {
        return Vec::new();
    };
    if !response.status().is_success() {
        return Vec::new();
    }
    let Ok(value) = response.json::<Value>().await else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    if let Some(models) = value.get("models").and_then(|m| m.as_array()) {
        for model in models {
            let supported = model
                .get("supportedGenerationMethods")
                .and_then(|m| m.as_array())
                .map(|methods| {
                    methods
                        .iter()
                        .any(|item| item.as_str() == Some("generateContent"))
                })
                .unwrap_or(false);
            if !supported {
                continue;
            }
            if let Some(name) = model.get("name").and_then(|n| n.as_str()) {
                ids.push(name.trim_start_matches("models/").to_string());
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

async fn list_models_anthropic(base_url: &str, api_key: &str) -> Vec<String> {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let Ok(response) = client
        .get(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
    else {
        return Vec::new();
    };
    if !response.status().is_success() {
        return Vec::new();
    }
    let Ok(value) = response.json::<Value>().await else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    if let Some(models) = value.get("data").and_then(|d| d.as_array()) {
        for model in models {
            if let Some(id) = model.get("id").and_then(|x| x.as_str()) {
                ids.push(id.to_string());
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

async fn discover_models_for_api_provider(
    spec: &ApiKeyProviderSpec,
    base_url: &str,
    secret: &str,
    extra_headers: &BTreeMap<String, String>,
) -> Vec<String> {
    match spec.kind {
        ProviderKind::OpenAiCompatible => {
            list_models_openai_v1_with_auth(base_url, Some(secret), extra_headers).await
        }
        ProviderKind::GoogleGenai => list_models_google_genai(base_url, secret).await,
        ProviderKind::Anthropic => list_models_anthropic(base_url, secret).await,
        _ => Vec::new(),
    }
}

fn select_or_prompt_model(discovered: &[String], default_model: &str) -> Result<String> {
    if discovered.is_empty() {
        return Ok(Input::new()
            .with_prompt("Model id")
            .default(default_model.to_string())
            .interact_text()?);
    }

    let mut options = build_model_options(discovered, default_model);
    let manual_idx = options.len();
    let recommended_idx = options.iter().position(|opt| opt.recommended).unwrap_or(0);
    let help = "Choose a model (type to filter, ↑↓ move, Enter to select)";
    print_tip(help);
    options.push(ModelOption {
        id: String::new(),
        label: "Other — type the model id manually".to_string(),
        recommended: false,
    });
    let labels = options
        .iter()
        .map(|opt| opt.label.as_str())
        .collect::<Vec<_>>();
    let pick = FuzzySelect::new()
        .with_prompt("Model")
        .items(&labels)
        .default(recommended_idx)
        .interact()?;
    if pick == manual_idx {
        prompt_model_id_non_empty(
            "Exact model id (must match the provider, e.g. from /models or provider docs)",
        )
    } else {
        Ok(options[pick].id.clone())
    }
}

fn build_model_options(discovered: &[String], default_model: &str) -> Vec<ModelOption> {
    let mut options = discovered
        .iter()
        .filter(|id| is_usable_chat_model(id))
        .map(|id| ModelOption {
            id: id.clone(),
            label: format_model_label(id, default_model),
            recommended: is_recommended_model(id, default_model),
        })
        .collect::<Vec<_>>();

    if options.is_empty() {
        options = discovered
            .iter()
            .map(|id| ModelOption {
                id: id.clone(),
                label: format_model_label(id, default_model),
                recommended: is_recommended_model(id, default_model),
            })
            .collect();
    }

    options.sort_by_key(|opt| {
        (
            !opt.recommended,
            opt.id.contains("preview") || opt.id.contains("exp"),
            opt.id.clone(),
        )
    });
    options
}

fn is_usable_chat_model(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let blocklist = [
        "robotics",
        "image",
        "tts",
        "clip",
        "video",
        "audio",
        "embedding",
        "banana",
        "lyria",
        "vision",
        "transcribe",
        "speech",
    ];
    if blocklist.iter().any(|bad| name.contains(bad)) {
        return false;
    }
    if name.contains("gemma-3-1b") || name.contains("gemma-3-2b") {
        return false;
    }
    true
}

fn is_recommended_model(name: &str, default_model: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower == default_model.to_ascii_lowercase() {
        return true;
    }
    lower == "gemini-2.5-flash"
        || lower == "gemini-2.5-flash-lite"
        || lower == "gpt-4.1-mini"
        || lower == "claude-sonnet-4"
}

fn format_model_label(name: &str, default_model: &str) -> String {
    let mut tags = Vec::new();
    let lower = name.to_ascii_lowercase();

    if is_recommended_model(name, default_model) {
        tags.push("recommended");
    }
    if lower.contains("preview") || lower.contains("experimental") || lower.contains("exp") {
        tags.push("preview");
    } else {
        tags.push("stable");
    }
    if lower.contains("flash") || lower.contains("lite") {
        tags.push("free tier likely");
    } else if lower.contains("pro") {
        tags.push("paid likely");
    }
    if !lower.contains("lite") && !lower.contains("1b") && !lower.contains("2b") {
        tags.push("tool calling");
    }

    format!("{name:<30} {}", tags.join(" · "))
}

fn merge_dedup_sorted(mut a: Vec<String>, b: Vec<String>) -> Vec<String> {
    a.extend(b);
    a.sort();
    a.dedup();
    a
}

fn prompt_model_id_non_empty(hint: &str) -> Result<String> {
    loop {
        let s: String = Input::new().with_prompt(hint).interact_text()?;
        let t = s.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
        println!("That can’t be empty — paste the exact model id your server expects.");
    }
}

async fn configure_local_openai_compatible() -> Result<ProviderConfig> {
    let variants = [
        "Ollama",
        "LM Studio",
        "vLLM / llama.cpp / custom local server",
    ];
    let choice = Select::new()
        .with_prompt("Choose the local server you want to start from")
        .items(variants)
        .default(0)
        .interact()?;

    let (default_name, default_base_url) = match choice {
        0 => ("ollama", "http://localhost:11434/v1"),
        1 => ("lm-studio", "http://localhost:1234/v1"),
        _ => ("local", "http://localhost:8000/v1"),
    };

    let name: String = Input::new()
        .with_prompt("Provider name to save this as")
        .default(default_name.to_string())
        .interact_text()?;
    let base_url: String = Input::new()
        .with_prompt("Local API base URL (OpenAI-compatible /v1 endpoint)")
        .default(default_base_url.to_string())
        .interact_text()?;

    let mut discovered = list_models_openai_v1(&base_url).await;
    if choice == 0 {
        let ollama_root = ollama_native_base_url(&base_url);
        if let Ok(from_tags) = list_ollama_pulled_models(&ollama_root).await {
            discovered = merge_dedup_sorted(discovered, from_tags);
        }
    }

    let model: String = if !discovered.is_empty() {
        select_or_prompt_model(&discovered, "llama3.2:latest")?
    } else {
        print_section_title("Model discovery");
        println!(
            "  • Could not auto-detect models. Is the dev server running? Try:\n  • GET {}/models",
            base_url.trim_end_matches('/')
        );
        if choice == 0 {
            println!(
                "  • GET {}/api/tags  (Ollama — pulled models)",
                ollama_native_base_url(&base_url).trim_end_matches('/')
            );
        }
        println!();
        prompt_model_id_non_empty(
            "Model id to use in API calls (no default — it must be exact for your server)",
        )?
    };

    Ok(ProviderConfig {
        name,
        kind: ProviderKind::OpenAiCompatible,
        base_url,
        model,
        verified: true,
        auth: Some(ProviderAuthConfig::None),
        api_key_ref: None,
        extra_headers: BTreeMap::new(),
    })
}

fn configure_azure_api_key() -> Result<ProviderConfig> {
    print_tip("Get an Azure OpenAI key and endpoint: https://portal.azure.com/");
    let name: String = Input::new()
        .with_prompt("Provider name to save this as")
        .default("azure".to_string())
        .interact_text()?;
    let base_url: String = Input::new()
        .with_prompt("Azure OpenAI endpoint, e.g. https://your-resource.openai.azure.com")
        .interact_text()?;
    let deployment: String = Input::new()
        .with_prompt("Deployment name (used as the model id)")
        .interact_text()?;
    let secret_ref: String = Input::new()
        .with_prompt("Name for the key in the OS keychain")
        .default("AZURE_OPENAI_API_KEY".to_string())
        .interact_text()?;
    let secret = Password::new()
        .with_prompt(format!("Paste the API key for `{name}`"))
        .interact()?;
    if secret.trim().is_empty() {
        bail!("No API key provided");
    }
    save_secret(&secret_ref, &secret)?;
    Ok(ProviderConfig {
        name,
        kind: ProviderKind::AzureOpenAi,
        base_url,
        model: deployment,
        verified: true,
        auth: Some(ProviderAuthConfig::ApiKey {
            secret_ref,
            help_url: Some("https://portal.azure.com/".to_string()),
        }),
        api_key_ref: None,
        extra_headers: BTreeMap::new(),
    })
}

fn configure_bridge_provider(
    default_name: &str,
    client: McpBridgeClient,
    paths: &ConfigPaths,
) -> Result<(ProviderConfig, String)> {
    let name: String = Input::new()
        .with_prompt("Provider name to save this as")
        .default(default_name.to_string())
        .interact_text()?;
    let bridge = install_bridge(client, &paths.root)?;
    let mut next_step = format!(
        "Configured the {} MCP bridge in {}.",
        client.display_name(),
        bridge.config_path.display()
    );
    if let Some(backup) = bridge.backup_path {
        next_step.push_str(&format!(" Backup: {}.", backup.display()));
    }
    next_step.push_str(&format!(" Next step: {}", bridge.launch_command));
    Ok((
        ProviderConfig {
            name,
            kind: ProviderKind::OpenAiCompatible,
            base_url: "http://127.0.0.1/unused-for-mcp-bridge".to_string(),
            model: "subscription".to_string(),
            verified: true,
            auth: Some(ProviderAuthConfig::McpBridge { client }),
            api_key_ref: None,
            extra_headers: BTreeMap::new(),
        },
        next_step,
    ))
}

fn prompt_project(detected: Option<String>) -> Result<Option<String>> {
    let default_value = detected.unwrap_or_default();
    let project: String = Input::new()
        .with_prompt("Google Cloud project (leave blank to skip)")
        .default(default_value)
        .allow_empty(true)
        .interact_text()?;
    if project.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(project))
    }
}

fn prompt_vertex_region() -> Result<String> {
    loop {
        let region: String = Input::new()
            .with_prompt("Vertex region")
            .default("us-central1".to_string())
            .interact_text()?;
        let trimmed = region.trim();
        if trimmed.is_empty() {
            println!("Vertex region cannot be empty.");
            continue;
        }
        if !trimmed
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
            || trimmed.starts_with('-')
            || trimmed.ends_with('-')
        {
            println!(
                "Vertex region must look like `us-central1` and contain only lowercase letters, digits, and hyphens."
            );
            continue;
        }
        return Ok(trimmed.to_string());
    }
}

fn prompt_extra_headers(extra_headers: &mut BTreeMap<String, String>) -> Result<()> {
    while Confirm::new()
        .with_prompt("Add an extra HTTP header?")
        .default(false)
        .interact()?
    {
        let name: String = Input::new().with_prompt("Header name").interact_text()?;
        let value: String = Input::new().with_prompt("Header value").interact_text()?;
        extra_headers.insert(name, value);
    }
    Ok(())
}

fn upsert_provider(config: &mut AppConfig, provider: ProviderConfig) {
    config.default = provider.name.clone();
    if let Some(existing) = config
        .providers
        .iter_mut()
        .find(|item| item.name == provider.name)
    {
        *existing = provider;
    } else {
        config.providers.push(provider);
    }
}
