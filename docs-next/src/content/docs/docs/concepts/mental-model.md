---
title: Mental model
description: How sync, schema, tools, and the agent loop fit together.
---

Four moving parts. Once you have them straight, everything else makes sense.

## 1. Your app

Already running. `appctl` never modifies it. It talks to it the same way a browser or a curl command does.

## 2. The schema

A local file at `.appctl/schema.json`. It describes the tools the agent can call: name, HTTP method, path template, parameter JSON Schema, auth strategy, safety level, and provenance.

The schema is produced by `appctl sync`. Nothing else writes it.

## 3. The tool runtime

When the agent wants to call a tool, `appctl` looks it up in the schema, fills in arguments, makes the HTTP or SQL call, and returns the response. It also enforces safety (read-only mode, dry-run, confirmation prompts).

## 4. The agent loop

A loop that alternates between the LLM and the tool runtime. The LLM sees a conversation plus the list of tools. It picks a tool and arguments. `appctl` runs it. The result goes back to the LLM. Repeat until the LLM emits a final answer or hits the iteration cap.

## Data flow

```
┌─────────┐     sync     ┌──────────────┐
│ Your app│ ───────────► │schema.json   │
└─────────┘              └──────┬───────┘
                                │
                                ▼
                    ┌──────────────────────┐
           prompt → │   agent loop         │
                    │ (LLM  ⇄  tool calls) │ → response
                    └──────────┬───────────┘
                               │ HTTP / SQL
                               ▼
                         ┌─────────┐
                         │ Your app│
                         └─────────┘
```

## Trust levels

Each tool has a `provenance` field:

- `declared` — the source told `appctl` this tool exists (OpenAPI spec, Django model, MCP `tools/list`). High trust.
- `inferred` — we guessed from static files (a Rails route, a controller scan). Medium trust.
- `verified` — a live call confirmed the route is reachable. Highest trust.

The `--strict` flag blocks inferred tools until `appctl doctor --write` marks them verified. See [Provenance and safety](/docs/concepts/provenance-and-safety/).

## Where each piece lives

| Piece | Location |
| --- | --- |
| Schema | `.appctl/schema.json` |
| History | `.appctl/history.db` (SQLite) |
| Config | `.appctl/config.toml` |
| Secrets | OS keychain (service `appctl`) |
| Plugins | `~/.appctl/plugins/` |

## Next

- [Sync and schema](/docs/concepts/sync-and-schema/)
- [Tools and actions](/docs/concepts/tools-and-actions/)
- [Agent loop](/docs/concepts/agent-loop/)
