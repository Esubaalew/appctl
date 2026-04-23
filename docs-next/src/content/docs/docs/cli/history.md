---
title: appctl history
description: Replay the agent's audit log — every plan, call, and observation.
---

`appctl history` prints the audit log stored in `.appctl/history.db` (SQLite). Every
tool call ever issued by `appctl chat`, `appctl run`, or `appctl serve` against
the current app directory is there with full arguments, response, and status.

## Usage

```bash
appctl history [OPTIONS]
```

## Options

| Flag | Default | What it does |
| --- | --- | --- |
| `--last <N>` | `20` | Print the most recent N entries. |
| `--undo <ID>` | — | Attempt to reverse the mutation with this history id, if the original tool declared an inverse. |

## What an entry looks like

Each row of the SQLite table is rendered as a block with:

- timestamp (UTC, ISO-8601)
- tool name and `op` (`http`, `sql`, `mcp`, `plugin`, ...)
- safety mode that was active (`read_only`, `dry_run`, `confirm`, `strict`)
- status (`success` / `error`)
- full JSON arguments
- truncated JSON response (up to the executor's cap)

The same table is what the Web UI's **History** tab reads and what the VS Code
extension replays.

## Undo

`--undo <ID>` only works when the original tool declared an inverse during
sync. For example, the Django source emits `users.create` with an inverse
pointing at `users.delete`. Sources that do not declare inverses will error out
on undo. Most mutating tools today do **not** declare inverses.

## Examples

```bash
# Last 10 tool calls
appctl history --last 10

# Print everything in the last session (use your app's real N)
appctl history --last 500

# Undo a specific mutation (requires an inverse)
appctl history --undo 42
```

## Reading history from code

`appctl serve` exposes the same log at `GET /history?limit=<N>`. See
[HTTP endpoints](/docs/api/http/).

## Related

- [`appctl serve`](/docs/cli/serve/) — view history in the Web UI.
- [Agent loop](/docs/concepts/agent-loop/) — what each row represents.
- [Provenance and safety](/docs/concepts/provenance-and-safety/) — safety
  flags recorded per call.
