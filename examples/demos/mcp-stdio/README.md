# Demo: MCP (stdio stub)

This directory contains a **minimal** Node script for manual experiments. For production MCP integration, run a compliant MCP server and point `appctl sync --mcp` at it per CLI help.

```bash
npm install
node server.mjs
```

Use `appctl sync --mcp <url>` when your MCP server is reachable over the transport appctl expects (see main docs).
