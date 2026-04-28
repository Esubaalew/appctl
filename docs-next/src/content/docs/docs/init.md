---
title: appctl init
description: Interactive first-time setup for providers, secrets, and verification.
---

For most users, start with [`appctl setup`](/docs/cli/setup/). It calls the same
provider setup and then continues through sync, doctor, and first chat/web
guidance.

`appctl init` is the lower-level interactive way to configure only the **AI
provider**. It writes provider settings to `.appctl/config.toml`, stores model
provider secrets in the OS keychain, runs the real auth flow, and ends with a
live verify call before printing `done`. If the verify call fails, the config is
still written but the provider is marked
`verified = false` so later commands can warn you instead of silently using a
broken setup.

It does not sync tools, verify your target API, or manage the target app's user
session. For those, run [`appctl setup`](/docs/cli/setup/) or `appctl sync` /
`appctl auth target ...`.

## Usage

```bash
appctl init
```

Run it from the project root you want to control. `init` will create
`.appctl/config.toml` in the current directory unless `--app-dir` is passed.
If a parent folder already contains `.appctl/`, `init` reuses that app
directory instead of creating a second nested one.

## What it walks you through

1. **Detect existing config.** If `.appctl/config.toml` already exists, it
   asks whether to replace or augment it so you can add a second provider
   without losing the first.
2. **Pick a provider path.** The menu currently offers concrete choices:
   - Vertex AI via Google ADC.
   - Gemini API key.
   - Guided OpenAI-compatible setup (OpenRouter, NVIDIA NIM, custom base URLs).
   - Local OpenAI-compatible setup (Ollama, LM Studio, vLLM, llama.cpp).
   - Anthropic API key.
   - Qwen DashScope API key.
   - Azure OpenAI API key.
   - MCP subscription bridges (Codex, Claude Code, Qwen Code, Gemini CLI).
3. **Run the real flow for that path.**
   - **Google ADC (Vertex):** shells out to
     `gcloud auth application-default login`. Your default browser opens, you
     sign in, and `gcloud` owns the token cache. `appctl` reads fresh access
     tokens on demand, never stores them.
   - **API key:** prompts for the key, stores it in the keychain under a
     short `secret_ref` name under the `appctl` service, and records the provider's help URL so later
     error messages can point users back to the key-issuance page.
   - **MCP bridge:** writes an `appctl` entry into the external client's
     config file (for example `~/.codex/config.toml`) with a timestamped
     backup of the original file.
4. **Pick a model.** For providers that expose a list endpoint, `init` pulls
   the actual catalogue for your account and shows a searchable selector
   (type to filter). For providers without a list endpoint, `init` falls back
   to a curated default or asks for the exact deployment / model id.
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
ones) or replace, and respect whichever choice you make. It will also offer to
refresh the matching global app registration if one already exists.

## See also

- [First 10 minutes](/docs/first-10-minutes/)
- [`appctl setup`](/docs/cli/setup/)
- [Installation → Configure a provider manually](/docs/installation/#configure-a-provider-manually)
- [Provider matrix](/docs/provider-matrix/)
- [`appctl auth`](/docs/cli/auth/)
- [`appctl app`](/docs/cli/app/) — register this project as a named context.
