---
title: Introduction
description: What appctl does and how it connects your application to an LLM.
---

`appctl` gives an LLM a controlled set of tools for your existing application.
It reads an app surface you already have, writes a local tool contract, executes
the tool calls the model requests, and records what happened.

The source can be an OpenAPI document, framework project, SQL
`information_schema`, Supabase/PostgREST service, MCP server, URL login flow, or
plugin. The generated files live under `.appctl/`, so the contract can be
reviewed before the agent uses it.

At runtime, appctl sends your prompt to the configured model provider. If the
model asks to call a tool, appctl performs the HTTP, SQL, MCP, or plugin-backed
operation using your configured auth and safety settings. Call history is stored
locally, for example in `.appctl/history.db`.

## When to use it

Use appctl when you want to operate or inspect a real backend from natural
language without writing a custom agent bridge first. Typical use cases include
internal admin workflows, support operations, reporting, QA checks, demo apps,
and local developer tools.

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
- [Copy-to-clipboard setup prompt for your AI](/#agent-prompt) (project homepage)
