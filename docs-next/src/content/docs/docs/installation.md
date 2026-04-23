---
title: Installation
description: Install appctl and configure at least one language model provider.
---

`appctl` is a single Rust binary. Install it from crates.io, from source, or
pull a prebuilt release. The embedded web UI is bundled into the binary at
build time — there is nothing extra to install for [`appctl serve`](/docs/cli/serve/).

## Supported platforms

`appctl` is tested in CI on:

- macOS (arm64, x86_64)
- Linux (x86_64, aarch64, glibc ≥ 2.31)
- Windows (x86_64, MSVC toolchain)

## From crates.io

```bash
cargo install appctl
```

Verify:

```bash
appctl --version
```

Output matches the version in this repo's `Cargo.toml` (currently `{{appctl_version}}`).

## From source

Tracking `main` is the fastest path for new features. The embedded web UI
must be built before you `cargo install`, because `appctl` is compiled against
the `web/dist/` output.

```bash
git clone https://github.com/Esubaalew/appctl.git
cd appctl

# Build the web UI bundle (Node 20+ required, matches CI)
cd web && npm ci && npm run build && cd ..

# Install the binary
cargo install --locked --path crates/appctl
```

If you skip the `web` build step, `cargo install` will still succeed but the
`appctl serve` web console will 404 on `/`.

## From a prebuilt release

Each GitHub Release ships self-contained binaries for macOS (Intel + Apple
Silicon), Linux (x86_64 + aarch64), and Windows (x86_64):

```bash
gh release download -R Esubaalew/appctl --pattern '*-apple-darwin*'
tar -xzf appctl-*-apple-darwin*.tar.gz
mv appctl /usr/local/bin/
```

Or grab them from the [Releases page](https://github.com/Esubaalew/appctl/releases).

## Configure a provider

`appctl` needs at least one language-model provider to run `chat`, `run`, or
`serve`. The simplest path is:

```bash
# Initialize the config file
appctl config init

# Append a preset for the provider you want
appctl config provider-sample --preset openai >> .appctl/config.toml
```

Currently-supported presets:

- `openai` — OpenAI (API key)
- `claude` — Anthropic (API key)
- `gemini` — Google Gemini (OAuth2)
- `vertex` — Vertex AI (Google ADC)
- `qwen` — Qwen DashScope (API key, OpenAI-compatible transport)
- `ollama` — local Ollama (no auth)
- `default` — a multi-provider scaffold

Then either set the API key:

```bash
appctl config set-secret OPENAI_API_KEY
# prompts you, no echo
```

Or run the OAuth / ADC flow:

```bash
appctl auth provider login gemini         # OAuth2 (real browser)
gcloud auth application-default login && appctl auth provider login vertex
```

Secrets are written to the OS keychain (macOS Keychain, Windows Credential
Manager, GNOME Keyring / libsecret on Linux). Environment variables with the
same name override at runtime.

## Supported LLM providers

Native transports:

- **OpenAI-compatible** (`kind = "open_ai_compatible"`) — OpenAI, OpenRouter,
  Groq, Together, Fireworks, NVIDIA NIM, Mistral's OpenAI endpoint, LiteLLM,
  Ollama, LM Studio, vLLM, llama.cpp server, DashScope (Qwen), anything that
  speaks `/chat/completions`.
- **Anthropic** (`kind = "anthropic"`) — Claude with the native API shape.
- **Google GenAI** (`kind = "google_genai"`) — Gemini via the
  `generativelanguage.googleapis.com` endpoint.
- **Vertex AI** (`kind = "vertex_ai"`) — Gemini via Google Vertex, using
  Google ADC.
- **Azure OpenAI** (`kind = "azure_openai"`) — Azure deployments with AAD or
  key auth.

See [Provider matrix](/docs/provider-matrix/) for the exact `auth` shape and
billing expectations for each one.

## Verify the install

```bash
appctl --help
```

You should see every subcommand:

```text
Commands:
  sync      Introspect your app and regenerate the tool schema.
  chat      Interactive REPL against the synced schema.
  run       One-shot prompt against the synced schema.
  doctor    Probe HTTP tools for reachability and verify provenance.
  history   Print the audit log of every tool call.
  serve     Start the HTTP + WebSocket server and embedded web UI.
  config    View and edit .appctl/config.toml and keychain secrets.
  plugin    Manage dynamic sync plugins.
  auth      Authenticate the target app and the LLM provider.
  mcp       Run appctl itself as an MCP server.
```

## Next

- [Quickstart](/docs/quickstart/) — run a demo app end-to-end.
- [Provider matrix](/docs/provider-matrix/) — choose the right auth path.
- [Sources](/docs/sources/openapi/) — pick a sync source for your app.
