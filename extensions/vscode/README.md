# appctl for VS Code

Chat and tool-trace panel for the [`appctl`](https://github.com/Esubaalew/appctl) CLI (sync your API or DB, then call tools from the editor).

## Features

- Chat with the `appctl` agent directly from VS Code.
- Live tool-call traces (HTTP / SQL / plugin calls) streamed over WebSocket.
- View recent sessions from the active project's `.appctl/history.db` through the running daemon.

## Requirements

1. Install the `appctl` CLI:

   ```bash
   cargo install appctl
   ```

2. Sync an app:

   ```bash
   appctl sync --openapi ./openapi.json
   ```

3. Run the daemon:

   ```bash
   appctl serve --port 4242
   ```

## Configuration

- `appctl.serverUrl` — base URL of the `appctl serve` process (default `http://127.0.0.1:4242`).
- `appctl.token` — optional bearer token.

## Commands

- **appctl: Open Chat** — focus the chat panel.
- **appctl: Reconnect** — reconnect the WebSocket to the daemon.
