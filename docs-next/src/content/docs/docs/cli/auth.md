---
title: appctl auth
description: OAuth and credential management for target apps and LLM providers.
---

`appctl auth` splits into two independent namespaces:

- `appctl auth target ...` — OAuth flows for the application you are *managing*
  (e.g. a GitHub or Stripe account behind your OpenAPI spec). Tokens are stored
  under the `appctl_oauth::<name>` keychain entry.
- `appctl auth provider ...` — credentials for the LLM provider that powers
  `chat`, `run`, and `serve`. Handles API keys, OAuth2, Google ADC, Azure AD,
  Qwen device flow, and MCP bridge clients.

Top-level shortcuts (`appctl auth login <provider>`, `appctl auth status
<provider>`) are kept as aliases for `appctl auth target login / status`.

## Target auth

```bash
appctl auth target login <name> \
    --client-id <id> \
    --auth-url <URL> \
    --token-url <URL> \
    [--client-secret <secret>] \
    [--scope <scope>]... \
    [--redirect-port 8421]

appctl auth target status <name>
```

`login` runs a real OAuth 2.0 Authorization-Code-with-PKCE flow against the
URLs you pass. A local listener on `--redirect-port` catches the callback.
Missing values fall back to environment variables named `<NAME>_CLIENT_ID`
and `<NAME>_CLIENT_SECRET`.

The resulting token payload is stored in the OS keychain under
`appctl_oauth::<name>`.

## Provider auth

```bash
appctl auth provider login <name>   [--profile <str>] [--value <str>] [--client-id ...] [--client-secret ...] [--auth-url ...] [--token-url ...] [--scope ...] [--redirect-port 8421]
appctl auth provider status [name]
appctl auth provider logout <name>
appctl auth provider list
```

`provider login` inspects the `auth` block on the provider (or a built-in
preset if the provider is not yet in `.appctl/config.toml`) and runs the
matching flow:

| `auth.kind` | What happens |
| --- | --- |
| `none` | Nothing to do — "no credentials required". |
| `api_key` | Prompt for the key (or read `--value`), write it to the keychain under `secret_ref`. No browser. |
| `oauth2` | Authorization-Code + PKCE flow using the configured `auth_url`, `token_url`, scopes, and client id/secret. Opens a real browser. |
| `google_adc` | Requires that `gcloud auth application-default login` has already been run. Prints the project hint and recovery command if the ADC credentials are missing. |
| `qwen_oauth` | Same detection flow — prints a recovery hint when the token file is missing. |
| `azure_ad` | Same — the Azure AD device-code flow is triggered by the verify path, not by `login`. |
| `mcp_bridge` | Prints the recovery hint for the external client (Codex CLI, Claude Code, Qwen Code, Gemini CLI). |

`provider status` prints a one-line summary per provider:

```text
gemini       kind=google_genai     auth=api_key         configured
claude       kind=anthropic        auth=api_key         missing GOOGLE_API_KEY
openai       kind=open_ai_compat   auth=api_key         configured
```

`provider logout` deletes the stored credentials for a named provider.
`provider list` is an alias for `provider status` without a filter.

## Presets for uncofigured providers

Calling `provider login <name>` with no `[[provider]]` block in
`.appctl/config.toml` uses a built-in preset so you can bootstrap quickly:

| Name | Kind | Auth | Secret name |
| --- | --- | --- | --- |
| `gemini` | `google_genai` | OAuth2 | `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET` |
| `qwen` | `open_ai_compat` | `api_key` | `DASHSCOPE_API_KEY` |
| `claude` | `anthropic` | `api_key` | `anthropic` |
| `openai` | `open_ai_compat` | `api_key` | `OPENAI_API_KEY` |
| `vertex` | `google_genai` | Google ADC | — |
| `ollama` | `open_ai_compat` | `none` | — |

To make the provider available for `chat` and `run`, also add the matching
`[[provider]]` block to `.appctl/config.toml` (use `appctl config
provider-sample --preset <name>` for a scaffold).

## Examples

```bash
# API key: store it in the keychain, no browser
appctl auth provider login openai
# → Enter API key for `openai`: ************

# OAuth2: real browser flow
GOOGLE_CLIENT_ID=xxx GOOGLE_CLIENT_SECRET=yyy appctl auth provider login gemini

# Google ADC (run gcloud first)
gcloud auth application-default login
appctl auth provider login vertex

# Verify the current state of everything
appctl auth provider status

# Remove a stored credential
appctl auth provider logout openai
```

## What is not there

- There is **no** built-in "login with ChatGPT subscription" or "login with
  Claude subscription" flow inside this command. The MCP bridge entries that
  appear under `kind = "mcp_bridge"` depend on the external client already
  being installed (Codex CLI, Claude Code, Qwen Code, Gemini CLI).
- There is no Azure CLI wrapper here. The Azure AD path currently expects the
  access token to be retrieved by the runtime verify logic, not by `login`.

## Related

- [`appctl config`](/docs/cli/config/) — where the provider entry lives.
- [Provider matrix](/docs/provider-matrix/) — authoritative list of what each
  provider accepts.
- [Secrets and keys](/docs/deploy/secrets-and-keys/) — runtime precedence
  (env var > keychain) and CI recipes.
