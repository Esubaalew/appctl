---
title: appctl mcp
description: Expose synced appctl tools as an MCP server over stdio.
---

`appctl mcp serve` turns your synced `.appctl` tool catalog into an MCP stdio server.

## Usage

```
appctl mcp serve [OPTIONS]
```

## Options

- `--read-only`
- `--dry-run`
- `--strict`
- `--confirm`

These map to the same safety controls used by `chat`, `run`, and `serve`.

## What it implements

The stdio server currently supports:

- `initialize`
- `tools/list`
- `tools/call`

Each synced `appctl` tool is exposed as one MCP tool with its JSON Schema input.

## Example

```bash
appctl sync --openapi http://127.0.0.1:8000/openapi.json \
  --base-url http://127.0.0.1:8000 --force

appctl mcp serve --read-only
```

Point Gemini CLI, Qwen Code, Claude Code, Codex, or another MCP-capable client at that stdio command.

## Known limits

- This release is stdio-first. HTTP and SSE transports can be added later.
- Only MCP tools are exposed. Resources and prompts are not.

## Related

- [MCP servers](/docs/sources/mcp/)
- [Provider matrix](/docs/provider-matrix/)
