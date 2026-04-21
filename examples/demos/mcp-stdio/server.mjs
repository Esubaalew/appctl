#!/usr/bin/env node
/**
 * Minimal MCP server over stdio.
 *
 * Implements the MCP JSON-RPC protocol:
 *   initialize  → returns capabilities and protocol version
 *   tools/list  → returns two demo tools
 *   tools/call  → echoes the call back as text
 *
 * Run:  node server.mjs
 * Then pipe JSON-RPC messages to stdin, one per line.
 */
import readline from "node:readline";

const TOOLS = [
  {
    name: "echo",
    description: "Echoes back the text you send.",
    inputSchema: {
      type: "object",
      properties: {
        text: { type: "string", description: "Text to echo" },
      },
      required: ["text"],
    },
  },
  {
    name: "add",
    description: "Adds two numbers together.",
    inputSchema: {
      type: "object",
      properties: {
        a: { type: "number" },
        b: { type: "number" },
      },
      required: ["a", "b"],
    },
  },
];

function reply(id, result) {
  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id, result }) + "\n");
}

function error(id, code, message) {
  process.stdout.write(
    JSON.stringify({ jsonrpc: "2.0", id, error: { code, message } }) + "\n"
  );
}

const rl = readline.createInterface({ input: process.stdin });

rl.on("line", (line) => {
  let msg;
  try {
    msg = JSON.parse(line.trim());
  } catch {
    return;
  }

  const { id, method, params } = msg;

  if (method === "initialize") {
    reply(id, {
      protocolVersion: "2024-11-05",
      capabilities: { tools: {} },
      serverInfo: { name: "appctl-mcp-demo", version: "1.0.0" },
    });
    return;
  }

  if (method === "tools/list") {
    reply(id, { tools: TOOLS });
    return;
  }

  if (method === "tools/call") {
    const name = params?.name;
    const args = params?.arguments ?? {};
    if (name === "echo") {
      reply(id, { content: [{ type: "text", text: String(args.text ?? "") }] });
      return;
    }
    if (name === "add") {
      const sum = Number(args.a ?? 0) + Number(args.b ?? 0);
      reply(id, { content: [{ type: "text", text: String(sum) }] });
      return;
    }
    error(id, -32601, `unknown tool: ${name}`);
    return;
  }

  // Notifications (no id) are silently ignored.
  if (id !== undefined) {
    error(id, -32601, `method not found: ${method}`);
  }
});
