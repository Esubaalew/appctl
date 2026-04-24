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

Output matches the binary you actually installed. If you build from source, the
version comes from the root `Cargo.toml` in that checkout.

## From source

Tracking `main` is the fastest path for new features. A normal checkout already
includes the current embedded web bundle, so a straight `cargo install` works.
Rebuild the web bundle only if you changed `web/src` locally or want to refresh
the tracked assets before compiling.

```bash
git clone https://github.com/Esubaalew/appctl.git
cd appctl

# Install the binary
cargo install --locked --path crates/appctl
```

If you changed `web/src`, rebuild the bundle first:

```bash
cd web && npm ci && npm run build && cd ..
cargo install --locked --path crates/appctl
```

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
`serve`. Start with the interactive wizard:

```bash
appctl init
```

It walks you through picking a provider, runs the real auth flow (browser
OAuth, device code, or API-key prompt), stores the secret in the OS keychain,
and verifies the connection with a tiny live call before printing `done`. See
[`appctl init`](/docs/init/) for everything it touches.

If you prefer to skip the wizard and write the config by hand, the lower-level
building blocks are still available:

```bash
# Initialize an empty config file
appctl config init

# Append a preset for a specific provider
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

## Register as a named app (optional)

If you are going to juggle more than one `.appctl/` directory across
projects, register this one so you can switch contexts by name:

```bash
appctl app add backend     # name defaults to the parent folder
appctl app list            # shows all registered apps and the active one
appctl app use backend     # set the globally active app
```

See [`appctl app`](/docs/cli/app/) for the full resolution rules (explicit
flag → auto-detect from cwd → global active app).

## Supported LLM providers

Native transports:

- **OpenAI-compatible** (`kind = "open_ai_compatible"`) — OpenAI, OpenRouter,
  Groq, Together, Fireworks, NVIDIA NIM, Mistral's OpenAI endpoint, LiteLLM,
  Ollama, LM Studio, vLLM, llama.cpp server, DashScope (Qwen), anything that
  speaks `/chat/completions`.
- **Anthropic** (`kind = "anthropic"`) — Claude with the native API shape.
- **Google GenAI** (`kind = "google_genai"`) — Gemini via the
  `generativelanguage.googleapis.com` endpoint.
- **Vertex AI** (`kind = "vertex"`) — Gemini via Google Vertex, using
  Google ADC.
- **Azure OpenAI** (`kind = "azure_open_ai"`) — Azure deployments with AAD or
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
  init      Set up a `.appctl` directory (models, auth, and provider) interactively.
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
  app       Manage known app contexts and the global active app.
```

## Next

- [Quickstart](/docs/quickstart/) — run a demo app end-to-end.
- [Provider matrix](/docs/provider-matrix/) — choose the right auth path.
- [Sources](/docs/sources/openapi/) — pick a sync source for your app.
