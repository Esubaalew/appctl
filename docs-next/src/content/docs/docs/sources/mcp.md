---
title: MCP servers
description: Use appctl as an MCP client or expose synced tools through appctl mcp serve.
---

`appctl` now has two MCP roles:

- `appctl sync --mcp <url>` registers a passthrough action that forwards `tools/call` to a remote MCP server at runtime.
- `appctl mcp serve` exposes your synced `appctl` tools over stdio so Gemini CLI, Qwen Code, Claude Code, Codex, and other MCP clients can use them.

## Prerequisites

- For `sync --mcp`: an MCP server URL that accepts JSON-RPC `tools/call` requests over HTTP.
- For `mcp serve`: a synced `.appctl/schema.json` and `.appctl/tools.json`.
- `appctl` installed.

## Using a remote MCP server

```bash
appctl sync --mcp http://localhost:5555/mcp --force
```

Current behavior is intentionally small: the sync writes one `call_remote_mcp_tool` action and stores the server URL. At runtime, `appctl` forwards `tools/call` to that server.

## Expose synced tools as an MCP server

```bash
appctl sync --openapi http://127.0.0.1:8000/openapi.json \
  --base-url http://127.0.0.1:8000 --force

appctl mcp serve --read-only
```

The stdio server implements:

- `initialize`
- `tools/list`
- `tools/call`

It returns each synced `appctl` tool as an MCP tool, using the same safety checks as the normal executor.

## Connect external clients

- Gemini CLI: point its MCP config at `appctl mcp serve`.
- Qwen Code: point its MCP config at `appctl mcp serve`.
- Claude Code, Codex, and other MCP-capable clients: use the same stdio command.

## The demo in this repo

[`examples/demos/mcp-stdio/`](https://github.com/Esubaalew/appctl/tree/main/examples/demos/mcp-stdio) is a standalone MCP stdio server. It is useful as a protocol conformance check and as example code for writing your own server. Because it speaks stdio and not HTTP, you cannot feed it directly into `appctl sync --mcp`.

### Run it manually

```bash
cd examples/demos/mcp-stdio
node server.mjs < <(printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add","arguments":{"a":3,"b":4}}}')
```

Real output:

```
{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"appctl-mcp-demo","version":"1.0.0"}}}
{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"echo",...},{"name":"add",...}]}}
{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"7"}]}}
```

## Known limits

- `sync --mcp` is still passthrough-only. It does not expand remote `tools/list` into separate `appctl` tools yet.
- `appctl mcp serve` is stdio-first in this release. HTTP and SSE transports can be added later.
- Only the `tools` portion of the MCP spec is handled. Resources and prompts are not.

## See also

- [`appctl sync`](/docs/cli/sync/)
- [`appctl serve`](/docs/cli/serve/)
- [Provider matrix](/docs/provider-matrix/)
