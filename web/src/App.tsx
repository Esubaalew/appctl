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

type ChatEntry = {
  role: "you" | "assistant" | "tool" | "error";
  title: string;
  body: string;
  tone?: "default" | "accent" | "success" | "danger";
};

type PublicConfig = {
  default_provider?: string;
  sync_source?: string;
  base_url?: string | null;
  read_only?: boolean;
  dry_run?: boolean;
  strict?: boolean;
  confirm_default?: boolean;
};

type ToolDef = {
  name: string;
  description: string;
  input_schema?: {
    properties?: Record<string, { type?: string; description?: string }>;
    required?: string[];
  };
};

type SchemaShape = {
  source: string;
  base_url?: string | null;
  auth: { kind: string };
  resources: Resource[];
};

type Resource = {
  name: string;
  description?: string | null;
  fields: Field[];
  actions: Action[];
};

type Field = {
  name: string;
  field_type: string;
  required?: boolean;
  location?: string | null;
};

type Action = {
  name: string;
  description?: string | null;
  verb: string;
  transport: Transport;
  parameters: Field[];
  safety: "read_only" | "mutating" | "destructive";
  provenance?: "inferred" | "declared" | "verified";
  resource?: string | null;
};

type Transport =
  | { kind: "http"; method: string; path: string }
  | { kind: "form"; method: string; action: string }
  | { kind: "sql"; table: string; operation: string; database_kind: string }
  | { kind: "mcp"; server_url: string };

type HistoryEntry = {
  id: number;
  ts: string;
  session_id: string;
  tool: string;
  arguments_json: unknown;
  request_snapshot_json?: unknown;
  response_json?: unknown;
  status: string;
  undone: boolean;
};

function authHeaders(token: string): HeadersInit {
  const headers: Record<string, string> = {};
  if (token) {
    headers.Authorization = `Bearer ${token}`;
    headers["x-appctl-token"] = token;
  }
  return headers;
}

