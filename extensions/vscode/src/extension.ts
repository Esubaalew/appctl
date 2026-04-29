import * as vscode from "vscode";
import WebSocket from "ws";

/** Matches `crates/appctl/src/events.rs` JSON shape. */
type AgentEvent =
  | { kind: "user_prompt"; text: string }
  | { kind: "assistant_delta"; text: string }
  | { kind: "assistant_thought_delta"; text: string }
  | { kind: "assistant_thought"; text: string }
  | { kind: "assistant_message"; text: string }
  | { kind: "tool_call"; id: string; name: string; arguments: unknown }
  | {
      kind: "tool_result";
      id: string;
      result: unknown;
      status: "ok" | "error";
      duration_ms?: number;
    }
  | { kind: "error"; message: string }
  | { kind: "done" };

type ToolDef = {
  type?: string;
  function?: { name?: string; description?: string };
};

type ServerHistoryEntry = {
  id: number;
  tool: string;
  status: string;
  ts?: string;
  session_id?: string;
};

function serverConfig() {
  const cfg = vscode.workspace.getConfiguration("appctl");
  return {
    serverUrl: cfg.get<string>("serverUrl", "http://127.0.0.1:4242"),
    token: cfg.get<string>("token", ""),
  };
}

function authHeaders(token: string): Record<string, string> {
  const h: Record<string, string> = {};
  if (token) {
    h["Authorization"] = `Bearer ${token}`;
    h["x-appctl-token"] = token;
  }
  return h;
}

class ToolsTreeProvider implements vscode.TreeDataProvider<vscode.TreeItem> {
  private _onDidChange = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this._onDidChange.event;

  refresh() {
    this._onDidChange.fire();
  }

  getTreeItem(element: vscode.TreeItem): vscode.TreeItem {
    return element;
  }

  async getChildren(): Promise<vscode.TreeItem[]> {
    const { serverUrl, token } = serverConfig();
    try {
      const res = await fetch(new URL("/tools", serverUrl).toString(), {
        headers: authHeaders(token),
      });
      if (!res.ok) {
        const err = new vscode.TreeItem("Unauthorized or no tools");
        err.description = String(res.status);
        return [err];
      }
      const data = (await res.json()) as ToolDef[];
      if (!Array.isArray(data)) return [new vscode.TreeItem("Invalid /tools response")];
      return data.map((t) => {
        const name = t.function?.name ?? "(anonymous)";
        const item = new vscode.TreeItem(name, vscode.TreeItemCollapsibleState.None);
        item.description = t.function?.description?.slice(0, 80);
        item.tooltip = new vscode.MarkdownString(
          `\`\`\`json\n${JSON.stringify(t, null, 2).slice(0, 4000)}\n\`\`\``
        );
        return item;
      });
    } catch (e) {
      const item = new vscode.TreeItem("Failed to load tools");
      item.description = String(e);
      return [item];
    }
  }
}

class HistoryTreeProvider implements vscode.TreeDataProvider<vscode.TreeItem> {
  private _onDidChange = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this._onDidChange.event;

  refresh() {
    this._onDidChange.fire();
  }

  getTreeItem(element: vscode.TreeItem): vscode.TreeItem {
    return element;
  }

  async getChildren(): Promise<vscode.TreeItem[]> {
    const { serverUrl, token } = serverConfig();
    try {
      const res = await fetch(new URL("/history?limit=30", serverUrl).toString(), {
        headers: authHeaders(token),
      });
      if (!res.ok) {
        const err = new vscode.TreeItem("Unauthorized or no history");
        err.description = String(res.status);
        return [err];
      }
      const data = (await res.json()) as ServerHistoryEntry[];
      if (!Array.isArray(data)) return [new vscode.TreeItem("Invalid /history response")];
      return data.map((row) => {
        const item = new vscode.TreeItem(
          `#${row.id} ${row.tool}`,
          vscode.TreeItemCollapsibleState.None
        );
        item.description = row.status;
        item.tooltip = new vscode.MarkdownString(
          `**${row.tool}** · ${row.status}\n\n\`\`\`json\n${JSON.stringify(row, null, 2).slice(0, 3500)}\n\`\`\``
        );
        return item;
      });
    } catch (e) {
      const item = new vscode.TreeItem("Failed to load history");
      item.description = String(e);
      return [item];
    }
  }
}

class ChatViewProvider implements vscode.WebviewViewProvider {
  public static readonly viewType = "appctl.chat";

  private view?: vscode.WebviewView;
  private socket?: WebSocket;

  constructor(private readonly context: vscode.ExtensionContext) {}

  resolveWebviewView(webviewView: vscode.WebviewView): void {
    this.view = webviewView;
    webviewView.webview.options = {
      enableScripts: true,
      localResourceRoots: [this.context.extensionUri],
    };
    webviewView.webview.html = this.render(webviewView.webview);

    webviewView.webview.onDidReceiveMessage(async (msg) => {
      switch (msg.type) {
        case "send":
          await this.sendPrompt(String(msg.text ?? ""));
          break;
        case "reconnect":
          this.connectSocket();
          break;
      }
    });

    this.connectSocket();
  }

