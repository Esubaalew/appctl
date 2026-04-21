---
title: MCP servers
description: Connect appctl to any MCP server URL and expose its tools.
---

Connect `appctl` to an MCP server that exposes tools. Any MCP tool becomes a tool the agent can call.

## Prerequisites

- An MCP server URL. `appctl sync --mcp` expects an HTTP URL that speaks MCP over WebSocket or SSE. Pure stdio MCP servers need a small URL wrapper.
- `appctl` installed.

## Using a URL-based MCP server

```bash
appctl sync --mcp ws://localhost:5555/mcp --force
```

The sync calls `tools/list`, creates one `appctl` tool per MCP tool, and stores the URL so the agent can later send `tools/call`.

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

- Pure stdio MCP servers (the most common kind) are not supported directly. Wrap them with a small HTTP or WebSocket shim.
- Only the `tools` portion of the MCP spec is imported. Resources and prompts are not.

## See also

- [`appctl sync`](/docs/cli/sync/)
