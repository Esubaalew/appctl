---
title: Introduction
description: What appctl is, what it is not, and why you might want it.
---

`appctl` is a Rust CLI that turns an existing application into a set of typed tools an LLM can call. One command gets you an agent that understands your app, uses your endpoints, and logs every action.

## The three commands you actually use

1. [`appctl sync`](/docs/cli/sync/) — point it at your app. It reads your
   routes or schema and writes a local contract to `.appctl/schema.json` plus
   a flattened `.appctl/tools.json`.
2. [`appctl chat`](/docs/cli/chat/) — talk to the agent in plain English. It
   picks the right tool, runs it, and logs the result.
3. [`appctl serve`](/docs/cli/serve/) — run it as an HTTP + WebSocket daemon
   with the embedded web console for teammates, IDE plugins, or custom
   frontends.

## What it is not

- It is not a framework. You do not rewrite your app.
- It is not a new database. Your data stays where it is.
- It is not an LLM. You bring your own (OpenAI, Anthropic, Google, xAI, Mistral, Ollama, any OpenAI-compatible endpoint).
- It is not magic. It reads routes and schemas that already exist. If the information is not there, the tools will be wrong.

## When to use it

- You have an internal app and want teammates to drive it in natural language.
- You have a SaaS with an OpenAPI spec and want an agent that can take action, not just answer questions.
- You have a legacy HTML admin with a form login and no API.
- You want MCP support without rebuilding everything as MCP servers.

## Supported sources

| Source | Reads | Output |
| --- | --- | --- |
| [OpenAPI / Swagger](/docs/sources/openapi/) | spec document | one tool per operation |
| [Django (DRF)](/docs/sources/django/) | `models.py`, `settings.py` | five tools per model |
| [Rails API](/docs/sources/rails/) | `routes.rb`, `schema.rb` | five tools per resource |
| [Laravel API](/docs/sources/laravel/) | `routes/api.php`, migrations | five tools per resource |
| [ASP.NET Core](/docs/sources/aspnet/) | swagger.json or controllers | tools per endpoint |
| [Strapi v4](/docs/sources/strapi/) | content-type schemas | five tools per content type |
| [Supabase / PostgREST](/docs/sources/supabase/) | OpenAPI from PostgREST | REST tools per table |
| [SQL databases](/docs/sources/db/) | information_schema | five SQL tools per table |
| [URL login](/docs/sources/url/) | HTML forms, links | tools per form |
| [MCP servers](/docs/sources/mcp/) | `tools/list` | one tool per MCP tool |
| [Plugins](/docs/sources/plugins/) | your Rust `cdylib` | anything |

## Next

- [Installation](/docs/installation/): get `appctl` on your machine.
- [Quickstart](/docs/quickstart/): run a demo app end-to-end in five minutes.
- [Mental model](/docs/concepts/mental-model/): how the pieces fit together.
