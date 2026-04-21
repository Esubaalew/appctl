---
title: Installation
description: Install appctl and configure at least one language model provider.
---

`appctl` is a single Rust binary. Install it from crates.io, from source, or grab a prebuilt release.

## From crates.io

```bash
cargo install appctl
```

Verify:

```bash
appctl --version
# appctl 0.2.0
```

## From source

Clone the repo and install in place. This is the fastest way to track `main`:

```bash
git clone https://github.com/Esubaalew/appctl.git
cd appctl
cargo install --locked --path crates/appctl
```

To build the embedded web UI the same way CI does (required for a clean install from a working tree):

```bash
cd web && npm ci && npm run build && cd ..
cargo install --locked --path crates/appctl
```

## From a prebuilt release

Each GitHub Release ships binaries for macOS (Intel, Apple Silicon), Linux (x86_64, aarch64), and Windows (x86_64). Download with `gh`:

```bash
gh release download -R Esubaalew/appctl
```

Or from the [Releases page](https://github.com/Esubaalew/appctl/releases).

## Configure a provider

`appctl` needs at least one language model provider. Create the default config:

```bash
appctl config init
```

Open `.appctl/config.toml` and uncomment the provider you want, or start from the sample:

```bash
appctl config provider-sample
```

Output:

```toml
default = "ollama"

[[provider]]
name = "claude"
kind = "anthropic"
base_url = "https://api.anthropic.com"
model = "claude-sonnet-4"
api_key_ref = "anthropic"

[[provider]]
name = "ollama"
kind = "open_ai_compatible"
base_url = "http://localhost:11434/v1"
model = "llama3.1"
```

### Store the API key

Store the key in the OS keychain (keyed by `api_key_ref`):

```bash
appctl config set-secret anthropic --value "$ANTHROPIC_API_KEY"
```

Secrets never leave your machine. An environment variable of the same name still overrides at runtime.

## Supported LLM providers

Any OpenAI-compatible endpoint works out of the box:

- OpenAI, OpenRouter, NVIDIA NIM, Groq, Together, Fireworks
- Ollama, LM Studio, vLLM, LiteLLM (local or self-hosted)
- Anthropic Claude (native `kind = "anthropic"`)

Google Gemini and xAI work through their OpenAI-compatible endpoints.

## Verify install

```bash
appctl --help
```

You should see a list of subcommands: `sync`, `chat`, `run`, `doctor`, `history`, `serve`, `config`, `plugin`, `auth`.

## Next

- [Quickstart](/docs/quickstart/): run through a demo app end-to-end.
- [Sources](/docs/sources/openapi/): pick a sync source for your app.
