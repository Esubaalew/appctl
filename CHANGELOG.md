# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- SQL tools: double-quote (ANSI) and backtick (MySQL) table/column identifiers so reserved names like `order` do not break SQLite/PostgreSQL/MySQL.

### Added

- `appctl serve`: auto-open the default browser to the local UI (`--no-open` to skip), `APPCTL_PORT` / `APPCTL_BIND` env, port `0` for an ephemeral free port.
- Agent system message includes a “This app” block (registry, label, description, path, sync source).

## [0.4.0] - 2026-04-23

Operator-console redesign and an honest rewrite of the CLI reference.

### Added

- **Web operator console** — full visual overhaul of the embedded web UI. Pitch-black monochrome palette, inline assistant messages, collapsed-by-default tool cards, data-table history panel, sectioned settings panel with usage/provider/auth/app sections. Dynamic prompt suggestions generated from the synced schema and the active app name.
- **App-name context** — `serve` now exposes the app label on `GET /config/public`, and the web console renders it next to the provider in the top bar.
- **`appctl` library surface** extended: `run_chat`, `run_once`, `run_server`, and `run_agent` take the app name so downstream UIs can display session context.

### Changed

- **Documentation theme parity** — `docs-next` (Starlight) now uses the same design tokens as the operator console: pure-black surfaces, white accent, tight hairlines, flat buttons, terminal-styled code frames.
- **CLI reference rewritten** — `chat`, `run`, `doctor`, `serve`, `history`, `config`, `auth`, `provider-matrix`, `installation`, and `security` pages now match the real command surface, real slash commands, real endpoints, real output, and real auth flows.
- **Provider matrix** is now presentational rather than apologetic: two columns (Direct API vs MCP bridge), with a dedicated section for signing in with a ChatGPT/Claude consumer subscription via the MCP bridge.
- **Internal content links** across the docs are now base-path aware via a small `remark` plugin, so cross-page links resolve correctly when the site is served under a project-site prefix (e.g. `/appctl/` on GitHub Pages).

### Fixed

- Removed stub documentation for unimplemented commands (`appctl init`, `appctl app`). The CLI never exposed them; the pages linked to 404s.
- Defensive "what is not faked" / "what it does NOT do" sections on `auth` and `doctor` rewritten into user-facing alternatives and scope statements.
- Corrected `history.md` and `quickstart.md` references from `history.sqlite` to the actual SQLite file name `history.db`.
- Corrected `--confirm` semantics everywhere: on `chat`/`run` it auto-approves mutations (default is interactive prompt on TTY); on `serve` it defaults to on.
- `cargo clippy -D warnings` clean across the workspace (`needless_borrow`, `print_literal`, `format_in_format_args`, `clone_on_copy`, `manual_clamp`, `items_after_test_module`, non-exhaustive auth match).
- `serve` embeds the latest `web/dist` bundle so the crates.io tarball ships the new console without a separate build step.

### Release

- Removed the premature `require-live-providers` release gate from `release-plz.yml`. `live-providers` stays in the repo as an informational nightly workflow.

## [0.3.0] - 2026-04-21

Provider authentication, MCP serve, and onboarding polish.

### Added

- **Provider authentication** — `appctl auth provider login|status|logout|list` and a full `ProviderAuthConfig` model covering API key, OAuth2 (PKCE), Google Application Default Credentials, Azure AD device code, Qwen OAuth, and MCP bridge flows.
- **New providers** — first-class `Vertex AI` (`kind = "vertex"`, Google ADC) and `Azure OpenAI` (`kind = "azure_open_ai"`, API key or Azure AD) transports alongside existing OpenAI-compatible, Anthropic, and Google GenAI kinds.
- **`appctl mcp serve`** — expose the local schema and tools to an external agent client (Codex CLI, Claude Code, Qwen Code, Gemini CLI) over MCP, enabling subscription-backed billing via the bridge.
- **Schema replacement** — `docs-next` Astro/Starlight site replaces the previous static docs.
- **UI onboarding** — `appctl init` / `appctl config init` produces a working `.appctl/config.toml` with a provider preset; `appctl config provider-sample --preset <name>` prints a paste-ready `[[provider]]` block for every supported kind.
- **Verification** — `verified` flag on `ProviderConfig`; `help_url` and `bridge_client` hints on `ProviderAuthStatus`.

### Changed

- Schema layout: `.appctl/schema.json`, `.appctl/tools.json`, and `.appctl/history.db` are now the canonical on-disk artifacts per app directory.
- `--app-dir` resolution is consistent across `sync`, `chat`, `run`, `doctor`, `history`, and `serve`.

### Fixed

- Identity leaks in CLI rendering — the assistant no longer reveals the underlying model or provider in introductions.
- Response rendering in terminal — the CLI now wraps the whole streamed response in one frame instead of per-chunk separators.

## [0.2.1] - 2026-04-20

Polish release focused on demos and documentation accuracy.

### Fixed

- Real demo apps in `examples/demos/` run end-to-end against the documented commands.
- Rewrote the "Will my app work?" matrix so every row has a tested source adapter.
- Assorted documentation typos and broken links.

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
- **Target auth precedence**: active **`[target].oauth_provider`** tokens are tried first, then **`[target].auth_header`**, then sync metadata/schema auth.

### Fixed

- Phantom **`/api/...`** tools on plain Django without DRF — HTTP tools are omitted with a clear metadata warning.

## [0.1.0] - TBD

_First tagged release._
