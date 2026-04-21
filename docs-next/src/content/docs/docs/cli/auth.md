---
title: appctl auth
description: Manage target-app auth and LLM provider auth from explicit CLI subcommands.
---

`appctl auth` is now split into two surfaces:

- `appctl auth target ...` for the synced app you call through tools
- `appctl auth provider ...` for the LLM provider that powers `chat`, `run`, and `serve`

## Usage

```
appctl auth <COMMAND>
```

Commands:

- `target login <provider>`
- `target status <provider>`
- `provider login <name>`
- `provider status [name]`
- `provider logout <name>`
- `provider list`

The older `appctl auth login ...` and `appctl auth status ...` commands remain as deprecated aliases for `target`.

## `appctl auth target login`

```
appctl auth target login [OPTIONS] <PROVIDER>
```

### Options

- `--client-id <ID>`
- `--client-secret <SECRET>`
- `--auth-url <URL>` — authorization endpoint.
- `--token-url <URL>` — token exchange endpoint.
- `--scope <SCOPE>` — OAuth scope (provider-specific).
- `--redirect-port <PORT>` — local port for the redirect listener (default `8421`).

### Example

```bash
appctl auth target login github \
  --client-id "$GH_CLIENT_ID" \
  --client-secret "$GH_CLIENT_SECRET" \
  --auth-url https://github.com/login/oauth/authorize \
  --token-url https://github.com/login/oauth/access_token \
  --scope "repo read:user"
```

`appctl` opens the browser, waits for the callback on `localhost:<redirect-port>`, exchanges the code for a token, and stores it in the target-auth namespace in the keychain.

## `appctl auth provider login`

Use this when the model provider itself needs OAuth or local ADC discovery.

### Gemini OAuth

```bash
appctl auth provider login gemini
```

If the provider config already declares OAuth2, `appctl` reuses that profile and scope list. For Gemini, `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` are used automatically when present.

### Qwen / DashScope

```bash
appctl auth provider login qwen
```

For API-key providers, the login command stores the configured secret in the keychain instead of opening a browser flow.

## `appctl auth provider status`

```bash
appctl auth provider status
```

This prints the auth kind, whether credentials are configured, expiry when known, and recovery hints.

## `appctl auth provider list`

```bash
appctl auth provider list
```

List every configured provider and its redacted auth status.

## `appctl auth provider logout`

```bash
appctl auth provider logout gemini
```

Deletes the stored provider OAuth token for that provider's configured profile.

## Using target OAuth in sync

Reference the provider in your schema's `auth` block:

```json
"auth": { "kind": "oauth_flow", "provider": "github" }
```

`appctl` fetches the target token at call time and sets `Authorization: Bearer <token>` on every HTTP tool.

## Related

- [Provider matrix](/docs/provider-matrix/)
- [`appctl config`](/docs/cli/config/)
- [Secrets and keys](/docs/deploy/secrets-and-keys/)
- [Security](/docs/security/)
