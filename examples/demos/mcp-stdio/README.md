# mcp-stdio demo

A minimal MCP (Model Context Protocol) server over stdio.
Exposes two tools (`echo` and `add`) using the standard MCP JSON-RPC protocol.

## What's here

```
server.mjs      MCP server: handles initialize, tools/list, tools/call
package.json    declares node >=18, no extra dependencies
```

## How to use with appctl

`appctl sync --mcp <server-url>` registers a single passthrough tool that proxies
all calls to the specified MCP server URL at runtime. It does not introspect the
server's tool list during sync; it generates one `call_remote_mcp_tool` action.

### Manual stdio testing

You can talk to the server directly to verify the protocol:

```sh
node server.mjs
# then type (or pipe) JSON-RPC messages:
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","arguments":{"text":"hello"}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"add","arguments":{"a":3,"b":4}}}
```

### Register with appctl

For a URL-based MCP server (HTTP+SSE or WebSocket transport), run a compatible server
and then:

```sh
appctl sync --mcp http://localhost:9000 --force
appctl chat "call echo with text hello"
```

## Known limits

- This demo uses stdio transport. `appctl sync --mcp` expects a URL (HTTP or WebSocket).
  Use this demo to verify the MCP JSON-RPC protocol; for full end-to-end testing, run
  a server with HTTP transport (e.g., the `@modelcontextprotocol/sdk` Express adapter).
- No authentication. Production MCP servers should require a bearer token.