  public focus(): void {
    this.view?.show?.(true);
  }

  public reconnect(): void {
    this.socket?.close();
    this.connectSocket();
  }

  private wsUrl(): string {
    const { serverUrl } = serverConfig();
    const url = new URL("/chat", serverUrl);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    const { token } = serverConfig();
    if (token) url.searchParams.set("token", token);
    return url.toString();
  }

  private connectSocket() {
    const headers: Record<string, string> = {};
    const { token } = serverConfig();
    if (token) headers["Authorization"] = `Bearer ${token}`;

    try {
      this.socket?.removeAllListeners();
      this.socket?.close();
    } catch {
      /* ignore */
    }

    const ws = new WebSocket(this.wsUrl(), { headers });
    this.socket = ws;

    ws.on("open", () => this.post({ type: "status", status: "connected" }));
    ws.on("close", () => this.post({ type: "status", status: "disconnected" }));
    ws.on("error", (err) =>
      this.post({ type: "status", status: "error", message: err.message })
    );
    ws.on("message", (data) => {
      try {
        const parsed = JSON.parse(data.toString()) as AgentEvent;
        this.post({ type: "agent_event", payload: parsed });
      } catch {
        this.post({ type: "message", payload: { raw: data.toString() } });
      }
    });
  }

  private async sendPrompt(text: string) {
    if (!text.trim()) return;
    const body = JSON.stringify({ message: text });
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) {
      this.post({ type: "status", status: "reconnecting" });
      this.connectSocket();
      await this.sendHttp(text);
      return;
    }
    this.socket.send(body);
    this.post({ type: "echo", text });
  }

  private async sendHttp(text: string) {
    const { serverUrl, token } = serverConfig();
    try {
      const res = await fetch(new URL("/run", serverUrl).toString(), {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          ...authHeaders(token),
        },
        body: JSON.stringify({ message: text }),
      });
      const body = (await res.json()) as {
        result?: unknown;
        events?: AgentEvent[];
        error?: string;
      };
      if (body.error) {
        this.post({ type: "status", status: "error", message: body.error });
        return;
      }
      if (Array.isArray(body.events)) {
        for (const ev of body.events) {
          this.post({ type: "agent_event", payload: ev });
        }
      }
      this.post({
        type: "agent_event",
        payload: { kind: "assistant_message", text: JSON.stringify(body.result, null, 2) },
      });
    } catch (err) {
      this.post({
        type: "status",
        status: "error",
        message: String(err),
      });
    }
  }

  private post(msg: unknown) {
    this.view?.webview.postMessage(msg);
  }

  private render(webview: vscode.Webview): string {
    const nonce = randomNonce();
    const csp = `default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}'; connect-src *;`;
    return /* html */ `<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta http-equiv="Content-Security-Policy" content="${csp}" />
    <title>appctl</title>
    <style>
      :root {
        --appctl-bg: #0b0d10;
        --appctl-fg: #e7ecf2;
        --appctl-muted: #8a94a4;
        --appctl-accent: #7cc4ff;
        --appctl-accent2: #a8ff9a;
        --appctl-panel: #141820;
        --appctl-border: #222833;
      }
      body {
        font-family: var(--vscode-font-family);
        color: var(--vscode-foreground);
        background: var(--vscode-sideBar-background);
        margin: 0;
        display: flex;
        flex-direction: column;
        height: 100vh;
      }
      #status {
        padding: 4px 8px;
        font-size: 11px;
        background: var(--vscode-editorWidget-background);
        border-bottom: 1px solid var(--vscode-panel-border);
      }
      #log {
        flex: 1;
        overflow-y: auto;
        padding: 8px;
        font-family: var(--vscode-editor-font-family);
        font-size: var(--vscode-editor-font-size);
      }
      .msg { margin-bottom: 12px; }
      .me { color: var(--appctl-accent); }
      .agent { color: var(--vscode-editor-foreground); }
      .tool {
        color: var(--appctl-accent2);
        font-family: var(--vscode-editor-font-family);
        white-space: pre-wrap;
        border-left: 2px solid var(--appctl-border);
        padding-left: 8px;
        margin: 6px 0;
      }
      .err { color: var(--vscode-errorForeground); }
      #input {
        display: flex;
        padding: 8px;
        border-top: 1px solid var(--vscode-panel-border);
        gap: 6px;
      }
      textarea {
        flex: 1;
        background: var(--vscode-input-background);
        color: var(--vscode-input-foreground);
        border: 1px solid var(--vscode-input-border);
        padding: 4px 6px;
        font-family: var(--vscode-editor-font-family);
        resize: vertical;
        min-height: 42px;
      }
      button {
        background: var(--vscode-button-background);
        color: var(--vscode-button-foreground);
        border: none;
        padding: 4px 10px;
        cursor: pointer;
      }
      button:hover { background: var(--vscode-button-hoverBackground); }
    </style>
  </head>
  <body>
    <div id="status">disconnected</div>
    <div id="log"></div>
    <div id="input">
      <textarea id="prompt" placeholder="Ask appctl anything..."></textarea>
      <button id="send">Send</button>
      <button id="reconnect">Reconnect</button>
    </div>
    <script nonce="${nonce}">
      const vscode = acquireVsCodeApi();
      const log = document.getElementById('log');
      const status = document.getElementById('status');
      const prompt = document.getElementById('prompt');
      document.getElementById('send').addEventListener('click', send);
      document.getElementById('reconnect').addEventListener('click', () => vscode.postMessage({ type: 'reconnect' }));
      prompt.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' && !e.shiftKey) {
          e.preventDefault();
          send();
        }
      });
      function send() {
        const text = prompt.value.trim();
        if (!text) return;
        vscode.postMessage({ type: 'send', text });
        prompt.value = '';
      }
      function append(cls, text) {
        const div = document.createElement('div');
        div.className = 'msg ' + cls;
        div.textContent = text;
        log.appendChild(div);
        log.scrollTop = log.scrollHeight;
      }
      window.addEventListener('message', (event) => {
        const msg = event.data;
        if (msg.type === 'status') {
          status.textContent = msg.status + (msg.message ? ': ' + msg.message : '');
        } else if (msg.type === 'echo') {
          append('me', '> ' + msg.text);
        } else if (msg.type === 'agent_event') {
          const ev = msg.payload;
          if (!ev || !ev.kind) {
            append('agent', JSON.stringify(ev));
            return;
          }
          if (ev.kind === 'user_prompt') append('me', ev.text);
          else if (ev.kind === 'assistant_message' || ev.kind === 'assistant_delta')
            append('agent', ev.text || '');
          else if (ev.kind === 'assistant_thought_delta' || ev.kind === 'assistant_thought')
            append('tool', 'thinking…');
          else if (ev.kind === 'tool_call')
            append('tool', 'tool ' + ev.name + ' ' + JSON.stringify(ev.arguments, null, 2));
          else if (ev.kind === 'tool_result')
            append(
              'tool',
              'result [' + ev.status + '] ' + (ev.duration_ms || 0) + 'ms\\n' + JSON.stringify(ev.result, null, 2)
            );
          else if (ev.kind === 'error') append('err', ev.message);
        } else if (msg.type === 'message') {
          const payload = msg.payload;
          append('agent', JSON.stringify(payload, null, 2));
        }
      });
    </script>
  </body>
</html>`;
  }
}

