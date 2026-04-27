---
title: Introduction
description: What appctl is and how it relates to your application and LLM.
---

> Talk to your app. In plain English.

`appctl` is a CLI that reads a machine-readable description of an application
(OpenAPI, ORM, SQL `information_schema`, framework layout, etc.), writes
`.appctl/schema.json` and `.appctl/tools.json`, and at runtime dispatches
tool calls from the model to your HTTP endpoints or database according to that
schema. Call history is stored locally (e.g. `.appctl/history.db`).

## Primary commands

Start with [`appctl setup`](/docs/cli/setup/) if you want the guided path.

1. [`appctl setup`](/docs/cli/setup/) — provider, sync source, checks, and next steps in one flow.
2. [`appctl sync`](/docs/cli/sync/) — generate the schema and tools from one selected source. If `.appctl/schema.json` already exists, [`--force`](/docs/cli/sync/#when-to-use-force) is required to replace it.
3. [`appctl chat`](/docs/cli/chat/) or [`appctl run`](/docs/cli/run/) — send user text; the model may emit tool calls which `appctl` executes.
4. [`appctl serve`](/docs/cli/serve/) — expose HTTP/WebSocket and the embedded web console.

## What appctl is not

- A web framework: your application is unchanged; `appctl` only consumes what it can read.
- A database: data remains in your systems.
- An LLM product: you configure [providers](/docs/installation/) in `config.toml`.

## Sync sources (summary)

Not sure which flag to use? Start with [Choosing a sync source](/docs/sources/choosing-a-sync-source/) (Nest, Spring, Next.js, and other stacks without a built-in flag).

| Source | Documentation |
| --- | --- |
| OpenAPI | [OpenAPI](/docs/sources/openapi/) |
| Django (DRF), Flask | [Django](/docs/sources/django/), [`appctl sync --flask`](/docs/cli/sync/#examples) |
| Rails, Laravel, ASP.NET, Strapi | [Rails](/docs/sources/rails/), [Laravel](/docs/sources/laravel/), [ASP.NET](/docs/sources/aspnet/), [Strapi](/docs/sources/strapi/) |
| Supabase / PostgREST | [Supabase](/docs/sources/supabase/) |
| SQL and other datastores | [Databases](/docs/sources/db/) |
| URL + login | [URL login](/docs/sources/url/) |
| MCP | [MCP](/docs/sources/mcp/) |
| Dynamic plugins | [Plugins](/docs/sources/plugins/) |

## See also

- [Installation](/docs/installation/)
- [First 10 minutes](/docs/first-10-minutes/)
- [Quickstart](/docs/quickstart/)
- [Mental model](/docs/concepts/mental-model/)
