---
title: appctl auth
description: OAuth and credential management for target apps and LLM providers.
---

`appctl auth` splits into two independent namespaces:

- `appctl auth target ...` — credentials for the application described by your
  OpenAPI schema or synced tools. This includes header auth, bearer tokens,
  token endpoints, and OAuth/OIDC profiles.
- `appctl auth provider ...` — credentials for the LLM provider that powers
  `chat`, `run`, and `serve`. Handles API keys, OAuth2, Google ADC, Azure AD
  device code, and MCP bridge clients.

Top-level shortcuts (`appctl auth login [profile]`, `appctl auth status
[profile]`) are kept as aliases for `appctl auth target login / status`.

## Target auth

Target auth is the credential appctl attaches when it calls **your app** through
HTTP tools. It is independent from sync: changing target auth does **not**
regenerate `.appctl/schema.json` or `.appctl/tools.json`.

Target auth covers the credential patterns appctl can add to requests:

| App auth shape | appctl command |
| --- | --- |
| Public API | `appctl auth target clear` |
| Bearer token | `appctl auth target set-bearer --env API_TOKEN` |
| API key or custom header | `appctl auth target set-header 'X-Api-Key: env:API_KEY'` |
| Cookie/session header | `appctl auth target set-header 'Cookie: env:APP_SESSION_COOKIE'` |
| Basic auth or another header scheme | `appctl auth target set-header 'Authorization: Basic env:BASIC_AUTH'` |
| Query parameter token | configure `[target.default_query]` in `.appctl/config.toml` |
| Username/password token endpoint | `appctl auth target token-login <profile> --url <token-endpoint>` |
| OAuth/OIDC browser login | `appctl auth target login <profile> --client-id ... --auth-url ... --token-url ...` |
| Credentials declared in OpenAPI `securitySchemes` | set the referenced env var or keychain secret |

The commands do not change synced tools. They only change how appctl
authenticates when it executes those tools.

### Header auth

Use `set-header` for any API that expects credentials in an HTTP header:

```bash
appctl auth target set-header 'X-Api-Key: env:API_KEY'
appctl auth target set-header 'Authorization: Bearer env:API_TOKEN'
appctl auth target set-header 'Cookie: env:APP_SESSION_COOKIE'
```

Use `set-bearer` when the header is specifically `Authorization: Bearer ...`:

```bash
appctl auth target set-bearer --env API_TOKEN
appctl auth target set-bearer --keychain appctl_target_bearer::production
```

`set-bearer` and `set-header` update `[target].auth_header` in
`.appctl/config.toml`. appctl expands `env:` or `keychain:` at request time.
The model does not receive the secret value.

### Query parameter auth

Some APIs use a query parameter such as `?api_key=...` or `?access_token=...`.
Put those defaults in `.appctl/config.toml`:

```toml
[target.default_query]
api_key = "env:API_KEY"
```

If the OpenAPI document declares a query `apiKey` security scheme, appctl can
also read the referenced secret from the environment or keychain at runtime.

### Token endpoints

Use `token-login` for APIs that issue a token from a username/password or
password-grant style endpoint:

```bash
appctl auth target token-login api \
    --url https://api.example.com/oauth/token \
    --username-env APP_USER \
    --password-env APP_PASSWORD \
    --token-field access_token
```

`token-login` posts `username` and `password`, reads the JSON field named by
`--token-field`, stores the token in the OS keychain, and sets
`[target].auth_header` to a keychain-backed bearer header. Do not paste
passwords into chat.

### OAuth/OIDC browser login

Use `login` when the target application has OAuth/OIDC authorization and token
endpoints:

```bash
appctl auth target login api \
    --client-id <id> \
    --auth-url <URL> \
    --token-url <URL> \
    [--client-secret <secret>] \
    [--scope <scope>]... \
    [--redirect-port 8421]
```

`login` runs an Authorization Code + PKCE flow. A local listener on
`--redirect-port` catches the callback. Missing values fall back to environment
variables named `<NAME>_CLIENT_ID` and `<NAME>_CLIENT_SECRET`.

### Inspect and clear

```bash
appctl auth target status [name]
appctl auth target use <name>
appctl auth target logout <name>
appctl auth target clear
```

The resulting token payload is stored in the OS keychain under
`appctl_oauth::<name>`. `login` also sets `[target].oauth_provider = "<name>"`
so HTTP tools use that bearer token automatically. Use `target use` to switch to
another stored target profile without logging in again.

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

## Changing users without resyncing

Resync only when the app contract changes. To switch the user appctl calls your
API as, change target auth and restart any long-running `appctl chat` or
`appctl serve` process that needs a new environment.

```bash
# env-backed bearer: rotate the env var value
export API_TOKEN='new-token'
appctl auth target status
appctl doctor --write

# token endpoint: log in again outside chat
appctl auth target token-login api \
  --url https://api.example.com/oauth/token \
  --username-env APP_USER \
  --password-env APP_PASSWORD
```

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
