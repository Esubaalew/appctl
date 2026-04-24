---
title: Choosing a sync source
description: How to use appctl when there is no dedicated flag for your framework (Nest, Spring, Next.js, etc.).
---

appctl does not add a new CLI flag for every web framework. Long-term, that does not scale: each
stack would need its own parser and tests. What scales is a **small set of machine-readable inputs**
plus an **extension point** (plugins) for special cases.

## Recommended paths

| You have… | Use | Details |
| --- | --- | --- |
| An OpenAPI 2/3 (or Swagger) document | [`appctl sync --openapi`](/docs/sources/openapi/) | [Nest](https://docs.nestjs.com/openapi/introduction) (`@nestjs/swagger`), [Spring / springdoc](https://springdoc.org/), [FastAPI](https://fastapi.tiangolo.com/features/openapi-schema/), [ASP.NET](/docs/sources/aspnet/) (often emits JSON), and many other stacks can expose a spec. Point `--openapi` at a URL or file. |
| Only a database and SQL access | [`appctl sync --db`](/docs/sources/db/) | Introspection from `information_schema` (or equivalent) for [supported engines](/docs/sources/db/). No HTTP layer required. |
| An MCP server (HTTP) | [`appctl sync --mcp`](/docs/sources/mcp/) | [Model Context Protocol](https://modelcontextprotocol.io/) tools map 1:1. Good when the app already exposes an MCP surface or a bridge. |
| Something else | [Dynamic plugin](/docs/sources/plugins/) | Rust `cdylib` that implements the plugin SDK. Use when there is no spec and you need custom discovery. |
| A browser-only or legacy HTML app | [`appctl sync --url`](/docs/sources/url/) | Crawl + forms (limited; prefer an API with OpenAPI when you can). |

**Built-in framework flags** ([Django](/docs/sources/django/), [Rails](/docs/sources/rails/), [Laravel](/docs/sources/laravel/), …) exist for projects that do **not** yet expose OpenAPI: appctl reads project files to build a best-effort contract. When you *can* export OpenAPI, prefer `--openapi` so tools match what the server actually documents.

## Stacks without a dedicated flag (examples)

| Ecosystem | Practical approach |
| --- | --- |
| **Next.js** (App Router, Route Handlers) | Export or generate an OpenAPI document from your API route layer (or a BFF) and `sync --openapi`. If you only have server actions with no spec, use `--db` or `--mcp` if you wrap the backend. |
| **NestJS** | Use `@nestjs/swagger` and the generated JSON/YAML: `--openapi` to that file or URL. |
| **Java / Spring Boot** | Use springdoc (or similar); typically `/v3/api-docs` (often listed automatically when you pass a **root** base URL to `--openapi`—see [OpenAPI](/docs/sources/openapi/#fetching-the-document)). |
| **Ruby (not using the Rails sync)** | If you use a gem that serves OpenAPI, use `--openapi`. Otherwise `--db` or a plugin. |
| **gRPC** | No first-class gRPC sync today; use a sidecar that exposes gRPC or REST, or a plugin. |

## Why this is enough

- **One OpenAPI path** matches how API gateways, gateways, and **clients** (Postman, Insomnia, code generators) already work: contract-first.
- **MCP** is the “bring your own tool server” path that does not require appctl to know your repo layout.
- **Plugins** keep odd integrations out of the core binary while still being first-class in the `sync` command.

## See also

- [OpenAPI / Swagger](/docs/sources/openapi/) — including authenticated fetch and common paths
- [Introduction — sync sources (summary)](/docs/introduction/#sync-sources-summary)
- [`appctl sync`](/docs/cli/sync/)
