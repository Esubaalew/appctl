---
title: Provider matrix
description: Exactly which auth paths are implemented in appctl today, cell by cell.
---

This is the honest list. Each cell is either **real browser**, **API key**,
**MCP bridge**, or **not supported**. If a path is not in this table, it is
not wired — no matter what third-party docs say.

| Provider | Direct API | Direct subscription | MCP bridge |
| --- | --- | --- | --- |
| Gemini API | API key or OAuth2 | not supported | MCP bridge via Gemini CLI |
| Vertex AI Gemini | Google ADC (real browser via `gcloud`) | not supported | not applicable |
| OpenAI (GPT) | API key | not supported | MCP bridge via Codex CLI |
| Anthropic Claude | API key | not supported | MCP bridge via Claude Code |
| Qwen DashScope | API key | not supported | MCP bridge via Qwen Code |
| Azure OpenAI | API key or Azure AD | not supported | not applicable |
| OpenAI-compatible gateways (OpenRouter, Groq, NVIDIA NIM, custom) | API key | not applicable | not applicable |
| Local OpenAI-compatible (Ollama, LM Studio, vLLM, llama.cpp) | no auth | not applicable | not applicable |

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
flow against `accounts.google.com`. You need `GOOGLE_CLIENT_ID` and
`GOOGLE_CLIENT_SECRET` in the environment (or the keychain) for your own
OAuth client — `appctl` does not ship an embedded public client for Gemini.

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

## What is NOT supported (and will not be faked)

- Direct ChatGPT subscription auth inside `appctl`. No scraping, no
  unofficial OAuth.
- Direct Claude consumer subscription auth inside `appctl`.
- Any "login with X" that does not have a documented public OAuth client.

## Verification

`appctl auth provider status` prints each provider's auth kind, whether
credentials are configured, and a recovery hint when they are not. It does
**not** currently send a live request to the provider — errors from bad keys
surface on the first real `chat` or `run` call.