function formatJson(value: unknown): string {
  if (value == null) return "null";
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function previewJson(value: unknown, max = 260): string {
  const rendered = formatJson(value).replace(/\s+/g, " ").trim();
  if (rendered.length <= max) return rendered;
  return `${rendered.slice(0, max - 1)}…`;
}

function formatTs(ts: string): string {
  const date = new Date(ts);
  if (Number.isNaN(date.getTime())) return ts;
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function sourceLabel(source?: string | null): string {
  if (!source) return "Not synced";
  return source.replace(/_/g, " ");
}

function transportLabel(transport: Transport): string {
  switch (transport.kind) {
    case "http":
      return `${transport.method} ${transport.path}`;
    case "form":
      return `${transport.method} ${transport.action}`;
    case "sql":
      return `${transport.database_kind} ${transport.operation} ${transport.table}`;
    case "mcp":
      return `MCP ${transport.server_url}`;
    default:
      return "Unknown transport";
  }
}

function toneForSafety(safety: Action["safety"]): string {
  switch (safety) {
    case "read_only":
      return "border-emerald-400/30 bg-emerald-400/10 text-emerald-200";
    case "mutating":
      return "border-sky-400/30 bg-sky-400/10 text-sky-100";
    case "destructive":
      return "border-rose-400/30 bg-rose-400/10 text-rose-100";
    default:
      return "border-border bg-panel text-fg";
  }
}

function toneForProvenance(provenance?: Action["provenance"]): string {
  switch (provenance) {
    case "verified":
      return "border-emerald-400/30 bg-emerald-400/10 text-emerald-200";
    case "declared":
      return "border-sky-400/30 bg-sky-400/10 text-sky-100";
    default:
      return "border-amber-400/30 bg-amber-400/10 text-amber-100";
  }
}

function promptSuggestions(schema: SchemaShape | null): string[] {
  const firstResource = schema?.resources[0]?.name;
  if (!firstResource) {
    return [
      "Summarize the synced app and tell me which write actions are available.",
      "Show me the riskiest mutating tools and explain when to enable strict mode.",
      "List a few safe example prompts I can use with this app.",
    ];
  }

  return [
    `List the available ${firstResource} records and summarize what matters.`,
    `Create a realistic ${firstResource} example, but explain the exact tool call before acting.`,
    `Audit the ${firstResource} tools and tell me which ones can write or delete data.`,
  ];
}

function matchesAssistantResult(result: unknown, events: unknown[] | undefined): boolean {
  if (!Array.isArray(events)) return false;
  const rendered = formatJson(result);
  return events.some((raw) => {
    const event = raw as AgentEvent;
    return event.kind === "assistant_message" && event.text === rendered;
  });
}

function TabButton({
  id,
  active,
  onClick,
  children,
}: {
  id: Tab;
  active: Tab;
  onClick: (tab: Tab) => void;
  children: string;
}) {
  const selected = id === active;
  return (
    <button
      type="button"
      onClick={() => onClick(id)}
      className={`rounded-full border px-3 py-1.5 text-sm transition ${
        selected
          ? "border-accent bg-accent/15 text-fg shadow-[0_0_0_1px_rgba(124,196,255,0.12)]"
          : "border-border bg-panel/60 text-muted hover:border-accent/40 hover:text-fg"
      }`}
    >
      {children}
    </button>
  );
}

function StatCard({ label, value, hint }: { label: string; value: string; hint: string }) {
  return (
    <div className="rounded-2xl border border-border bg-panel/80 p-4 shadow-[0_16px_48px_-28px_rgba(0,0,0,0.85)]">
      <p className="text-xs uppercase tracking-[0.18em] text-muted">{label}</p>
      <p className="mt-2 text-2xl font-semibold tracking-tight text-fg">{value}</p>
      <p className="mt-1 text-sm text-muted">{hint}</p>
    </div>
  );
}

function SectionShell({
  eyebrow,
  title,
  description,
  children,
}: {
  eyebrow: string;
  title: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <section className="rounded-[28px] border border-border bg-panel/80 p-5 shadow-[0_24px_80px_-40px_rgba(0,0,0,0.95)]">
      <p className="text-xs uppercase tracking-[0.18em] text-muted">{eyebrow}</p>
      <div className="mt-2 flex flex-wrap items-end justify-between gap-3">
        <div>
          <h2 className="text-xl font-semibold tracking-tight text-fg">{title}</h2>
          <p className="mt-1 max-w-2xl text-sm text-muted">{description}</p>
        </div>
      </div>
      <div className="mt-4">{children}</div>
    </section>
  );
}

function StatusChip({
  label,
  value,
  on,
}: {
  label: string;
  value: string;
  on?: boolean;
}) {
  return (
    <span
      className={`rounded-full border px-3 py-1 text-xs ${
        on
          ? "border-emerald-400/30 bg-emerald-400/10 text-emerald-100"
          : "border-border bg-panel text-muted"
      }`}
    >
      <span className="text-muted">{label}</span> {value}
    </span>
  );
}

function SafetyToggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <label className="group flex cursor-pointer items-start gap-3 rounded-2xl border border-border bg-panel/50 p-4 transition hover:border-accent/40">
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        className="mt-1 h-4 w-4 rounded border-border bg-bg text-accent focus:ring-accent"
      />
      <span>
        <span className="block text-sm font-medium text-fg">{label}</span>
        <span className="mt-1 block text-sm text-muted">{hint}</span>
      </span>
    </label>
  );
}

