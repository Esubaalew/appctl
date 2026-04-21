#!/usr/bin/env node
/**
 * Minimal stdio MCP-style echo server for manual testing with appctl --mcp.
 * Not a full MCP implementation — replace with your real MCP server.
 */
import readline from "node:readline";

const rl = readline.createInterface({ input: process.stdin });

rl.on("line", (line) => {
  try {
    const msg = JSON.parse(line);
    if (msg.method === "initialize") {
      process.stdout.write(
        JSON.stringify({
          jsonrpc: "2.0",
          id: msg.id,
          result: { protocolVersion: "2024-11-05", capabilities: {} },
        }) + "\n"
      );
      return;
    }
    process.stdout.write(
      JSON.stringify({
        jsonrpc: "2.0",
        id: msg.id ?? 0,
        result: { content: [{ type: "text", text: "demo-ok" }] },
      }) + "\n"
    );
  } catch {
    /* ignore */
  }
});
