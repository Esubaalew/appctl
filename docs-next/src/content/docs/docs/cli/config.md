---
title: appctl config
description: Initialize, inspect, and manage appctl configuration and secrets.
---

Manage `.appctl/config.toml` and secrets in the OS keychain.

## Usage

```
appctl config <COMMAND>
```

Commands:

- `init` — create `.appctl/config.toml` with defaults.
- `show` — print the current config as TOML.
- `provider-sample` — print a sample config covering a local Ollama and an Anthropic provider.
- `set-secret <NAME>` — store a secret in the OS keychain (service `appctl`). Env vars of the same name still override at runtime.

## Config file layout

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

[behavior]
max_iterations = 8
history_limit = 100
```

### `[[provider]]` fields

- `name` — user-facing name (`--provider <name>`).
- `kind` — `anthropic` or `open_ai_compatible`.
- `base_url` — REST root.
- `model` — default model for this provider.
- `api_key_ref` — name of the secret (keychain entry, or env var) that holds the API key.
- `extra_headers` — optional headers (for custom gateways).

### `[behavior]`

- `max_iterations` — upper bound on agent loop iterations (default `8`).
- `history_limit` — how many past messages to include in context (default `100`).

## Secrets

```bash
appctl config set-secret anthropic --value "$ANTHROPIC_API_KEY"
appctl config set-secret supabase_anon --value "$SUPABASE_ANON_KEY"
```

Values are stored in the OS keychain under service `appctl`. On Linux, `secret-service` (GNOME Keyring or KWallet) is required.

If a secret is not in the keychain, `appctl` falls back to the environment variable of the same name.

## Related

- [Installation → Configure a provider](/docs/installation/#configure-a-provider)
- [Secrets and keys](/docs/deploy/secrets-and-keys/)
