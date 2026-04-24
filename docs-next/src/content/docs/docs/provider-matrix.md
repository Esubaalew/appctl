---
title: Provider matrix
description: Which authentication method each provider uses in appctl.
---

Pick the column that matches how you want to sign in. If a cell says "—", that
path isn't available for the provider.

| Provider | Direct API | MCP bridge via another CLI |
| --- | --- | --- |
| Gemini API | API key or OAuth2 | Gemini CLI |
| Vertex AI Gemini | Google Application Default Credentials (`gcloud`) | — |
| OpenAI (GPT) | API key | Codex CLI |
| Anthropic Claude | API key | Claude Code |
| Qwen DashScope | API key | Qwen Code |
| Azure OpenAI | API key or Azure AD | — |
| OpenAI-compatible gateways (OpenRouter, Groq, NVIDIA NIM, custom) | API key | — |
| Local OpenAI-compatible (Ollama, LM Studio, vLLM, llama.cpp) | No authentication | — |

## What "API key" means

Run:

```bash
appctl auth provider login <name>
```

or store the secret directly:

```bash
appctl config set-secret <SECRET_NAME>
```

`SECRET_NAME` is whatever `secret_ref` the `auth` block declares. The CLI
prompts you without echo, writes the value into the OS keychain, and you are
done. No browser.

## What "real browser" (Google ADC) means

Vertex AI reuses Google's Application Default Credentials. You run:

```bash
gcloud auth application-default login
```

Your default browser opens. After you sign in, `gcloud` owns the token cache
at `~/.config/gcloud/application_default_credentials.json` and `appctl` asks
`gcloud` for a fresh access token on every call. Run `appctl auth provider
login vertex` afterwards to sanity-check that the credentials are readable.

## What "OAuth2" means

For the Gemini preset with `auth.kind = "oauth2"`, running `appctl auth
provider login gemini` performs a standard OAuth2 Authorization-Code-with-PKCE
flow against `accounts.google.com`. Supply `GOOGLE_CLIENT_ID` and
`GOOGLE_CLIENT_SECRET` from your Google Cloud OAuth client (environment
variables or keychain).

## What "Azure AD" means

When a provider is configured with `auth.kind = "azure_ad"`, the Azure AD
device-code flow runs the first time the provider makes a request. A code is
printed, you complete the flow at
<https://microsoft.com/devicelogin>, and the access token is cached in the OS
keychain until expiry.

## What "MCP bridge" means

The provider's official CLI keeps model auth and billing. `appctl` registers
itself as an MCP server inside that CLI's config:

- Codex CLI: `~/.codex/config.toml`
- Claude Code: `~/.claude/settings.json`
- Qwen Code: `~/.qwen/settings.json`
- Gemini CLI: `~/.gemini/settings.json`

You install the external client yourself, then add an `[[provider]]` block in
`.appctl/config.toml` with `auth = { kind = "mcp_bridge", client = "..." }`.
Launching that client then lets it talk to `appctl mcp serve` for tools.

## Signing in with a ChatGPT or Claude consumer subscription

Use the **MCP bridge** column. `appctl` talks to the vendor's own CLI
(Codex CLI for ChatGPT, Claude Code for Claude), and that CLI handles the
subscription login in its own browser flow. Your subscription, your billing,
your quota — `appctl` just borrows the session to call models.

The bridge requires the external CLI to be installed first. Configure it with:

```toml
[[provider]]
name = "openai-subscription"
kind = "open_ai_compatible"
auth = { kind = "mcp_bridge", client = "codex" }
```

## Checking your credentials

Run:

```bash
appctl auth provider status
```

It lists every provider, the auth method in use, and whether credentials are
present locally. Wrong keys still show up on the first real `appctl chat`
turn, when the provider responds.
