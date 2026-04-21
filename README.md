# appctl

> One command. Any app. Full AI control.

`appctl` syncs any application into a local tool layer, then lets an LLM drive
it. Point it at an OpenAPI spec, a database, a Rails / Laravel / ASP.NET /
Strapi / Supabase project, a Django codebase, an MCP server, or a live URL,
and you get a sandboxed agent for that app.

**Setup is CLI. Daily use is a conversation.** A dev runs `sync`, `auth login`,
and `serve` once. After that, end users type plain English into `appctl chat`,
the VS Code panel, or any UI that speaks to `appctl serve` &mdash; the LLM
picks the right tool under the hood. Nobody memorises CRUD commands.

- Site: <https://esubaalew.github.io/appctl>
- Repo: <https://github.com/esubaalew/appctl>
- Author: [Esubalew](https://esubalew.dev)

## Install

From [crates.io](https://crates.io/crates/appctl) (after the maintainer publishes the release):

```bash
cargo install appctl
```

From this repository (exact version, no crates.io wait):

```bash
cargo install --locked --git https://github.com/esubaalew/appctl.git --tag v0.2.0
```

To build the embedded web UI the same way CI does (needed for a clean `cargo install` from a working tree):

```bash
cd web && npm ci && npm run build && cd ..
cargo install --locked --path crates/appctl
```

## Usage

```bash
# Pick any source:
appctl sync --openapi https://api.example.com/openapi.json
appctl sync --rails ./my-rails-app
appctl sync --laravel ./my-laravel-app
appctl sync --aspnet ./MyAspApp
appctl sync --strapi ./cms
appctl sync --supabase https://xyz.supabase.co
appctl sync --django ./my-django
appctl sync --db postgresql://localhost/mydb
appctl sync --url https://myapp.com --login-url /login --login-user me@x --login-password pw
appctl sync --plugin airtable        # dynamic plugin from ~/.appctl/plugins/

# Talk to it:
appctl chat
appctl run "Add a user named John"
appctl history --last 20
appctl serve --port 4242             # HTTP + WebSocket daemon + bundled web UI
appctl auth login github --client-id ... --auth-url ... --token-url ...
```

## LLM providers

Works with anything OpenAI-compatible and Anthropic natively:
OpenAI, OpenRouter, NVIDIA NIM, Groq, Together, Fireworks, Ollama, LM Studio,
vLLM, LiteLLM, plus Anthropic.

Configure in `.appctl/config.toml`; secrets live in the OS keychain (fallback
to env vars).

## Plugins

Build a `cdylib` against [`appctl-plugin-sdk`](https://crates.io/crates/appctl-plugin-sdk)
and drop it into `~/.appctl/plugins/`, or run:

```bash
appctl plugin install <path|crate|git-url>
appctl plugin list
```

See [`examples/plugins/appctl-airtable`](examples/plugins/appctl-airtable) for
a reference.

## VS Code

Chat panel + tool traces over WebSocket — see
[`extensions/vscode`](extensions/vscode). Build with
`npm ci && npm run compile && npx vsce package`.

## Safety

- `--read-only` blocks writes.
- `--dry-run` previews without executing.
- Mutations prompt by default; `--confirm` auto-approves.
- Every tool call is logged to `.appctl/history.db` (SQLite).

## Repo layout

```
crates/appctl              # CLI + library
crates/appctl-plugin-sdk   # stable schema + C ABI for plugins
examples/plugins/*         # reference dynamic plugins
extensions/vscode          # VS Code extension
```

## Development

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Release process: see [RELEASING.md](RELEASING.md). Changelog:
[CHANGELOG.md](CHANGELOG.md).

## License

MIT © [Esubalew](https://esubalew.dev)
