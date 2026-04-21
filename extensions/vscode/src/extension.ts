import * as vscode from "vscode";
import WebSocket from "ws";

interface HistoryEntry {
  id: string;
  prompt: string;
  response: string;
  created_at: string;
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
        case "history":
          await this.loadHistory();
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

  private config() {
    const cfg = vscode.workspace.getConfiguration("appctl");
    return {
      serverUrl: cfg.get<string>("serverUrl", "http://127.0.0.1:7878"),
      token: cfg.get<string>("token", ""),
    };
  }

  private wsUrl(): string {
    const { serverUrl } = this.config();
    const url = new URL("/chat", serverUrl);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    return url.toString();
  }

  private connectSocket() {
    const headers: Record<string, string> = {};
    const { token } = this.config();
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
        const parsed = JSON.parse(data.toString());
        this.post({ type: "message", payload: parsed });
      } catch {
        this.post({ type: "message", payload: { raw: data.toString() } });
      }
    });
  }

  private async sendPrompt(text: string) {
    if (!text.trim()) return;
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) {
      this.post({ type: "status", status: "reconnecting" });
      this.connectSocket();
      // Fall back to the HTTP /run endpoint while the socket is setting up.
      await this.sendHttp(text);
      return;
    }
    this.socket.send(text);
    this.post({ type: "echo", text });
  }

  private async sendHttp(text: string) {
    const { serverUrl, token } = this.config();
    try {
      const res = await fetch(new URL("/run", serverUrl).toString(), {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          ...(token ? { Authorization: `Bearer ${token}` } : {}),
        },
        body: JSON.stringify({ message: text }),
      });
      const body = await res.json();
      this.post({ type: "message", payload: body });
    } catch (err) {
      this.post({
        type: "status",
        status: "error",
        message: String(err),
      });
    }
  }

  private async loadHistory() {
    const { serverUrl, token } = this.config();
    try {
      const res = await fetch(
        new URL("/history?limit=20", serverUrl).toString(),
        {
          headers: token ? { Authorization: `Bearer ${token}` } : {},
        }
      );
      const entries = (await res.json()) as HistoryEntry[];
      this.post({ type: "history", entries });
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
      .me { color: var(--vscode-textLink-foreground); }
      .agent { color: var(--vscode-foreground); }
      .tool { color: var(--vscode-textPreformat-foreground); font-family: var(--vscode-editor-font-family); white-space: pre-wrap; }
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
      <button id="history">History</button>
    </div>
    <script nonce="${nonce}">
      const vscode = acquireVsCodeApi();
      const log = document.getElementById('log');
      const status = document.getElementById('status');
      const prompt = document.getElementById('prompt');
      document.getElementById('send').addEventListener('click', send);
      document.getElementById('history').addEventListener('click', () => vscode.postMessage({ type: 'history' }));
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
        } else if (msg.type === 'message') {
          const payload = msg.payload;
          if (payload && payload.error) {
            append('agent', '[error] ' + payload.error);
          } else if (payload && payload.result) {
            append('agent', typeof payload.result === 'string' ? payload.result : JSON.stringify(payload.result, null, 2));
          } else if (payload && payload.tool) {
            append('tool', '[tool] ' + JSON.stringify(payload, null, 2));
          } else {
            append('agent', JSON.stringify(payload, null, 2));
          }
        } else if (msg.type === 'history') {
          append('agent', '-- history --');
          for (const entry of msg.entries ?? []) {
            append('me', entry.prompt);
            append('agent', entry.response);
          }
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
  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(
      ChatViewProvider.viewType,
      provider
    ),
    vscode.commands.registerCommand("appctl.openChat", () => provider.focus()),
    vscode.commands.registerCommand("appctl.reload", () => provider.reconnect())
  );
}

export function deactivate() {}
