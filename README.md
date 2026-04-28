# appctl

> Talk to your app. In plain English.

Command-line tool: introspect an HTTP API, database, or supported application
codebase, write a tool contract to `.appctl/`, and execute the tools your
configured language model requests (HTTP, SQL, and related transports).

**Documentation:** <https://esubaalew.github.io/appctl>  
**Repository:** <https://github.com/Esubaalew/appctl>

## Install

```bash
cargo install appctl
```

Build from a clone, or install with a custom web UI bundle: see
[Installation](https://esubaalew.github.io/appctl/docs/installation/).

## Commands (overview)

| Command | Purpose |
| --- | --- |
| `appctl setup` | Guided first-run flow: provider, sync source, checks, and next steps. |
| `appctl init` | Create `.appctl/config.toml` and store provider secrets. |
| `appctl sync` | Generate `.appctl/schema.json` and `tools.json` from a source (e.g. `--openapi`, `--django`, `--db`). |
| `appctl chat` / `appctl run` | Send a prompt; the model may call tools via `appctl`. |
| `appctl serve` | HTTP and WebSocket API plus bundled web UI. |

```bash
appctl setup
appctl chat
```

Run `setup` from the app/project folder you want to control. It creates or reuses
that folder's `.appctl/`, guides provider setup, syncs tools, verifies the target
API, and tells you exactly how to open the terminal or web console.

For protected APIs, prefer environment-backed target auth:

```bash
export API_TOKEN="..."
appctl setup
# Auth header prompt:
# Authorization: Bearer env:API_TOKEN
```

Advanced manual setup is still available:

```bash
appctl init
appctl sync --openapi https://api.example.com/openapi.json --base-url https://api.example.com
appctl doctor --write
```

Full CLI reference, sync sources, providers, `serve`, and plugins are covered
in the [documentation](https://esubaalew.github.io/appctl/docs/introduction/).

## License

MIT © [Esubalew](https://esubalew.dev)
