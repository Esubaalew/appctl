import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";

const TOKEN_KEY = "appctl_token";

type Tab = "chat" | "tools" | "history" | "settings";

type AgentEvent =
  | { kind: "user_prompt"; text: string }
  | { kind: "assistant_delta"; text: string }
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

type PublicConfig = {
  default_provider?: string;
  sync_source?: string;
  base_url?: string | null;
  read_only?: boolean;
  dry_run?: boolean;
  strict?: boolean;
  confirm_default?: boolean;
};

function authHeaders(token: string): HeadersInit {
  const h: Record<string, string> = {};
  if (token) {
    h["Authorization"] = `Bearer ${token}`;
    h["x-appctl-token"] = token;
  }
  return h;
}

function TabButton({
  id,
  active,
  onClick,
  children,
}: {
  id: Tab;
  active: Tab;
  onClick: (t: Tab) => void;
  children: ReactNode;
}) {
  const on = id === active;
  return (
    <button
      type="button"
      onClick={() => onClick(id)}
      className={`rounded-md px-3 py-1.5 text-sm font-medium transition ${
        on
          ? "bg-accent/20 text-accent"
          : "text-muted hover:bg-panel hover:text-fg"
      }`}
    >
      {children}
    </button>
  );
}

function StatusBadge({ label, on }: { label: string; on: boolean }) {
  if (!on) return null;
  return (
    <span className="rounded border border-border bg-panel px-2 py-0.5 text-xs text-accent2">
      {label}
    </span>
  );
}

