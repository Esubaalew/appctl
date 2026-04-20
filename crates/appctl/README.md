# appctl

> One command. Any app. Full AI control.

`appctl` is a Rust CLI that introspects an application — via an OpenAPI spec,
Django source, a SQL schema, a live URL, an MCP server, Rails, Laravel,
ASP.NET, Strapi, Supabase, or any dynamic `appctl-plugin-sdk` plugin — and
exposes it to any LLM as a first-class, sandboxed tool layer.

```bash
cargo install appctl

appctl sync --openapi https://api.example.com/openapi.json
appctl chat
```

See the [project README](https://github.com/esubaalew/appctl) for full
documentation, architecture notes, and the list of supported sync sources.

## Features

- Provider-agnostic LLM layer (OpenAI-compatible, NVIDIA NIM, OpenRouter,
  local endpoints).
- Audit log of every tool call in a local SQLite database.
- OAuth2 PKCE login with OS-keychain-persisted refresh tokens.
- HTTP + WebSocket daemon (`appctl serve`) used by the companion VS Code
  extension.
- Dynamic plugins loaded from `~/.appctl/plugins/` (see the
  [`appctl-plugin-sdk`](https://crates.io/crates/appctl-plugin-sdk) crate).

## License

MIT — see [`LICENSE`](https://github.com/esubaalew/appctl/blob/main/LICENSE).
