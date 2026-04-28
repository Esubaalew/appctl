---
title: Installation
description: Build or install the appctl binary and set up a provider in config.
---

`appctl` is distributed as a Rust binary. Install from crates.io, from a git
checkout, or from release artifacts. The web console for
[`appctl serve`](/docs/cli/serve/) is embedded at build time; no separate web
package is installed at runtime.

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

The reported version is that of the installed binary. Source builds use the
workspace version in the root `Cargo.toml`.

## From source

A checkout of `main` includes the prebuilt `web/dist` that `build.rs` copies
into the crate. Rebuild the web app (`cd web && npm ci && npm run build`) if you
change `web/src` before `cargo build` or `cargo install`.

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

## Start the guided setup

After install, the simplest path is:

```bash
appctl setup
```

It guides you through provider setup, source selection, sync, checks, and the
next command for terminal or web chat. See [First 10 minutes](/docs/first-10-minutes/)
for the complete first-run flow.

## Configure a provider manually

`appctl` needs at least one language-model provider to run `chat`, `run`, or
`serve`. If you only want to configure the provider and sync manually, use:

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
  setup     Guided first-run setup: provider, sync source, checks, and next steps.
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

- [First 10 minutes](/docs/first-10-minutes/) — the recommended setup path.
- [Quickstart](/docs/quickstart/) — run a demo app end-to-end.
- [Provider matrix](/docs/provider-matrix/) — choose the right auth path.
- [Sources](/docs/sources/openapi/) — pick a sync source for your app.
