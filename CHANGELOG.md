# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-04-21

### Added

- **`appctl doctor`** — probes HTTP tools (HEAD/OPTIONS/GET), prints a table; **`--write`** marks successful routes as **`Provenance::Verified`** in the schema.
- **`Provenance`** on actions (`Inferred`, `Declared`, `Verified`) in **`appctl-plugin-sdk`**.
- **`--strict`** for `chat`, `run`, and `serve` — blocks HTTP tools that are still **`Inferred`** (points to `appctl doctor --write`).
- **`AgentEvent`** stream from **`run_agent`** (terminal, **`POST /run`**, WebSocket **`/chat`**).
- Pretty terminal rendering (**`termimad`**, spinners, tool cards) in **`chat`** / **`run`**.
- **`appctl serve`** — **`GET /schema`**, **`GET /config/public`**, **`POST /run`** returns **`{ result, events }`**, **`/chat`** streams JSON **`AgentEvent`** messages; static UI from **`rust-embed`** (`web/dist`); token auth via **`x-appctl-token`**, **`Authorization`**, or **`?token=`**.
- **`appctl config set-secret`** — store API keys in the OS keychain.
- Bundled **Vite + React + TypeScript + Tailwind** web UI under **`web/`** (Chat, Tools, History, Settings).
- **VS Code extension** — Tools and History tree views, status bar, **`AgentEvent`** rendering, commands for web UI / settings / sync hint.
- **Design tokens** — **`docs/design-tokens.css`** shared with docs and web UI variables.
- **Docs** — “Will my app work?” matrix, per-source pages under **`docs/sources/`**, deploy/embed guide **`docs/deploy/`**.
- **Demos** under **`examples/demos/`** (OpenAPI/FastAPI, Django+DRF, Postgres, Rails, Laravel, ASP.NET, Strapi, Supabase, URL login, MCP stub).
- **E2E tests** — `e2e_openapi_fastapi`, `e2e_django_drf`, optional Postgres (`e2e_db_postgres`, ignored), optional NVIDIA NIM smoke (`e2e_nvidia_nim`, ignored).
- **CI** — web build/lint/typecheck job; Rust jobs build **`web/dist`** first; optional **`.github/workflows/e2e-nvidia.yml`** when **`NVIDIA_API_KEY`** is configured.

### Changed

- **Django / Rails / Laravel / ASP.NET** sync is more honest about missing API surfaces (warnings, fewer phantom tools).
- **`build_headers`**: inline **`auth_header`** from sync metadata overrides schema bearer/env requirements (fixes `--auth-header` with Django’s default bearer hint).

### Fixed

- Phantom **`/api/...`** tools on plain Django without DRF — HTTP tools are omitted with a clear metadata warning.

## [0.1.0] - TBD

_First tagged release._
