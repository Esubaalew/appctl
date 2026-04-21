---
title: appctl history
description: Inspect or undo past tool calls stored in .appctl/history.db.
---

Inspect or undo past tool calls recorded in `.appctl/history.db`.

## Usage

```
appctl history [OPTIONS]
```

## Options

- `--last <N>` — print the last `N` entries (default `20`).
- `--undo <ID>` — attempt to reverse tool call with the given id.

## Examples

List recent calls:

```bash
appctl history --last 10
```

Undo a specific call:

```bash
appctl history --undo 47
```

`--undo` works only when the original tool has a reversible counterpart the runtime can derive (for example, a `create_*` call writes the created id into history, and the undo issues a matching `delete_*`). If there is no inverse, `--undo` reports `not_reversible` and makes no changes.

## Direct SQLite access

The store is plain SQLite:

```bash
sqlite3 .appctl/history.db 'select ts, tool, status from tool_calls order by ts desc limit 5;'
```

Rows include the prompt, the chosen tool, arguments, HTTP status, and which provider/model produced the response.

## Related

- [Agent loop](/docs/concepts/agent-loop/)
- [AgentEvent stream](/docs/api/agent-events/)
