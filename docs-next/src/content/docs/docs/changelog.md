---
title: Changelog
description: Version history for appctl. Mirrors the latest tagged releases.
---

Mirrors [`CHANGELOG.md`](https://github.com/Esubaalew/appctl/blob/main/CHANGELOG.md) in the repository. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versioning follows [SemVer](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **SQL tools:** Quote identifiers in generated SQL for SQLite, PostgreSQL, and MySQL so table names like `order` (reserved word) no longer cause syntax errors.

### Added

- **`appctl serve`:** Optional `--no-open` (default is to open the local UI in the default browser), env overrides `APPCTL_PORT` / `APPCTL_BIND`, and port `0` for an OS-assigned ephemeral port.
- **Agent system prompt:** A fixed “This app” block (registry name, display label, optional `description`, `.appctl` path, sync source) so the model always knows which project it is in.
- **`app add`:** `--display-name` and `--description`; optional positional `NAME` must come **after** those flags (Clap rule).

### Changed

- **`appctl sync --db`:** If `[target] database_url` is unset, the sync connection string is written to config so DB tools work without hand-editing.

## [0.7.0] — 2026-04-23

### Added

- `appctl sync --watch` for OpenAPI sources, plus `--doctor-write` to verify routes after sync.
- Named chat sessions via `appctl chat --session <name>`, with session labels in history and the web Activity panel.
- `appctl run --json` for machine-readable one-shot output.
- `appctl app add --openapi ...` to register and sync in one step.
- Native `appctl sync --flask .` support.
- Datastore support for MongoDB, Redis, Firestore, and DynamoDB under `sync --db` (and SQLite for SQL workflows).
- Tool pinning, aliasing, and runtime tool policy in app config.
- `appctl serve --identity-header` and `--tunnel`.

### Changed

- Cookie/session auth from URL-login syncs is persisted across invocations.
- Runtime target resolution can read the base URL from config or environment (`target.base_url_env` / `APPCTL_BASE_URL`).
- The web history panel shows session labels alongside session ids.
- CI and release automation use current Node.js and action major versions aligned with the main workflow.

## [0.6.0] — 2026-04-23

### Fixed

- Harden tool diagnostics, improve app context flow, and clean up release docs.

## [0.5.0] — 2026-04-23

### Added

- `init`, `app`, and `doctor models` were wired into the real CLI surface.

## [0.4.0] — 2026-04-23

Operator-console redesign and an honest rewrite of the CLI reference.

### Added

- **Web operator console** — full visual overhaul of the embedded web UI. Pitch-black monochrome palette, inline assistant messages, collapsed-by-default tool cards, data-table history panel, sectioned settings panel (usage, providers, auth, app). Prompt suggestions are generated from the synced schema and the active app name.
- **App-name context** — `serve` exposes the app label on `GET /config/public`; the web console renders it next to the provider in the top bar.
- **Library surface** extended: `run_chat`, `run_once`, `run_server`, and `run_agent` take the app name so downstream UIs can display session context.

### Changed

- **Documentation theme parity** — `docs-next` now uses the same design tokens as the operator console: pure-black surfaces, white accent, tight hairlines, flat buttons, terminal-styled code frames.
- **CLI reference rewritten** — `chat`, `run`, `doctor`, `serve`, `history`, `config`, `auth`, `provider-matrix`, `installation`, and `security` pages match the real command surface, real slash commands, real endpoints, real output, and real auth flows.
- **Provider matrix** is now presentational rather than apologetic: two columns (Direct API vs MCP bridge), with a dedicated section for signing in with a ChatGPT/Claude consumer subscription via the MCP bridge.
- **Internal content links** across the docs are now base-path aware via a small `remark` plugin, so cross-page links resolve correctly when the site is served under a project-site prefix (for example `/appctl/` on GitHub Pages).

### Fixed

- Removed stub documentation for unimplemented commands (`appctl init`, `appctl app`). The CLI never exposed them; the pages linked to 404s.
- Defensive "what is not faked" / "what it does NOT do" sections on `auth` and `doctor` rewritten into user-facing alternatives and scope statements.
- Corrected `history.md` and `quickstart.md` references from `history.sqlite` to the actual SQLite filename `history.db`.
- Corrected `--confirm` semantics everywhere: on `chat`/`run` it auto-approves mutations (default is interactive prompt on TTY); on `serve` it defaults to on.
- `cargo clippy -D warnings` clean across the workspace.
- `serve` embeds the latest `web/dist` bundle so the crates.io tarball ships the new console without a separate build step.

### Release

- Removed the premature `require-live-providers` release gate from `release-plz.yml`. `live-providers` stays in the repo as an informational nightly workflow.

## [0.3.0] — 2026-04-21

Provider authentication, MCP serve, and onboarding polish.

### Added

- **Provider authentication** — `appctl auth provider login|status|logout|list` and a full `ProviderAuthConfig` model covering API key, OAuth2 (PKCE), Google Application Default Credentials, Azure AD device code, Qwen OAuth, and MCP bridge flows.
- **New providers** — first-class `Vertex AI` (`kind = "vertex"`, Google ADC) and `Azure OpenAI` (`kind = "azure_open_ai"`, API key or Azure AD) transports alongside existing OpenAI-compatible, Anthropic, and Google GenAI kinds.
- **`appctl mcp serve`** — expose the local schema and tools to an external agent client (Codex CLI, Claude Code, Qwen Code, Gemini CLI) over MCP, enabling subscription-backed billing via the bridge.
- **Astro/Starlight docs** replace the previous static site.
- **UI onboarding** — `appctl config init` produces a working `.appctl/config.toml`; `appctl config provider-sample --preset <name>` prints paste-ready `[[provider]]` blocks for every supported kind.
- **Verification** — `verified` flag on `ProviderConfig`; `help_url` and `bridge_client` hints on `ProviderAuthStatus`.

### Changed

- Schema layout: `.appctl/schema.json`, `.appctl/tools.json`, and `.appctl/history.db` are the canonical on-disk artifacts per app directory.
- `--app-dir` resolution is consistent across `sync`, `chat`, `run`, `doctor`, `history`, and `serve`.

### Fixed

- Identity leaks in CLI rendering — the assistant no longer reveals the underlying model or provider in introductions.
- Terminal rendering — the CLI wraps the whole streamed response in one frame instead of per-chunk separators.

## [0.2.1] — 2026-04-20

Polish release focused on demos and documentation accuracy.

### Fixed

- Real demo apps in `examples/demos/` run end-to-end against the documented commands.
- Rewrote the "Will my app work?" matrix so every row has a tested source adapter.
- Assorted documentation typos and broken links.

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

- **Django / Rails / Laravel / ASP.NET** sync is clearer about missing API surfaces (better warnings, fewer phantom tools).
- **`build_headers`**: inline `auth_header` from sync metadata overrides schema bearer/env requirements.

### Fixed

- Phantom `/api/...` tools on plain Django projects without DRF; HTTP tools are now omitted with an explicit metadata warning.
- `appctl doctor` URL construction now puts a slash between base URL and path, resolving "invalid port number" errors on non-root paths.
- `--supabase` sync probes both `/rest/v1` (hosted Supabase layout) and `/` (bare PostgREST), so local PostgREST instances work without flags.
- Rails sync generates plural API paths (`/api/v1/posts`) instead of singular (`/api/v1/post`).

## [0.1.0] — 2026-04-20

First tagged release. Plugin SDK, OpenAPI/Swagger sync, OAuth2, OS-keychain secret storage, VS Code extension baseline.
