---
title: Security
description: Threat model, safety flags, and the controls appctl gives you.
---

`appctl` is a privileged client of your own app. It uses your credentials and calls your endpoints. Everything an attacker could do with your key, they could do through `appctl`.

## Threat model

- `appctl` runs on a trusted host you control.
- The LLM provider is trusted with the conversation and tool results, but not with secrets. API keys never leave your machine.
- Tool targets (your app, your database) are trusted to enforce their own authorization. `appctl` cannot bypass it.
- A malicious prompt is treated as untrusted input. Safety flags restrict what the agent can do in response.

## Controls

### Token auth on the daemon

For any `appctl serve` reachable beyond `127.0.0.1`, always set `--token`. Clients must present the token on every request.

### Safety flags

| Flag | Effect |
| --- | --- |
| `--read-only` | Every mutating tool is rejected before the HTTP call. |
| `--dry-run` | The agent plans the call, the executor short-circuits before real I/O. |
| `--confirm` | Auto-approves mutations (default on `chat` / `run` is to prompt interactively; default on `serve` is **on**). |
| `--strict` | Blocks `provenance = "inferred"` tools until `appctl doctor --write` promotes them to `verified`. |

Pick the tightest combination that still gets the job done.

### Provenance

Only trust `declared` (the source published the contract) and `verified` (a live call confirmed it). `inferred` tools may target routes that do not exist.

Run [`appctl doctor --write`](/docs/cli/doctor/) after every sync in production.

### Upstream RBAC

The agent uses your credentials. It cannot do anything your API would not let the same key do. Use narrowly scoped keys; make a dedicated `appctl` role if your database supports it.

### Audit log

`.appctl/history.db` records every tool call with arguments, status, provider, and timestamp. Ship it to your SIEM for retention beyond a project's lifetime.

### Secrets

API keys and OAuth tokens live in the OS keychain. Environment variables override at runtime. See [Secrets and keys](/docs/deploy/secrets-and-keys/).

## Recommended posture

- Dev laptop: `appctl chat` with default prompting for mutations (do **not** pass `--confirm`).
- Team-internal `serve`: `--token`, `--strict`, `--confirm=false` if you want every mutation to wait for an explicit client approval.
- Production `serve` behind TLS: `--token`, `--strict`; run a separate `--read-only` instance for viewers.
- Embedding in a customer-facing product: `--token`, `--strict`, `--read-only`; do not expose a write endpoint without a second authorization layer.

## Reporting vulnerabilities

Please open a GitHub security advisory at [Esubaalew/appctl/security/advisories](https://github.com/Esubaalew/appctl/security/advisories/new) rather than a public issue.