export default function App() {
  const [tab, setTab] = useState<Tab>("chat");
  const [token, setToken] = useState(() => localStorage.getItem(TOKEN_KEY) ?? "");
  const [readOnly, setReadOnly] = useState(false);
  const [dryRun, setDryRun] = useState(false);
  const [publicCfg, setPublicCfg] = useState<PublicConfig | null>(null);
  const [toolsJson, setToolsJson] = useState<unknown>(null);
  const [historyJson, setHistoryJson] = useState<unknown>(null);
  const [chatLog, setChatLog] = useState<{ role: string; body: string }[]>([]);
  const [input, setInput] = useState("");
  const [wsStatus, setWsStatus] = useState<"idle" | "open" | "closed" | "err">("idle");
  const wsRef = useRef<WebSocket | null>(null);

  const saveToken = useCallback((t: string) => {
    setToken(t);
    if (t) localStorage.setItem(TOKEN_KEY, t);
    else localStorage.removeItem(TOKEN_KEY);
  }, []);

  const fetchCfg = useCallback(async () => {
    try {
      const r = await fetch("/config/public", { headers: authHeaders(token) });
      if (r.ok) setPublicCfg((await r.json()) as PublicConfig);
    } catch {
      setPublicCfg(null);
    }
  }, [token]);

  const fetchTools = useCallback(async () => {
    try {
      const r = await fetch("/tools", { headers: authHeaders(token) });
      if (r.ok) setToolsJson(await r.json());
    } catch {
      setToolsJson(null);
    }
  }, [token]);

  const fetchHistory = useCallback(async () => {
    try {
      const r = await fetch("/history?limit=50", { headers: authHeaders(token) });
      if (r.ok) setHistoryJson(await r.json());
    } catch {
      setHistoryJson(null);
    }
  }, [token]);

  useEffect(() => {
    void fetchCfg();
  }, [fetchCfg]);

  useEffect(() => {
    if (tab === "tools") void fetchTools();
    if (tab === "history") void fetchHistory();
  }, [tab, fetchTools, fetchHistory]);

  const wsUrl = useMemo(() => {
    const u = new URL("/chat", window.location.origin);
    u.protocol = u.protocol === "https:" ? "wss:" : "ws:";
    if (token) u.searchParams.set("token", token);
    return u.toString();
  }, [token]);

  const connectWs = useCallback(() => {
    wsRef.current?.close();
    try {
      const ws = new WebSocket(wsUrl);
      wsRef.current = ws;
      ws.onopen = () => setWsStatus("open");
      ws.onclose = () => setWsStatus("closed");
      ws.onerror = () => setWsStatus("err");
      ws.onmessage = (ev) => {
        try {
          const evp = JSON.parse(String(ev.data)) as AgentEvent;
          if (evp.kind === "user_prompt") {
            setChatLog((l) => [...l, { role: "you", body: evp.text }]);
          } else if (evp.kind === "assistant_message" || evp.kind === "assistant_delta") {
            const text = "text" in evp ? evp.text : "";
            setChatLog((l) => {
              const last = l[l.length - 1];
              if (last?.role === "assistant" && evp.kind === "assistant_delta") {
                return [...l.slice(0, -1), { role: "assistant", body: last.body + text }];
              }
              if (evp.kind === "assistant_delta") {
                return [...l, { role: "assistant", body: text }];
              }
              return [...l, { role: "assistant", body: text }];
            });
          } else if (evp.kind === "tool_call") {
            setChatLog((l) => [
              ...l,
              {
                role: "tool",
                body: `call ${evp.name} ${JSON.stringify(evp.arguments, null, 2)}`,
              },
            ]);
          } else if (evp.kind === "tool_result") {
            setChatLog((l) => [
              ...l,
              {
                role: "tool",
                body: `result [${evp.status}] ${evp.duration_ms ?? 0}ms\n${JSON.stringify(evp.result, null, 2)}`,
              },
            ]);
          } else if (evp.kind === "error") {
            setChatLog((l) => [...l, { role: "error", body: evp.message }]);
          }
        } catch {
          setChatLog((l) => [...l, { role: "error", body: String(ev.data) }]);
        }
      };
    } catch {
      setWsStatus("err");
    }
  }, [wsUrl]);

  useEffect(() => {
    connectWs();
    return () => wsRef.current?.close();
  }, [connectWs]);

  const sendChat = () => {
    const text = input.trim();
    if (!text) return;
    const payload = JSON.stringify({
      message: text,
      read_only: readOnly,
      dry_run: dryRun,
    });
    const ws = wsRef.current;
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(payload);
    } else {
      void (async () => {
        try {
          const r = await fetch("/run", {
            method: "POST",
            headers: { "Content-Type": "application/json", ...authHeaders(token) },
            body: JSON.stringify({
              message: text,
              read_only: readOnly,
              dry_run: dryRun,
            }),
          });
          const body = (await r.json()) as { result?: unknown; events?: unknown[]; error?: string };
          if (body.error) {
            setChatLog((l) => [...l, { role: "error", body: body.error! }]);
            return;
          }
          if (Array.isArray(body.events)) {
            for (const raw of body.events) {
              const evp = raw as AgentEvent;
              if (evp.kind === "assistant_message") {
                setChatLog((l) => [...l, { role: "assistant", body: evp.text }]);
              }
            }
          }
          setChatLog((l) => [
            ...l,
            {
              role: "assistant",
              body:
                typeof body.result === "string"
                  ? body.result
                  : JSON.stringify(body.result, null, 2),
            },
          ]);
        } catch (e) {
          setChatLog((l) => [...l, { role: "error", body: String(e) }]);
        }
      })();
    }
    setInput("");
  };

  return (
    <div className="flex min-h-screen flex-col font-sans">
      <header className="flex flex-wrap items-center gap-3 border-b border-border bg-panel px-4 py-3">
        <span className="text-lg font-semibold tracking-tight text-fg">appctl</span>
        <nav className="flex flex-wrap gap-1">
          <TabButton id="chat" active={tab} onClick={setTab}>
            Chat
          </TabButton>
          <TabButton id="tools" active={tab} onClick={setTab}>
            Tools
          </TabButton>
          <TabButton id="history" active={tab} onClick={setTab}>
            History
          </TabButton>
          <TabButton id="settings" active={tab} onClick={setTab}>
            Settings
          </TabButton>
        </nav>
        <div className="ml-auto flex flex-wrap items-center gap-2 text-xs text-muted">
          <span>
            ws:{" "}
            <span className={wsStatus === "open" ? "text-accent2" : "text-muted"}>{wsStatus}</span>
          </span>
          {publicCfg?.default_provider != null && (
            <span className="text-fg">provider: {publicCfg.default_provider}</span>
          )}
          <StatusBadge label="read-only" on={readOnly} />
          <StatusBadge label="dry-run" on={dryRun} />
        </div>
      </header>

      <main className="flex flex-1 flex-col p-4">
        {tab === "chat" && (
          <div className="flex flex-1 flex-col gap-3">
            <div className="flex flex-wrap gap-4 text-sm">
              <label className="flex cursor-pointer items-center gap-2 text-muted">
                <input
                  type="checkbox"
                  checked={readOnly}
                  onChange={(e) => setReadOnly(e.target.checked)}
                />
                Read-only
              </label>
              <label className="flex cursor-pointer items-center gap-2 text-muted">
                <input
                  type="checkbox"
                  checked={dryRun}
                  onChange={(e) => setDryRun(e.target.checked)}
                />
                Dry-run
              </label>
              <button
                type="button"
                className="rounded border border-border px-2 py-1 text-fg hover:bg-code"
                onClick={connectWs}
              >
                Reconnect WS
              </button>
            </div>
            <div className="flex flex-1 flex-col overflow-hidden rounded-lg border border-border bg-code">
              <div className="flex-1 overflow-y-auto p-3 font-mono text-sm">
                {chatLog.length === 0 && (
                  <p className="text-muted">Send a message. Events stream over WebSocket when connected.</p>
                )}
                {chatLog.map((line, i) => (
                  <div key={i} className="mb-3 whitespace-pre-wrap break-words">
                    <span
                      className={
                        line.role === "you"
                          ? "text-accent"
                          : line.role === "error"
                            ? "text-red-400"
                            : line.role === "tool"
                              ? "text-accent2"
                              : "text-fg"
                      }
                    >
                      {line.role === "you" ? "you › " : line.role === "tool" ? "tool › " : ""}
                    </span>
                    {line.body}
                  </div>
                ))}
              </div>
              <div className="flex gap-2 border-t border-border p-2">
                <textarea
                  className="min-h-[48px] flex-1 resize-y rounded border border-border bg-bg px-2 py-1 font-sans text-fg"
                  placeholder="Ask in plain English…"
                  value={input}
                  onChange={(e) => setInput(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.shiftKey) {
                      e.preventDefault();
                      sendChat();
                    }
                  }}
                />
                <button
                  type="button"
                  className="rounded bg-accent px-4 py-2 text-sm font-medium text-bg"
                  onClick={sendChat}
                >
                  Send
                </button>
              </div>
            </div>
          </div>
        )}

        {tab === "tools" && (
          <div className="overflow-auto rounded-lg border border-border bg-code p-3 font-mono text-xs text-fg">
            <pre>{toolsJson ? JSON.stringify(toolsJson, null, 2) : "Loading or unauthorized…"}</pre>
          </div>
        )}

        {tab === "history" && (
          <div className="overflow-auto rounded-lg border border-border bg-code p-3 font-mono text-xs text-fg">
            <pre>{historyJson ? JSON.stringify(historyJson, null, 2) : "Loading or unauthorized…"}</pre>
          </div>
        )}

        {tab === "settings" && (
          <div className="max-w-lg space-y-4 text-sm">
            <p className="text-muted">
              Token is sent as <code className="text-accent">Authorization: Bearer</code> and{" "}
              <code className="text-accent">x-appctl-token</code>. Matches <code>appctl serve --token</code>.
            </p>
            <label className="block">
              <span className="text-muted">Token</span>
              <input
                type="password"
                className="mt-1 w-full rounded border border-border bg-bg px-2 py-1 text-fg"
                value={token}
                onChange={(e) => saveToken(e.target.value)}
                placeholder="optional"
              />
            </label>
            <button
              type="button"
              className="rounded border border-border px-3 py-1 text-fg hover:bg-panel"
              onClick={() => {
                void fetchCfg();
                void fetchTools();
                void fetchHistory();
              }}
            >
              Refresh config
            </button>
            <div className="rounded border border-border bg-panel p-3 font-mono text-xs">
              <pre>{publicCfg ? JSON.stringify(publicCfg, null, 2) : "{}"}</pre>
            </div>
            <p className="text-muted">
              CLI config file: <code className="text-fg">.appctl/config.toml</code> in your project (not
              editable from here).
            </p>
          </div>
        )}
      </main>
    </div>
  );
}
