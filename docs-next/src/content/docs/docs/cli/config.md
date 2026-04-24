---
title: appctl config
description: Create, inspect, and edit the app's provider configuration.
---

`appctl config` manages the TOML file at `.appctl/config.toml` and secrets
stored in the OS keychain. The file decides which providers exist, what model
each one uses, and which is the default.

## Usage

```bash
appctl config <COMMAND>
```

## Subcommands

| Command | What it does |
| --- | --- |
| `appctl config init` | Create `.appctl/config.toml` with the default scaffold. Fails nothing, writes a stock file. |
| `appctl config show` | Print the current `.appctl/config.toml`, with secrets redacted. |
| `appctl config provider-sample [--preset <name>]` | Print a ready-to-paste `[[provider]]` block for a known preset (see below). |
| `appctl config set-secret <NAME> [--value <STRING>]` | Store a secret in the OS keychain under service `appctl`. If `--value` is omitted you are prompted for it without echo. |

## Presets available to `provider-sample`

The `--preset` argument accepts one of:

- `default` — the whole-file scaffold (multiple providers)
- `gemini` — Google Gemini via OAuth2
- `vertex` — Google Vertex via application-default credentials (with a region header placeholder)
- `openai` — OpenAI API
- `claude` — Anthropic Claude API
- `qwen` — Qwen via DashScope (OpenAI-compatible)
- `ollama` — local Ollama (no auth)

Anything else returns a "unknown preset" error.

## The file format

```toml
default = "gemini"

[[provider]]
name = "gemini"
kind = "google_genai"
base_url = "https://generativelanguage.googleapis.com"
model = "gemini-2.5-pro"
auth = { kind = "api_key", secret_ref = "GOOGLE_API_KEY" }
```

- `default` — the provider used when `--provider` is not passed on `chat`,
  `run`, or `serve`.
- `[[provider]]` — one block per configured provider. `kind` is one of
  `open_ai_compatible`, `anthropic`, `google_genai`, `azure_open_ai`,
  `vertex`.
- `auth` — one of: `{ kind = "none" }`, `{ kind = "api_key", secret_ref
  = "..." }`, `{ kind = "oauth2", profile = "...", scopes = [...] }`,
  `{ kind = "google_adc", project = "..." }`, `{ kind = "azure_ad", ... }`,
  `{ kind = "mcp_bridge", client = "..." }`.
- `[target]` — your **app under control** (HTTP base URL, auth for tools, default query, database URL). See [OpenAPI: protected APIs](/docs/sources/openapi/#openapi-and-protected-authenticated-apis) for `auth_header`, `base_url`, and `default_query`.
  - `auth_header` — optional; sent as the `Authorization` request header for HTTP tools.
  - `base_url` / `base_url_env` — override the synced API base URL.
  - `default_query` — optional table of default query parameters for HTTP tools; values can be `env:VAR` to read from the environment. Tool call arguments override the same key.

## Secrets

`set-secret` writes to the OS keychain (macOS Keychain, Windows Credential
Manager, GNOME Keyring / libsecret on Linux) under the service `appctl`. The
same name is also honoured from environment variables and always takes
precedence at runtime.

```bash
# interactive, no echo
appctl config set-secret GOOGLE_API_KEY

# explicit, shell-quoted
appctl config set-secret GOOGLE_API_KEY --value "$GOOGLE_API_KEY"
```

## Examples

```bash
# Scaffold a fresh app
appctl config init
appctl config provider-sample --preset openai >> .appctl/config.toml

# Inspect the merged configuration
appctl config show

# Store an API key for the openai preset above
appctl config set-secret OPENAI_API_KEY
```

## Related

- [`appctl auth`](/docs/cli/auth/) — OAuth / ADC / device-code flows for
  providers that do not use API keys.
- [Provider matrix](/docs/provider-matrix/) — the auth kind every supported
  provider actually uses.
- [Secrets and keys](/docs/deploy/secrets-and-keys/) — how secrets flow
  through the CLI, server, and CI.
