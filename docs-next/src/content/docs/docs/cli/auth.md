---
title: appctl auth
description: OAuth and credential management for target apps and LLM providers.
---

`appctl auth` splits into two independent namespaces:

- `appctl auth target ...` — OAuth flows for the application you are *managing*
  (e.g. a GitHub or Stripe account behind your OpenAPI spec). Tokens are stored
  under the `appctl_oauth::<name>` keychain entry.
- `appctl auth provider ...` — credentials for the LLM provider that powers
  `chat`, `run`, and `serve`. Handles API keys, OAuth2, Google ADC, Azure AD
  device code, and MCP bridge clients.

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
| `google_adc` | Checks whether `gcloud auth application-default login` has already been run and prints the recovery command when it is missing. |
| `qwen_oauth` | Status-only today. `appctl` can read stored tokens if you configured them manually, but the interactive Qwen OAuth login is not wired into this command. |
| `azure_ad` | Starts the Azure AD device-code flow, opens the verification URL, and stores the resulting tokens in the keychain. |
| `mcp_bridge` | Prints launch guidance for the external client (Codex CLI, Claude Code, Qwen Code, Gemini CLI). |

`provider status` prints a one-line summary per provider:

```text
gemini kind=google_genai model=gemini-2.5-pro auth=oauth2 configured=true
claude kind=anthropic model=claude-sonnet-4 auth=api_key configured=false
  secret_ref: ANTHROPIC_API_KEY
openai kind=open_ai_compatible model=gpt-5 auth=api_key configured=true
```

`provider logout` removes the stored key or token blob that `appctl` owns for a
named provider. For Google ADC and MCP bridges, it prints the external cleanup
step instead.
`provider list` is an alias for `provider status` without a filter.

## Presets for unconfigured providers

Calling `provider login <name>` with no `[[provider]]` block in
`.appctl/config.toml` uses a built-in preset so you can bootstrap quickly:

| Name | Kind | Auth | Secret name |
| --- | --- | --- | --- |
| `gemini` | `google_genai` | OAuth2 | `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET` |
| `qwen` | `open_ai_compatible` | `api_key` | `DASHSCOPE_API_KEY` |
| `claude` | `anthropic` | `api_key` | `ANTHROPIC_API_KEY` |
| `openai` | `open_ai_compatible` | `api_key` | `OPENAI_API_KEY` |
| `vertex` | `vertex` | `google_adc` | — |
| `ollama` | `open_ai_compatible` | `none` | — |

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

## Signing in with a consumer subscription

If you want to use your ChatGPT or Claude consumer subscription, use the
**MCP bridge**: install the vendor CLI (Codex CLI, Claude Code, Qwen Code,
Gemini CLI), sign in there, then point `appctl` at that client:

```toml
[[provider]]
name = "openai-subscription"
kind = "open_ai_compatible"
auth = { kind = "mcp_bridge", client = "codex" }
```

See the [Provider matrix](/docs/provider-matrix/) for the full list.

## Azure AD

For providers with `auth.kind = "azure_ad"`, `appctl auth provider login
<name>` starts the device-code flow immediately. `appctl chat` and
`appctl run` can then reuse the stored bearer token.

## Related

- [`appctl config`](/docs/cli/config/) — where the provider entry lives.
- [Provider matrix](/docs/provider-matrix/) — authoritative list of what each
  provider accepts.
- [Secrets and keys](/docs/deploy/secrets-and-keys/) — runtime precedence
  (env var > keychain) and CI recipes.
