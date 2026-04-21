---
title: Changelog
description: Version history for appctl. Mirrors CHANGELOG.md.
---

Mirrors [`CHANGELOG.md`](https://github.com/Esubaalew/appctl/blob/main/CHANGELOG.md) in the repository. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versioning follows [SemVer](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] — 2026-04-21

### Added

- **`appctl doctor`** probes HTTP tools (HEAD/OPTIONS/GET) and prints a reachability table. `--write` marks successful routes as `Provenance::Verified` in the schema.
- **`Provenance`** on actions (`Inferred`, `Declared`, `Verified`) exposed via `appctl-plugin-sdk`.
- **`--strict`** for `chat`, `run`, and `serve`. Blocks HTTP tools that are still `Inferred`, with guidance pointing at `appctl doctor --write`.
- **`AgentEvent`** stream from `run_agent`, consumed by the terminal renderer, `POST /run`, and WebSocket `/chat`.
- Pretty terminal rendering (`termimad`, spinners, tool cards) in `chat` and `run`.
- **`appctl serve`**: `GET /schema`, `GET /config/public`, `POST /run` returns `{ result, events }`, `/chat` streams JSON `AgentEvent` messages. Static web UI is embedded via `rust-embed`. Token auth works via `x-appctl-token`, `Authorization`, or `?token=`.
- **`appctl config set-secret`** stores API keys in the OS keychain.
- Bundled **Vite + React + TypeScript + Tailwind** web UI under `web/` (Chat, Tools, History, Settings).
- **VS Code extension** with Tools and History tree views, status bar, `AgentEvent` rendering, and commands for the web UI, settings, and sync.
- **Design tokens** in `docs/design-tokens.css`, shared with the web UI and the VS Code webviews.
- **Docs**: "Will my app work?" matrix, per-source pages under `docs/sources/`, deploy and embed guide at `docs/deploy/`.
- **Demos** under `examples/demos/` for every supported source (OpenAPI/FastAPI, Django+DRF, Postgres, Rails, Laravel, ASP.NET, Strapi, Supabase, URL login, MCP stub).
- **E2E tests**: `e2e_openapi_fastapi`, `e2e_django_drf`, optional Postgres and NVIDIA NIM smoke.
- **CI** with a web build job, a demo validation job, and an optional NVIDIA NIM workflow when the secret is configured.

### Changed

- **Django / Rails / Laravel / ASP.NET** sync is more honest about missing API surfaces (clearer warnings, fewer phantom tools).
- **`build_headers`**: inline `auth_header` from sync metadata overrides schema bearer/env requirements.

### Fixed

- Phantom `/api/...` tools on plain Django projects without DRF; HTTP tools are now omitted with an explicit metadata warning.
- `appctl doctor` URL construction now puts a slash between base URL and path, resolving "invalid port number" errors on non-root paths.
- `--supabase` sync probes both `/rest/v1` (hosted Supabase layout) and `/` (bare PostgREST), so local PostgREST instances work without flags.
- Rails sync generates plural API paths (`/api/v1/posts`) instead of singular (`/api/v1/post`).

## [0.1.0] — TBD

First tagged release.