export default function App() {
  const [tab, setTab] = useState<Tab>("chat");
  const [token, setToken] = useState(() => localStorage.getItem(TOKEN_KEY) ?? "");
  const [readOnly, setReadOnly] = useState(false);
  const [dryRun, setDryRun] = useState(false);
  const [strictMode, setStrictMode] = useState(false);
  const [safetyHydrated, setSafetyHydrated] = useState(false);
  const [publicCfg, setPublicCfg] = useState<PublicConfig | null>(null);
  const [schema, setSchema] = useState<SchemaShape | null>(null);
  const [tools, setTools] = useState<ToolDef[]>([]);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [chatLog, setChatLog] = useState<ChatEntry[]>([]);
  const [input, setInput] = useState("");
  const [wsStatus, setWsStatus] = useState<"idle" | "connecting" | "open" | "closed" | "err">(
    "idle",
  );
  const [lastError, setLastError] = useState<string | null>(null);
  const wsRef = useRef<WebSocket | null>(null);

  const saveToken = useCallback((value: string) => {
    setToken(value);
    if (value) localStorage.setItem(TOKEN_KEY, value);
    else localStorage.removeItem(TOKEN_KEY);
  }, []);

  const requestJson = useCallback(
    async <T,>(path: string): Promise<T | null> => {
      try {
        const response = await fetch(path, { headers: authHeaders(token) });
        if (!response.ok) {
          throw new Error(`${response.status} ${response.statusText}`);
        }
        return (await response.json()) as T;
      } catch (error) {
        setLastError(String(error));
        return null;
      }
    },
    [token],
  );

  const fetchCfg = useCallback(async () => {
    const data = await requestJson<PublicConfig>("/config/public");
    if (data) setPublicCfg(data);
  }, [requestJson]);

  const fetchSchema = useCallback(async () => {
    const data = await requestJson<SchemaShape>("/schema");
    if (data) setSchema(data);
  }, [requestJson]);

  const fetchTools = useCallback(async () => {
    const data = await requestJson<ToolDef[]>("/tools");
    if (Array.isArray(data)) setTools(data);
  }, [requestJson]);

  const fetchHistory = useCallback(async () => {
    const data = await requestJson<HistoryEntry[]>("/history?limit=30");
    if (Array.isArray(data)) setHistory(data);
  }, [requestJson]);

  useEffect(() => {
    void fetchCfg();
    void fetchSchema();
    void fetchTools();
    void fetchHistory();
  }, [fetchCfg, fetchHistory, fetchSchema, fetchTools]);

  useEffect(() => {
    if (publicCfg && !safetyHydrated) {
      setReadOnly(publicCfg.read_only ?? false);
      setDryRun(publicCfg.dry_run ?? false);
      setStrictMode(publicCfg.strict ?? false);
      setSafetyHydrated(true);
    }
  }, [publicCfg, safetyHydrated]);

  const wsUrl = useMemo(() => {
    const url = new URL("/chat", window.location.origin);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    if (token) url.searchParams.set("token", token);
    return url.toString();
  }, [token]);

  const handleAgentEvent = useCallback((event: AgentEvent) => {
    if (event.kind === "assistant_delta") {
      setChatLog((rows) => {
        const last = rows[rows.length - 1];
        if (last?.role === "assistant") {
          return [
            ...rows.slice(0, -1),
            {
              ...last,
              body: `${last.body}${event.text}`,
            },
          ];
        }
        return [
          ...rows,
          {
            role: "assistant",
            title: "Assistant",
            body: event.text,
          },
        ];
      });
      return;
    }

    if (event.kind === "assistant_message") {
      setChatLog((rows) => [
        ...rows,
        {
          role: "assistant",
          title: "Assistant",
          body: event.text,
        },
      ]);
      return;
    }

    if (event.kind === "tool_call") {
      setChatLog((rows) => [
        ...rows,
        {
          role: "tool",
          title: event.name,
          body: formatJson(event.arguments),
          tone: "accent",
        },
      ]);
      return;
    }

    if (event.kind === "tool_result") {
      setChatLog((rows) => [
        ...rows,
        {
          role: "tool",
          title: `${event.status.toUpperCase()}${event.duration_ms ? ` · ${event.duration_ms}ms` : ""}`,
          body: formatJson(event.result),
          tone: event.status === "ok" ? "success" : "danger",
        },
      ]);
      return;
    }

    if (event.kind === "error") {
      setLastError(event.message);
      setChatLog((rows) => [
        ...rows,
        {
          role: "error",
          title: "Runtime error",
          body: event.message,
          tone: "danger",
        },
      ]);
    }
  }, []);

  const connectWs = useCallback(() => {
    wsRef.current?.close();
    setWsStatus("connecting");
    try {
      const ws = new WebSocket(wsUrl);
      wsRef.current = ws;
      ws.onopen = () => {
        setWsStatus("open");
        setLastError(null);
      };
      ws.onclose = () => setWsStatus("closed");
      ws.onerror = () => setWsStatus("err");
      ws.onmessage = (raw) => {
        try {
          const event = JSON.parse(String(raw.data)) as AgentEvent;
          if (event.kind !== "user_prompt") {
            handleAgentEvent(event);
          }
        } catch {
          const message = String(raw.data);
          setLastError(message);
          setChatLog((rows) => [
            ...rows,
            {
              role: "error",
              title: "Unparsed event",
              body: message,
              tone: "danger",
            },
          ]);
        }
      };
    } catch (error) {
      setLastError(String(error));
      setWsStatus("err");
    }
  }, [handleAgentEvent, wsUrl]);

  useEffect(() => {
    connectWs();
    return () => wsRef.current?.close();
  }, [connectWs]);

  const sendChat = useCallback(async () => {
    const text = input.trim();
    if (!text) return;

    setLastError(null);
    setChatLog((rows) => [
      ...rows,
      {
        role: "you",
        title: "You",
        body: text,
      },
    ]);

    const payload = JSON.stringify({
      message: text,
      read_only: readOnly,
      dry_run: dryRun,
      strict: strictMode,
    });

    setInput("");

    const ws = wsRef.current;
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(payload);
      return;
    }

    try {
      const response = await fetch("/run", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          ...authHeaders(token),
        },
        body: payload,
      });
      const body = (await response.json()) as {
        result?: unknown;
        events?: unknown[];
        error?: string;
      };

      if (!response.ok || body.error) {
        const message = body.error ?? `HTTP ${response.status}`;
        setLastError(message);
        setChatLog((rows) => [
          ...rows,
          {
            role: "error",
            title: "Request failed",
            body: message,
            tone: "danger",
          },
        ]);
        return;
      }

      if (Array.isArray(body.events)) {
        for (const raw of body.events) {
          const event = raw as AgentEvent;
          if (event.kind !== "user_prompt") {
            handleAgentEvent(event);
          }
        }
      }

      if (body.result !== undefined && !matchesAssistantResult(body.result, body.events)) {
        setChatLog((rows) => [
          ...rows,
          {
            role: "assistant",
            title: "Assistant",
            body: formatJson(body.result),
          },
        ]);
      }

      void fetchHistory();
    } catch (error) {
      const message = String(error);
      setLastError(message);
      setChatLog((rows) => [
        ...rows,
        {
          role: "error",
          title: "Network failure",
          body: message,
          tone: "danger",
        },
      ]);
    }
  }, [dryRun, fetchHistory, handleAgentEvent, input, readOnly, strictMode, token]);

  const actions = useMemo(
    () =>
      (schema?.resources ?? []).flatMap((resource) =>
        resource.actions.map((action) => ({
          ...action,
          resourceName: resource.name,
        })),
      ),
    [schema],
  );

  const summary = useMemo(() => {
    const resources = schema?.resources.length ?? 0;
    const actionCount = actions.length;
    const writes = actions.filter((action) => action.safety !== "read_only").length;
    const destructive = actions.filter((action) => action.safety === "destructive").length;
    return { resources, actionCount, writes, destructive };
  }, [actions, schema]);

  const suggestions = useMemo(() => promptSuggestions(schema), [schema]);

  return (
    <div className="min-h-screen bg-[radial-gradient(circle_at_top_left,_rgba(124,196,255,0.12),_transparent_30%),radial-gradient(circle_at_top_right,_rgba(168,255,154,0.08),_transparent_28%),linear-gradient(180deg,_rgba(11,13,16,0.98),_rgba(9,11,14,1))]">
      <div className="mx-auto flex min-h-screen w-full max-w-[1500px] flex-col px-4 py-5 sm:px-6 lg:px-8">
        <header className="rounded-[28px] border border-border bg-panel/80 px-5 py-5 shadow-[0_24px_80px_-40px_rgba(0,0,0,0.95)]">
          <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
            <div className="space-y-3">
              <div className="flex flex-wrap items-center gap-3">
                <div className="flex h-11 w-11 items-center justify-center rounded-2xl border border-accent/30 bg-accent/10 text-lg font-semibold text-accent">
                  &gt;_
                </div>
                <div>
                  <p className="text-xs uppercase tracking-[0.22em] text-muted">
                    Operator Console
                  </p>
                  <h1 className="text-3xl font-semibold tracking-tight text-fg">appctl</h1>
                </div>
              </div>
              <p className="max-w-3xl text-sm text-muted sm:text-base">
                Drive the synced app with natural language, keep safety modes explicit, and inspect
                every available action before the model touches live data.
              </p>
              <div className="flex flex-wrap gap-2">
                <StatusChip label="WS" value={wsStatus} on={wsStatus === "open"} />
                <StatusChip
                  label="Provider"
                  value={publicCfg?.default_provider ?? "not configured"}
                  on={Boolean(publicCfg?.default_provider)}
                />
                <StatusChip
                  label="Source"
                  value={sourceLabel(publicCfg?.sync_source ?? schema?.source)}
                  on={Boolean(publicCfg?.sync_source ?? schema?.source)}
                />
                <StatusChip
                  label="Writes"
                  value={publicCfg?.confirm_default ? "auto-confirm on" : "review defaults"}
                  on={Boolean(publicCfg?.confirm_default)}
                />
              </div>
            </div>
            <nav className="flex flex-wrap gap-2">
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
          </div>
        </header>

        {lastError && (
          <div className="mt-4 rounded-2xl border border-rose-400/30 bg-rose-400/10 px-4 py-3 text-sm text-rose-100">
            <span className="font-medium">Attention:</span> {lastError}
          </div>
        )}

        <main className="mt-4 flex flex-1 flex-col gap-4">
          <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
            <StatCard
              label="Resources"
              value={String(summary.resources)}
              hint="Distinct entities in the synced contract."
            />
            <StatCard
              label="Actions"
              value={String(summary.actionCount)}
              hint="Callable operations available to the agent."
            />
            <StatCard
              label="Write Paths"
              value={String(summary.writes)}
              hint="Mutating actions that can change real data."
            />
            <StatCard
              label="History"
              value={String(history.length)}
              hint="Most recent actions loaded from the audit log."
            />
          </div>

          {tab === "chat" && (
            <div className="grid flex-1 gap-4 xl:grid-cols-[minmax(0,1.65fr)_360px]">
              <SectionShell
                eyebrow="Conversation"
                title="Operate the synced app"
                description="Use live chat when the socket is open. If the stream drops, the console falls back to HTTP requests so you can keep working."
              >
                <div className="flex flex-wrap gap-3">
                  <SafetyToggle
                    label="Read-only"
                    hint="Blocks any write or delete path. Useful for discovery and audits."
                    checked={readOnly}
                    onChange={setReadOnly}
                  />
                  <SafetyToggle
                    label="Dry-run"
                    hint="Shows the intended action without executing it against the target system."
                    checked={dryRun}
                    onChange={setDryRun}
                  />
                  <SafetyToggle
                    label="Strict"
                    hint="Blocks inferred HTTP tools until doctor has verified them."
                    checked={strictMode}
                    onChange={setStrictMode}
                  />
                </div>

                <div className="mt-4 overflow-hidden rounded-[24px] border border-border bg-code shadow-[inset_0_1px_0_rgba(255,255,255,0.03)]">
                  <div className="flex items-center justify-between border-b border-border px-4 py-3">
                    <div>
                      <p className="text-sm font-medium text-fg">Session stream</p>
                      <p className="text-xs text-muted">
                        {wsStatus === "open"
                          ? "Live events are streaming over WebSocket."
                          : "Socket offline; prompts will use POST /run until the stream reconnects."}
                      </p>
                    </div>
                    <button
                      type="button"
                      className="rounded-full border border-border bg-panel px-3 py-1.5 text-sm text-fg transition hover:border-accent/40 hover:text-accent"
                      onClick={connectWs}
                    >
                      Reconnect
                    </button>
                  </div>

                  <div className="max-h-[520px] min-h-[420px] overflow-y-auto p-4">
                    {chatLog.length === 0 ? (
                      <div className="grid h-full place-items-center rounded-[20px] border border-dashed border-border bg-panel/40 px-6 py-10 text-center">
                        <div className="max-w-xl">
                          <p className="text-xs uppercase tracking-[0.2em] text-muted">
                            First run
                          </p>
                          <h3 className="mt-2 text-2xl font-semibold tracking-tight text-fg">
                            Start with a safe prompt.
                          </h3>
                          <p className="mt-3 text-sm leading-7 text-muted">
                            Ask for a summary first, then move into writes once the contract looks
                            right. The chat log will show tool calls and results inline.
                          </p>
                          <div className="mt-6 flex flex-wrap justify-center gap-2">
                            {suggestions.map((prompt) => (
                              <button
                                key={prompt}
                                type="button"
                                className="rounded-full border border-border bg-panel px-3 py-2 text-sm text-fg transition hover:border-accent/40 hover:text-accent"
                                onClick={() => setInput(prompt)}
                              >
                                {prompt}
                              </button>
                            ))}
                          </div>
                        </div>
                      </div>
                    ) : (
                      <div className="space-y-3">
                        {chatLog.map((entry, index) => (
                          <article
                            key={`${entry.title}-${index}`}
                            className={`rounded-[20px] border px-4 py-3 ${
                              entry.role === "you"
                                ? "border-accent/30 bg-accent/10"
                                : entry.role === "error"
                                  ? "border-rose-400/30 bg-rose-400/10"
                                  : entry.role === "tool"
                                    ? entry.tone === "success"
                                      ? "border-emerald-400/25 bg-emerald-400/8"
                                      : entry.tone === "danger"
                                        ? "border-rose-400/25 bg-rose-400/8"
                                        : "border-sky-400/25 bg-sky-400/8"
                                    : "border-border bg-panel/60"
                            }`}
                          >
                            <div className="flex items-center justify-between gap-3">
                              <p className="text-sm font-medium text-fg">{entry.title}</p>
                              <span className="text-xs uppercase tracking-[0.18em] text-muted">
                                {entry.role}
                              </span>
                            </div>
                            <pre className="mt-3 overflow-x-auto whitespace-pre-wrap break-words font-mono text-sm leading-6 text-fg">
                              {entry.body}
                            </pre>
                          </article>
                        ))}
                      </div>
                    )}
                  </div>

                  <div className="border-t border-border bg-panel/70 p-3">
                    <div className="flex flex-col gap-3 md:flex-row">
                      <textarea
                        className="min-h-[92px] flex-1 rounded-[20px] border border-border bg-bg px-4 py-3 text-sm text-fg outline-none transition placeholder:text-muted focus:border-accent/60"
                        placeholder="Ask in plain English. Enter sends, Shift+Enter adds a new line."
                        value={input}
                        onChange={(event) => setInput(event.target.value)}
                        onKeyDown={(event) => {
                          if (event.key === "Enter" && !event.shiftKey) {
                            event.preventDefault();
                            void sendChat();
                          }
                        }}
                      />
                      <div className="flex flex-col gap-2 md:w-44">
                        <button
                          type="button"
                          className="rounded-[18px] bg-accent px-4 py-3 text-sm font-semibold text-bg transition hover:bg-[#97d4ff]"
                          onClick={() => void sendChat()}
                        >
                          Send prompt
                        </button>
                        <button
                          type="button"
                          className="rounded-[18px] border border-border bg-panel px-4 py-3 text-sm text-fg transition hover:border-accent/40 hover:text-accent"
                          onClick={() => setInput("Summarize this app and tell me the safest next prompt.")}
                        >
                          Insert starter
                        </button>
                      </div>
                    </div>
                  </div>
                </div>
              </SectionShell>

              <div className="space-y-4">
                <SectionShell
                  eyebrow="Runtime"
                  title="Current target"
                  description="Everything here comes from the synced schema and the live daemon config."
                >
                  <dl className="space-y-3 text-sm">
                    <div className="rounded-2xl border border-border bg-panel/50 p-4">
                      <dt className="text-xs uppercase tracking-[0.18em] text-muted">Base URL</dt>
                      <dd className="mt-2 break-all font-mono text-fg">
                        {publicCfg?.base_url ?? schema?.base_url ?? "Not configured"}
                      </dd>
                    </div>
                    <div className="rounded-2xl border border-border bg-panel/50 p-4">
                      <dt className="text-xs uppercase tracking-[0.18em] text-muted">Auth strategy</dt>
                      <dd className="mt-2 font-mono text-fg">{schema?.auth?.kind ?? "unknown"}</dd>
                    </div>
                    <div className="rounded-2xl border border-border bg-panel/50 p-4">
                      <dt className="text-xs uppercase tracking-[0.18em] text-muted">
                        Server defaults
                      </dt>
                      <dd className="mt-2 text-muted">
                        Read-only {publicCfg?.read_only ? "on" : "off"} · Dry-run{" "}
                        {publicCfg?.dry_run ? "on" : "off"} · Strict{" "}
                        {publicCfg?.strict ? "on" : "off"}
                      </dd>
                    </div>
                  </dl>
                </SectionShell>

                <SectionShell
                  eyebrow="Prompts"
                  title="Safe starters"
                  description="Load one into the composer, then adjust the safety toggles before sending."
                >
                  <div className="space-y-2">
                    {suggestions.map((prompt) => (
                      <button
                        key={prompt}
                        type="button"
                        className="w-full rounded-2xl border border-border bg-panel/50 px-4 py-3 text-left text-sm text-fg transition hover:border-accent/40 hover:text-accent"
                        onClick={() => setInput(prompt)}
                      >
                        {prompt}
                      </button>
                    ))}
                  </div>
                </SectionShell>
              </div>
            </div>
          )}

          {tab === "tools" && (
            <SectionShell
              eyebrow="Tools"
              title="Agent-callable actions"
              description="Review the exact actions the model can call, including transport, safety level, and required parameters."
            >
              <div className="grid gap-4 xl:grid-cols-2">
                {actions.length === 0 ? (
                  <div className="rounded-[24px] border border-dashed border-border bg-panel/40 px-6 py-10 text-sm text-muted">
                    No schema loaded yet. Run <code className="text-fg">appctl sync ...</code> and
                    refresh this page.
                  </div>
                ) : (
                  actions.map((action) => {
                    const tool = tools.find((item) => item.name === action.name);
                    const required =
                      tool?.input_schema?.required ??
                      action.parameters.filter((field) => field.required).map((field) => field.name);
                    return (
                      <article
                        key={action.name}
                        className="rounded-[24px] border border-border bg-code/80 p-5"
                      >
                        <div className="flex flex-wrap items-start justify-between gap-3">
                          <div>
                            <p className="text-xs uppercase tracking-[0.18em] text-muted">
                              {action.resourceName}
                            </p>
                            <h3 className="mt-1 text-lg font-semibold tracking-tight text-fg">
                              {action.name}
                            </h3>
                            <p className="mt-2 text-sm text-muted">
                              {action.description ?? tool?.description ?? "No description."}
                            </p>
                          </div>
                          <div className="flex flex-wrap gap-2">
                            <span
                              className={`rounded-full border px-3 py-1 text-xs ${toneForSafety(action.safety)}`}
                            >
                              {action.safety}
                            </span>
                            <span
                              className={`rounded-full border px-3 py-1 text-xs ${toneForProvenance(
                                action.provenance,
                              )}`}
                            >
                              {action.provenance ?? "inferred"}
                            </span>
                          </div>
                        </div>

                        <div className="mt-4 rounded-2xl border border-border bg-panel/50 p-4">
                          <p className="text-xs uppercase tracking-[0.18em] text-muted">
                            Transport
                          </p>
                          <p className="mt-2 font-mono text-sm text-fg">
                            {transportLabel(action.transport)}
                          </p>
                        </div>

                        <div className="mt-4 grid gap-3 md:grid-cols-2">
                          <div className="rounded-2xl border border-border bg-panel/50 p-4">
                            <p className="text-xs uppercase tracking-[0.18em] text-muted">
                              Required params
                            </p>
                            <p className="mt-2 text-sm text-fg">
                              {required.length > 0 ? required.join(", ") : "None"}
                            </p>
                          </div>
                          <div className="rounded-2xl border border-border bg-panel/50 p-4">
                            <p className="text-xs uppercase tracking-[0.18em] text-muted">
                              Parameter count
                            </p>
                            <p className="mt-2 text-sm text-fg">{action.parameters.length}</p>
                          </div>
                        </div>
                      </article>
                    );
                  })
                )}
              </div>
            </SectionShell>
          )}

          {tab === "history" && (
            <SectionShell
              eyebrow="Audit log"
              title="Recent actions"
              description="This is the same local history that powers inspection and undo. Use it to spot risky writes, bad defaults, or drift."
            >
              <div className="space-y-3">
                {history.length === 0 ? (
                  <div className="rounded-[24px] border border-dashed border-border bg-panel/40 px-6 py-10 text-sm text-muted">
                    No history entries yet. Once the agent executes tools, they will appear here.
                  </div>
                ) : (
                  history.map((entry) => (
                    <article
                      key={entry.id}
                      className="rounded-[24px] border border-border bg-code/80 p-5"
                    >
                      <div className="flex flex-wrap items-start justify-between gap-3">
                        <div>
                          <p className="text-xs uppercase tracking-[0.18em] text-muted">
                            #{entry.id} · {formatTs(entry.ts)}
                          </p>
                          <h3 className="mt-1 text-lg font-semibold tracking-tight text-fg">
                            {entry.tool}
                          </h3>
                        </div>
                        <div className="flex flex-wrap gap-2">
                          <span
                            className={`rounded-full border px-3 py-1 text-xs ${
                              entry.status === "ok"
                                ? "border-emerald-400/30 bg-emerald-400/10 text-emerald-100"
                                : "border-rose-400/30 bg-rose-400/10 text-rose-100"
                            }`}
                          >
                            {entry.status}
                          </span>
                          {entry.undone && (
                            <span className="rounded-full border border-amber-400/30 bg-amber-400/10 px-3 py-1 text-xs text-amber-100">
                              undone
                            </span>
                          )}
                        </div>
                      </div>

                      <div className="mt-4 grid gap-3 lg:grid-cols-3">
                        <div className="rounded-2xl border border-border bg-panel/50 p-4">
                          <p className="text-xs uppercase tracking-[0.18em] text-muted">
                            Session
                          </p>
                          <p className="mt-2 break-all font-mono text-xs text-fg">
                            {entry.session_id}
                          </p>
                        </div>
                        <div className="rounded-2xl border border-border bg-panel/50 p-4 lg:col-span-2">
                          <p className="text-xs uppercase tracking-[0.18em] text-muted">
                            Arguments
                          </p>
                          <p className="mt-2 font-mono text-xs text-fg">
                            {previewJson(entry.arguments_json)}
                          </p>
                        </div>
                        <div className="rounded-2xl border border-border bg-panel/50 p-4 lg:col-span-3">
                          <p className="text-xs uppercase tracking-[0.18em] text-muted">
                            Response preview
                          </p>
                          <p className="mt-2 font-mono text-xs text-fg">
                            {previewJson(entry.response_json)}
                          </p>
                        </div>
                      </div>
                    </article>
                  ))
                )}
              </div>
            </SectionShell>
          )}

          {tab === "settings" && (
            <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_420px]">
              <SectionShell
                eyebrow="Connection"
                title="Daemon access"
                description="Use a bearer token if you started appctl serve with --token. Refresh pulls config, tools, schema, and history again."
              >
                <div className="space-y-4">
                  <label className="block">
                    <span className="text-sm text-muted">Token</span>
                    <input
                      type="password"
                      className="mt-2 w-full rounded-[18px] border border-border bg-bg px-4 py-3 text-fg outline-none transition focus:border-accent/60"
                      value={token}
                      onChange={(event) => saveToken(event.target.value)}
                      placeholder="optional"
                    />
                  </label>
                  <div className="flex flex-wrap gap-3">
                    <button
                      type="button"
                      className="rounded-[18px] bg-accent px-4 py-3 text-sm font-semibold text-bg transition hover:bg-[#97d4ff]"
                      onClick={() => {
                        void fetchCfg();
                        void fetchSchema();
                        void fetchTools();
                        void fetchHistory();
                        connectWs();
                      }}
                    >
                      Refresh runtime
                    </button>
                    <button
                      type="button"
                      className="rounded-[18px] border border-border bg-panel px-4 py-3 text-sm text-fg transition hover:border-accent/40 hover:text-accent"
                      onClick={() => saveToken("")}
                    >
                      Clear token
                    </button>
                  </div>
                  <div className="rounded-[24px] border border-border bg-code p-4">
                    <p className="text-xs uppercase tracking-[0.18em] text-muted">Runtime JSON</p>
                    <pre className="mt-3 overflow-x-auto whitespace-pre-wrap break-words font-mono text-xs text-fg">
                      {formatJson(publicCfg ?? {})}
                    </pre>
                  </div>
                </div>
              </SectionShell>

              <SectionShell
                eyebrow="Project"
                title="What this daemon knows"
                description="Helpful when you’re debugging sync output or handing the project off to someone else."
              >
                <div className="space-y-3 text-sm text-muted">
                  <div className="rounded-2xl border border-border bg-panel/50 p-4">
                    <p className="text-xs uppercase tracking-[0.18em] text-muted">Schema source</p>
                    <p className="mt-2 text-fg">{sourceLabel(schema?.source ?? publicCfg?.sync_source)}</p>
                  </div>
                  <div className="rounded-2xl border border-border bg-panel/50 p-4">
                    <p className="text-xs uppercase tracking-[0.18em] text-muted">Target URL</p>
                    <p className="mt-2 break-all font-mono text-xs text-fg">
                      {schema?.base_url ?? publicCfg?.base_url ?? "Not set"}
                    </p>
                  </div>
                  <div className="rounded-2xl border border-border bg-panel/50 p-4">
                    <p className="text-xs uppercase tracking-[0.18em] text-muted">Local files</p>
                    <p className="mt-2">
                      <code className="text-fg">.appctl/config.toml</code>,{" "}
                      <code className="text-fg">.appctl/schema.json</code>,{" "}
                      <code className="text-fg">.appctl/tools.json</code>, and{" "}
                      <code className="text-fg">.appctl/history.db</code>
                    </p>
                  </div>
                </div>
              </SectionShell>
            </div>
          )}
        </main>
      </div>
    </div>
  );
}