function randomNonce(): string {
  return Math.random().toString(36).slice(2, 12) + Date.now().toString(36);
}

export function activate(context: vscode.ExtensionContext) {
  const provider = new ChatViewProvider(context);
  const tools = new ToolsTreeProvider();
  const history = new HistoryTreeProvider();

  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  status.command = "appctl.openChat";
  status.text = "$(plug) appctl";
  status.tooltip = "appctl connection";
  status.show();

  async function refreshStatus() {
    const { serverUrl, token } = serverConfig();
    try {
      const res = await fetch(new URL("/config/public", serverUrl).toString(), {
        headers: authHeaders(token),
      });
      if (!res.ok) {
        status.text = "$(alert) appctl";
        status.backgroundColor = new vscode.ThemeColor("statusBarItem.errorBackground");
        return;
      }
      const cfg = (await res.json()) as {
        default_provider?: string;
        read_only?: boolean;
        dry_run?: boolean;
      };
      const bits = [
        "$(plug) " + (cfg.default_provider ?? "appctl"),
        cfg.read_only ? "RO" : null,
        cfg.dry_run ? "DRY" : null,
      ].filter(Boolean);
      status.text = bits.join(" · ");
      status.backgroundColor = undefined;
    } catch {
      status.text = "$(alert) appctl";
      status.backgroundColor = new vscode.ThemeColor("statusBarItem.errorBackground");
    }
  }

  void refreshStatus();
  const interval = setInterval(() => void refreshStatus(), 15000);
  context.subscriptions.push({ dispose: () => clearInterval(interval) });

  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(ChatViewProvider.viewType, provider),
    vscode.window.registerTreeDataProvider("appctl.tools", tools),
    vscode.window.registerTreeDataProvider("appctl.history", history),
    vscode.commands.registerCommand("appctl.openChat", () => provider.focus()),
    vscode.commands.registerCommand("appctl.reload", () => {
      provider.reconnect();
      tools.refresh();
      history.refresh();
      void refreshStatus();
    }),
    vscode.commands.registerCommand("appctl.sync", () => {
      void vscode.window.showInformationMessage(
        "Run appctl sync in a terminal from your project root (see appctl --help)."
      );
    }),
    vscode.commands.registerCommand("appctl.changeProvider", () =>
      vscode.commands.executeCommand("workbench.action.openSettings", "appctl")
    ),
    vscode.commands.registerCommand("appctl.openWebUi", () => {
      const { serverUrl } = serverConfig();
      void vscode.env.openExternal(vscode.Uri.parse(serverUrl));
    }),
    vscode.commands.registerCommand("appctl.refreshTools", () => tools.refresh()),
    vscode.commands.registerCommand("appctl.refreshHistory", () => history.refresh()),
    status
  );
}

export function deactivate() {}
