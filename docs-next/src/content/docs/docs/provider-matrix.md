---
title: Provider matrix
description: Official auth paths, billing expectations, and the recommended way to connect each model provider.
---

Pick the provider path that matches how you plan to pay and where you want credentials to live.

| Provider | Native kind | Official auth in appctl | Subscription covers appctl usage | Recommended path |
| --- | --- | --- | --- | --- |
| Gemini API | `google_genai` | OAuth2 or Google ADC | No. Consumer chat subscriptions are separate from API usage. | `appctl auth provider login gemini` or `auth = { kind = "google_adc" }` |
| Vertex AI Gemini | `google_genai` | Google ADC | No. Billed through Google Cloud. | `auth = { kind = "google_adc" }` |
| Qwen via DashScope / Coding Plan | `open_ai_compatible` | API key | No. Chat subscriptions and API billing are separate. | `appctl config provider-sample --preset qwen` plus `config set-secret` |
| Anthropic Claude | `anthropic` | API key | No. Claude consumer plans do not cover API calls. | `appctl config provider-sample --preset claude` plus `config set-secret` |
| OpenAI and compatible gateways | `open_ai_compatible` | API key | No. ChatGPT subscriptions do not cover API calls. | `appctl config provider-sample --preset openai` plus `config set-secret` |
| Ollama | `open_ai_compatible` | None | Local only | `appctl config provider-sample --preset ollama` |

## Using your own client subscription

If you want to keep model usage inside another client such as Gemini CLI, Qwen Code, Claude Code, or Codex, expose your synced app through [`appctl mcp serve`](/docs/sources/mcp/). That path lets the external client keep model auth and billing while `appctl` supplies the tools.

## Notes

- `api_key_ref` remains supported for older configs, but new samples use the additive `auth` block.
- Provider OAuth tokens are stored separately from target-app OAuth tokens.
- The web UI shows redacted provider status, expiry, and recovery hints, but tokens remain local to the host running `appctl`.
