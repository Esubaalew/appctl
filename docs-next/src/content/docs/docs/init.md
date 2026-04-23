---
title: appctl init
description: Interactive first-time setup. No hand-edited config.
---

`appctl init` is the only blessed way to configure a provider. It writes
`.appctl/config.toml`, stores secrets in the OS keychain, runs the real auth
flow, and ends with a live verify call before printing `done`. If the verify
call fails, the config is still written but the provider is marked
`verified = false` so later commands can warn you instead of silently using a
broken setup.

## Usage

```bash
appctl init
```

Run it from the project root you want to control. `init` will create
`.appctl/config.toml`, `.appctl/schema.json` (empty), and other tracking files
in the current directory unless `--app-dir` is passed.

## What it walks you through

1. **Detect existing config.** If `.appctl/config.toml` already exists, it
   asks whether to replace or augment it so you can add a second provider
   without losing the first.
2. **Pick an auth path.** A menu is shown with four broad options:
   - Direct API via a real browser
     (Vertex via Google ADC, Azure OpenAI via Azure AD device code).
   - Direct API via an API key (OpenAI, Anthropic, Google GenAI, Groq,
     Together, Mistral, Cohere, Fireworks, Perplexity, DeepSeek, xAI, Qwen,
     local Ollama, …).
   - Guided OpenAI-compatible setup (OpenRouter, NVIDIA NIM, custom base URLs).
   - MCP subscription bridge (Codex, Claude Code, Qwen Code, Gemini CLI).
3. **Run the real flow for that path.**
   - **Google ADC (Vertex):** shells out to
     `gcloud auth application-default login`. Your default browser opens, you
     sign in, and `gcloud` owns the token cache. `appctl` reads fresh access
     tokens on demand, never stores them.
   - **Azure AD device code:** prints a one-time code, opens
     `https://microsoft.com/devicelogin`, and polls until you sign in. The
     resulting access token is stored in the OS keychain.
   - **API key:** prompts for the key, stores it in the keychain under a
     `appctl:<provider>` entry, and records the provider's help URL so later
     error messages can point users back to the key-issuance page.
   - **MCP bridge:** writes an `appctl` entry into the external client's
     config file (for example `~/.codex/config.toml`) with a timestamped
     backup of the original file.
4. **Pick a model.** For providers that expose a list endpoint, `init` pulls
   the actual catalogue for your account and shows a searchable selector
   (type to filter). For providers without a list endpoint, a small curated
   default list is offered with an "enter your own" escape hatch.
5. **Verify.** For direct-API providers, `init` sends a tiny
   `"Reply with ok."` prompt to the chosen model. On success the provider is
   stored with `verified = true`. On failure (invalid key, 404 model, 429
   quota, etc.), the config is still saved but `verified = false`. You then
   see an actionable error and a hint to rerun the step after fixing the
   underlying problem.
6. **Print the next command.** For direct-API providers this is
   `appctl chat` in the same directory. For MCP bridge providers it is the
   external client's launch command (`codex`, `claude`, `qwen`, `gemini`).

## Secrets

| What | Where it lives |
| --- | --- |
| API keys, OAuth refresh tokens | OS keychain (`security` on macOS, `secret-service` on Linux, Credential Manager on Windows). |
| Azure AD access tokens | OS keychain. |
| Google ADC tokens | `gcloud`'s own cache (`~/.config/gcloud/application_default_credentials.json`). |
| MCP client config | The external client's config file, with a backup beside it. |
| Non-secret provider metadata (model, base URL, `verified` flag) | `.appctl/config.toml` in your project. |

The secret itself is never written to `config.toml` and never printed to the
terminal. During a session, the key is read once into memory so macOS does
not re-prompt for keychain access on every chat turn.

## Re-running

`appctl init` is safe to re-run. It will detect the existing
`config.toml`, offer to augment (add another provider alongside the existing
ones) or replace, and respect whichever choice you make.

## What it does NOT do

- It does not ask for `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET` or require
  you to register your own OAuth client.
- It does not silently fall back to a different provider if verification
  fails.
- It does not write config fields you did not approve.
- It does not require you to edit `.appctl/config.toml` by hand.

## See also

- [Installation → Configure a provider](/docs/installation/#configure-a-provider)
- [Provider matrix](/docs/provider-matrix/)
- [`appctl auth`](/docs/cli/auth/)
- [`appctl app`](/docs/cli/app/) — register this project as a named context.
