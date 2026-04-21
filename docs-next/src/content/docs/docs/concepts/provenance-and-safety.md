---
title: Provenance and safety
description: How appctl tracks trust in each tool and what to turn on in production.
---

Not every tool is equally trustworthy. `appctl` tags each one with a `provenance` level and gives you safety flags to match.

## Provenance levels

### `declared`

The source itself described the tool.

- OpenAPI operation in the spec.
- Django `ModelViewSet` registered on a `DefaultRouter`.
- MCP `tools/list` entry.
- Strapi content-type schema.

High trust. The source publisher has committed to this contract.

### `inferred`

`appctl` guessed from static files without running anything.

- A Rails `resources :foo` line without a matching controller.
- An ASP.NET `.cs` scan without a swagger document.
- A URL-login form whose target endpoint is never touched.

Medium trust. The tool might not exist.

### `verified`

A live request returned a non-404 response.

Produced by `appctl doctor --write`.

```bash
appctl doctor --write
```

Highest trust. The endpoint definitely exists and is reachable as described.

## Safety flags

| Flag | What it does | Use when |
| --- | --- | --- |
| `--read-only` | removes every mutation tool from the loop | shared or demo environments |
| `--dry-run` | LLM plans calls; runtime fabricates a response | testing or cost control |
| `--confirm` | prompts before each mutation (CLI default) | CLI dev work |
| `--strict` | blocks `inferred` tools until `verified` | production |

## Recommended combinations

- **Dev laptop, trusted app**: no flags. `appctl chat`.
- **Shared serve, internal team**: `--strict --confirm` for ops, `--read-only` for viewers.
- **Customer-facing serve**: `--strict --read-only` by default, open a separate endpoint or token for writes.
- **CI, regression testing**: `--dry-run --strict` to catch contract drift without side effects.

## Audit trail

Every tool call writes to `.appctl/history.db`. Each row records:

- Timestamp
- Tool name
- Arguments
- HTTP status / SQL rowcount
- Provider + model
- User or `serve` client id

Export with `sqlite3`:

```bash
sqlite3 .appctl/history.db 'select ts, tool, status from tool_calls order by ts desc limit 20;'
```

## Next

- [`appctl doctor`](/docs/cli/doctor/) — run verification.
- [Security](/docs/security/) — deployment hardening.
