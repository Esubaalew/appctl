---
title: Secrets and keys
description: Where appctl stores API keys, how to override them, and how to handle rotation.
---

`appctl` never writes API keys or tokens to the schema or history. They live in one of two places.

## Storage

### OS keychain (preferred)

Service name: `appctl`. Each secret is keyed by a short name (`secret_ref` in
`config.toml`).

Store:

```bash
appctl config set-secret ANTHROPIC_API_KEY --value "$ANTHROPIC_API_KEY"
appctl config set-secret supabase_anon --value "$SUPABASE_ANON_KEY"
```

Platforms:

- macOS: the native Keychain.
- Linux: `secret-service` (GNOME Keyring or KWallet must be running and unlocked).
- Windows: Credential Manager.

### Environment variables (fallback)

If a secret is not in the keychain, `appctl` falls back to an environment variable of the same name:

```bash
export ANTHROPIC_API_KEY="$ANTHROPIC_API_KEY"
appctl chat
```

Env vars always override the keychain at runtime. Useful inside containers, CI, and systemd.

## Referencing secrets

In `config.toml`:

```toml
[[provider]]
name = "claude"
kind = "anthropic"
auth = { kind = "api_key", secret_ref = "ANTHROPIC_API_KEY" }   # <- this name
model = "claude-sonnet-4"
```

In a schema auth block:

```json
"auth": { "kind": "api_key", "header": "apikey", "env_ref": "supabase_anon" }
```

The `env_ref` field names the keychain/env key, not the raw value.

## Rotation

```bash
# Rotate
appctl config set-secret ANTHROPIC_API_KEY --value "$NEW_KEY"
```

No process restart is required. The next tool call reads the updated secret.

## OAuth tokens

`appctl auth target login <provider>` stores target-app OAuth tokens under
service `appctl` with key `appctl_oauth::<provider>`. Provider OAuth / device
code flows store their token blob under `appctl_llm_oauth::<profile>`. Refresh
happens automatically when a refresh token is present; otherwise rerun the
login flow.

## Do not commit

`.appctl/config.toml` can be committed (no secrets). `.appctl/history.db` should not. Recommended `.gitignore`:

```
.appctl/history.db
```

## See also

- [`appctl config`](/docs/cli/config/)
- [`appctl auth`](/docs/cli/auth/)
- [Security](/docs/security/)
