import {
  Fragment,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

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
  | {
      kind: "session_state";
      session_id: string;
      transcript_len: number;
      resumed?: boolean;
    }
  | { kind: "context_notice"; message: string }
  | { kind: "done" };

type ChatEntry =
  | { kind: "user"; id: string; body: string }
  | {
      kind: "assistant";
      id: string;
      body: string;
      streaming?: boolean;
    }
  | {
      kind: "tool_call";
      id: string;
      name: string;
      args: unknown;
      resultId?: string;
      status?: "pending" | "ok" | "error";
      duration_ms?: number;
      result?: unknown;
    }
  | { kind: "error"; id: string; body: string }
  | { kind: "notice"; id: string; body: string };

type PublicConfig = {
  /** Global registry (or folder) name from `~/.appctl/apps.toml`. */
  app_name?: string;
  /** Resolved label for UIs: `config.toml` `display_name` when set, else `app_name`. */
  banner_label?: string;
  display_name?: string | null;
  default_provider?: string;
  active_provider?: string;
  provider_statuses?: ProviderRuntimeStatus[];
  sync_source?: string;
  base_url?: string | null;
  read_only?: boolean;
  dry_run?: boolean;
  strict?: boolean;
  confirm_default?: boolean;
  description?: string | null;
};

type ProviderRuntimeStatus = {
  name: string;
  kind: string;
  base_url: string;
  model: string;
  verified: boolean;
  auth_status: {
    kind: "none" | "api_key" | "oauth2" | "google_adc";
    origin: "explicit" | "cloud" | "legacy_api_key_ref";
    configured: boolean;
    secret_ref?: string | null;
    profile?: string | null;
    expires_at?: number | null;
    scopes?: string[];
    project_id?: string | null;
    recovery_hint?: string | null;
  };
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
  | {
      kind: "no_sql";
      collection: string;
      operation: string;
      database_kind: string;
      primary_key?: string | null;
      secondary_key?: string | null;
    }
  | { kind: "mcp"; server_url: string };

type HistoryEntry = {
  id: number;
  ts: string;
  session_id: string;
  session_name?: string | null;
  tool: string;
  arguments_json: unknown;
  request_snapshot_json?: unknown;
  response_json?: unknown;
  status: string;
  undone: boolean;
};

/* ---------- helpers ---------- */

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

function toolResultSummary(value: unknown): string | null {
  if (!value || typeof value !== "object") return null;
  const summary = (value as { summary?: unknown }).summary;
  return typeof summary === "string" && summary.trim() ? summary : null;
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
  if (!source) return "not synced";
  return source.replace(/_/g, " ");
}

function authKindLabel(kind?: ProviderRuntimeStatus["auth_status"]["kind"]): string {
  switch (kind) {
    case "api_key":
      return "API key";
    case "oauth2":
      return "OAuth2";
    case "google_adc":
      return "Google ADC";
    case "none":
    default:
      return "None";
  }
}

function formatExpiry(expiresAt?: number | null): string {
  if (!expiresAt) return "no expiry reported";
  const date = new Date(expiresAt * 1000);
  if (Number.isNaN(date.getTime())) return "no expiry reported";
  return date.toLocaleString();
}

function transportLabel(transport: Transport): string {
  switch (transport.kind) {
    case "http":
      return `${transport.method} ${transport.path}`;
    case "form":
      return `${transport.method} ${transport.action}`;
    case "sql":
      return `${databaseKindLabel(transport.database_kind)} ${sqlOperationLabel(transport.operation)} ${transport.table}`;
    case "no_sql":
      return `${databaseKindLabel(transport.database_kind)} ${noSqlOperationLabel(transport.operation)} ${transport.collection}`;
    case "mcp":
      return `MCP ${transport.server_url}`;
    default:
      return "Unknown transport";
  }
}

function databaseKindLabel(value: string): string {
  switch (value) {
    case "postgres":
      return "Postgres";
    case "mysql":
      return "MySQL";
    case "sqlite":
      return "SQLite";
    case "mongodb":
      return "MongoDB";
    case "redis":
      return "Redis";
    case "firestore":
      return "Firestore";
    case "dynamodb":
      return "DynamoDB";
    default:
      return value.replace(/_/g, " ");
  }
}

function sqlOperationLabel(value: string): string {
  switch (value) {
    case "select":
      return "list";
    case "get_by_pk":
      return "get";
    case "insert":
      return "create";
    case "update_by_pk":
      return "update";
    case "delete_by_pk":
      return "delete";
    default:
      return value.replace(/_/g, " ");
  }
}

function noSqlOperationLabel(value: string): string {
  switch (value) {
    case "get_by_pk":
      return "get";
    case "update_by_pk":
      return "update";
    case "delete_by_pk":
      return "delete";
    default:
      return value.replace(/_/g, " ");
  }
}

function safetyTone(safety: Action["safety"]): {
  label: string;
  cls: string;
} {
  switch (safety) {
    case "read_only":
      return {
        label: "read-only",
        cls: "border-emerald-400/30 bg-emerald-400/10 text-emerald-200",
      };
    case "mutating":
      return {
        label: "mutating",
        cls: "border-sky-400/30 bg-sky-400/10 text-sky-100",
      };
    case "destructive":
      return {
        label: "destructive",
        cls: "border-rose-400/30 bg-rose-400/10 text-rose-100",
      };
    default:
      return { label: String(safety), cls: "border-border bg-panel text-fg" };
  }
}

function provenanceTone(provenance?: Action["provenance"]): {
  label: string;
  cls: string;
} {
  switch (provenance) {
    case "verified":
      return {
        label: "verified",
        cls: "border-emerald-400/30 bg-emerald-400/10 text-emerald-200",
      };
    case "declared":
      return {
        label: "declared",
        cls: "border-sky-400/30 bg-sky-400/10 text-sky-100",
      };
    default:
      return {
        label: "inferred",
        cls: "border-amber-400/30 bg-amber-400/10 text-amber-100",
      };
  }
}

function promptSuggestions(schema: SchemaShape | null, appName?: string): string[] {
  const resources = schema?.resources?.slice(0, 3).map((r) => r.name) || [];
  const app = appName ?? "this app";

  if (resources.length === 0) {
    return [
      `What tools exist for ${app}? Which are read-only?`,
      "Which tools write or delete data?",
      "What should I run with strict or read-only mode on?",
    ];
  }

  const first = resources[0];
  const second = resources.length > 1 ? resources[1] : first;

  return [
    `List ${first} entries (main fields only).`,
    `Create one ${second} and show the tool call you used.`,
    `Which ${app} tools are mutating?`,
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

const randId = () => Math.random().toString(36).slice(2, 10);

/* ---------- small UI atoms ---------- */

function Pill({
  children,
  tone = "muted",
  className = "",
}: {
  children: ReactNode;
  tone?: "muted" | "accent" | "success" | "warn" | "danger";
  className?: string;
}) {
  const map: Record<string, string> = {
    muted: "border-border bg-surface text-muted",
    accent: "border-border-strong bg-panel text-fg",
    success: "border-emerald-500/20 bg-emerald-500/10 text-emerald-400",
    warn: "border-amber-500/20 bg-amber-500/10 text-amber-400",
    danger: "border-rose-500/20 bg-rose-500/10 text-rose-400",
  };
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-md border px-2 py-0.5 text-[11px] font-medium ${map[tone]} ${className}`}
    >
      {children}
    </span>
  );
}

function KV({ k, v, mono = false }: { k: string; v: ReactNode; mono?: boolean }) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-2">
        {k}
      </span>
      <span className={`text-[13px] ${mono ? "font-mono text-fg" : "text-fg-dim"}`}>{v}</span>
    </div>
  );
}

function IconChat() {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="M21 11.5a8.4 8.4 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.4 8.4 0 0 1-3.8-.9L3 21l1.9-5.7a8.4 8.4 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6A8.4 8.4 0 0 1 12.5 3h.5a8.5 8.5 0 0 1 8 8v.5Z" />
    </svg>
  );
}
function IconTools() {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="M14.7 6.3a4 4 0 0 0 5 5L17 14l-7 7-3-3 7-7 2.7-2.7Z" />
      <path d="M7 14l-4 4" />
      <path d="M17 3l4 4" />
    </svg>
  );
}
function IconHistory() {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="M3 12a9 9 0 1 0 3-6.7" />
      <path d="M3 3v6h6" />
      <path d="M12 7v5l3 2" />
    </svg>
  );
}
function IconSettings() {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1.08-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9c.2.47.7.8 1.24.85L21 10a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1Z" />
    </svg>
  );
}

/* ---------- chat cards ---------- */

function Markdown({ source }: { source: string }) {
  return (
    <div className="markdown text-[14px] leading-relaxed text-fg">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          a: ({ href, children }) => (
            <a
              href={href}
              target="_blank"
              rel="noreferrer"
              className="text-accent underline decoration-dotted underline-offset-2 hover:decoration-solid"
            >
              {children}
            </a>
          ),
          code: ({ inline, className, children, ...rest }: {
            inline?: boolean;
            className?: string;
            children?: ReactNode;
          }) => {
            if (inline) {
              return (
                <code
                  className="rounded bg-elev/80 px-1.5 py-0.5 font-mono text-[12.5px] text-[#c7e3ff]"
                  {...rest}
                >
                  {children}
                </code>
              );
            }
            return (
              <code className={`${className ?? ""} font-mono text-[12.5px]`} {...rest}>
                {children}
              </code>
            );
          },
          pre: ({ children }) => (
            <pre className="my-3 overflow-x-auto rounded-xl border border-border bg-elev/60 p-3 font-mono text-[12.5px] leading-relaxed text-fg-dim">
              {children}
            </pre>
          ),
          table: ({ children }) => (
            <div className="my-3 overflow-x-auto rounded-xl border border-border">
              <table className="w-full border-collapse text-[13px]">{children}</table>
            </div>
          ),
          thead: ({ children }) => (
            <thead className="bg-elev/50 text-left text-[12px] uppercase tracking-[0.08em] text-muted">
              {children}
            </thead>
          ),
          th: ({ children }) => (
            <th className="border-b border-border px-3 py-2 font-semibold">{children}</th>
          ),
          td: ({ children }) => (
            <td className="border-b border-border/60 px-3 py-2 align-top text-fg-dim">
              {children}
            </td>
          ),
          tr: ({ children }) => <tr className="even:bg-white/[0.015]">{children}</tr>,
          ul: ({ children }) => (
            <ul className="my-2 list-disc space-y-1 pl-5 marker:text-muted-2">{children}</ul>
          ),
          ol: ({ children }) => (
            <ol className="my-2 list-decimal space-y-1 pl-5 marker:text-muted-2">{children}</ol>
          ),
          li: ({ children }) => <li className="text-fg-dim">{children}</li>,
          h1: ({ children }) => (
            <h1 className="mt-3 mb-1 text-[17px] font-semibold text-fg">{children}</h1>
          ),
          h2: ({ children }) => (
            <h2 className="mt-3 mb-1 text-[15px] font-semibold text-fg">{children}</h2>
          ),
          h3: ({ children }) => (
            <h3 className="mt-3 mb-1 text-[14px] font-semibold text-fg">{children}</h3>
          ),
          p: ({ children }) => <p className="my-1.5 text-fg">{children}</p>,
          strong: ({ children }) => <strong className="font-semibold text-fg">{children}</strong>,
          em: ({ children }) => <em className="text-fg">{children}</em>,
          blockquote: ({ children }) => (
            <blockquote className="my-2 border-l-2 border-accent/40 pl-3 text-muted">
              {children}
            </blockquote>
          ),
          hr: () => <hr className="my-3 border-border" />,
        }}
      >
        {source}
      </ReactMarkdown>
    </div>
  );
}

function UserMessage({ body }: { body: string }) {
  return (
    <div className="flex justify-end mb-2">
      <div className="max-w-[82%] rounded-lg bg-panel px-4 py-2.5 msg-user">
        <div className="whitespace-pre-wrap break-words text-[14px] leading-relaxed text-fg">
          {body}
        </div>
      </div>
    </div>
  );
}

function AssistantMessage({ body, streaming }: { body: string; streaming?: boolean }) {
  const trimmed = body ?? "";
  return (
    <div className="flex items-start gap-4 mb-4">
      <div className="mt-1 flex h-6 w-6 flex-none items-center justify-center rounded bg-fg text-[10px] font-bold text-bg">
        ap
      </div>
      <div className="min-w-0 flex-1 pt-0.5 msg-assistant">
        {trimmed ? (
          <Markdown source={trimmed} />
        ) : streaming ? (
          <div className="streaming-dot text-[14px] text-fg"> </div>
        ) : null}
      </div>
    </div>
  );
}

function ToolCard({
  name,
  args,
  status,
  duration_ms,
  result,
}: {
  name: string;
  args: unknown;
  status?: "pending" | "ok" | "error";
  duration_ms?: number;
  result?: unknown;
}) {
  const [open, setOpen] = useState(false);
  const summary = toolResultSummary(result);

  const statusIcon =
    status === "ok" ? (
      <span className="text-emerald-400">✓</span>
    ) : status === "error" ? (
      <span className="text-rose-400">✗</span>
    ) : (
      <span className="animate-pulse text-muted">...</span>
    );

  return (
    <div className="mb-4 ml-10 flex items-start gap-3">
      <div className="min-w-0 flex-1 rounded-md border border-border bg-surface">
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          className="flex w-full items-center gap-3 px-3 py-2 text-left hover:bg-panel transition"
        >
          <span className="font-mono text-[12px] text-fg-dim">{name}</span>
          {summary && (
            <span className="min-w-0 truncate text-[11px] text-muted">{summary}</span>
          )}
          <span className="ml-auto flex items-center gap-2 text-[11px]">
            {typeof duration_ms === "number" && (
              <span className="text-muted">{duration_ms}ms</span>
            )}
            {statusIcon}
          </span>
        </button>
        {open && (
          <div className="border-t border-border p-3 font-mono text-[11px] text-muted overflow-x-auto">
            <div className="mb-2 text-fg-dim">Arguments:</div>
            <pre>{formatJson(args)}</pre>
            {result !== undefined && (
              <>
                <div className="mt-3 mb-2 text-fg-dim">Result:</div>
                <pre>{formatJson(result)}</pre>
              </>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function ErrorMessage({ body }: { body: string }) {
  return (
    <div className="flex items-start gap-3">
      <div className="mt-0.5 flex h-8 w-8 flex-none items-center justify-center rounded-xl border border-rose-400/30 bg-rose-400/10 text-rose-200">
        !
      </div>
      <div className="min-w-0 flex-1 rounded-2xl border px-4 py-3 msg-error">
        <div className="mb-1.5 flex items-center gap-2 text-[11px] uppercase tracking-[0.14em] text-rose-200">
          chat request failed
        </div>
        <div className="whitespace-pre-wrap break-words font-mono text-[13px] leading-relaxed text-rose-50">
          {body}
        </div>
      </div>
    </div>
  );
}

function NoticeMessage({ body }: { body: string }) {
  return (
    <div className="ml-10 rounded-md border border-border bg-panel px-3 py-2 text-[12px] text-muted">
      {body}
    </div>
  );
}

/* ---------- nav rail & chrome ---------- */

function RailButton({
  active,
  onClick,
  children,
  label,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
  label: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={label}
      aria-label={label}
      className={`rail-btn ${active ? "active" : ""}`}
    >
      {children}
    </button>
  );
}

function Toggle({
  label,
  hint,
  checked,
  disabled = false,
  onChange,
}: {
  label: string;
  hint: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      aria-pressed={checked}
      onClick={() => onChange(!checked)}
      title={hint}
      className={`group flex items-center gap-2 rounded-md border px-2 py-1 text-[12px] transition ${
        checked
          ? "border-border-strong bg-panel text-fg"
          : "border-border bg-surface text-muted hover:border-border-strong hover:text-fg-dim"
      } ${disabled ? "cursor-not-allowed opacity-70" : ""}`}
    >
      <span
        className={`relative inline-flex h-3.5 w-6 items-center rounded-full transition ${
          checked ? "bg-fg" : "bg-border-strong"
        }`}
      >
        <span
          className={`inline-block h-2.5 w-2.5 transform rounded-full bg-bg shadow transition ${
            checked ? "translate-x-3" : "translate-x-0.5"
          }`}
        />
      </span>
      <span className="font-medium">{label}</span>
    </button>
  );
}

/* ---------- main component ---------- */

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
  const [sending, setSending] = useState(false);
  const [wsStatus, setWsStatus] = useState<"idle" | "connecting" | "open" | "closed" | "err">(
    "idle",
  );
  const [lastError, setLastError] = useState<string | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  /** Server-issued id shared by WebSocket and HTTP fallback so chat context stays real. */
  const runSessionIdRef = useRef<string | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);

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
        if (last?.kind === "assistant" && last.streaming) {
          return [
            ...rows.slice(0, -1),
            { ...last, body: `${last.body}${event.text}` },
          ];
        }
        return [
          ...rows,
          { kind: "assistant", id: randId(), body: event.text, streaming: true },
        ];
      });
      return;
    }

    if (event.kind === "assistant_message") {
      setChatLog((rows) => {
        const last = rows[rows.length - 1];
        if (last?.kind === "assistant" && last.streaming) {
          return [
            ...rows.slice(0, -1),
            { ...last, body: event.text, streaming: false },
          ];
        }
        return [
          ...rows,
          { kind: "assistant", id: randId(), body: event.text, streaming: false },
        ];
      });
      return;
    }

    if (event.kind === "tool_call") {
      setChatLog((rows) => [
        ...rows,
        {
          kind: "tool_call",
          id: event.id,
          name: event.name,
          args: event.arguments,
          status: "pending",
        },
      ]);
      return;
    }

    if (event.kind === "tool_result") {
      setChatLog((rows) =>
        rows.map((row) =>
          row.kind === "tool_call" && row.id === event.id
            ? {
                ...row,
                status: event.status,
                duration_ms: event.duration_ms,
                result: event.result,
              }
            : row,
        ),
      );
      return;
    }

    if (event.kind === "done") {
      setChatLog((rows) => {
        const last = rows[rows.length - 1];
        if (last?.kind === "assistant" && last.streaming) {
          return [...rows.slice(0, -1), { ...last, streaming: false }];
        }
        return rows;
      });
      setSending(false);
      return;
    }

    if (event.kind === "session_state") {
      runSessionIdRef.current = event.session_id;
      if (!event.resumed && event.transcript_len === 0) {
        return;
      }
      setChatLog((rows) => [
        ...rows,
        {
          kind: "notice",
          id: randId(),
          body: event.resumed
            ? `Resumed server context with ${event.transcript_len} message(s).`
            : "Started a fresh server context.",
        },
      ]);
      return;
    }

    if (event.kind === "context_notice") {
      setChatLog((rows) => [
        ...rows,
        { kind: "notice", id: randId(), body: event.message },
      ]);
      return;
    }

    if (event.kind === "error") {
      setLastError(event.message);
      setChatLog((rows) => [
        ...rows,
        { kind: "error", id: randId(), body: event.message },
      ]);
      setSending(false);
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
            { kind: "error", id: randId(), body: message },
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

  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [chatLog]);

  const sendChat = useCallback(async () => {
    const text = input.trim();
    if (!text || sending) return;

    setLastError(null);
    setSending(true);
    setChatLog((rows) => [...rows, { kind: "user", id: randId(), body: text }]);

    const payload = JSON.stringify({
      message: text,
      read_only: readOnly,
      dry_run: dryRun,
      strict: strictMode,
      ...(runSessionIdRef.current
        ? { session_id: runSessionIdRef.current }
        : {}),
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
        session_id?: string;
      };

      if (!response.ok || body.error) {
        const message = body.error ?? `HTTP ${response.status}`;
        setLastError(message);
        setChatLog((rows) => [
          ...rows,
          { kind: "error", id: randId(), body: message },
        ]);
        setSending(false);
        return;
      }

      if (typeof body.session_id === "string" && body.session_id.length > 0) {
        runSessionIdRef.current = body.session_id;
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
            kind: "assistant",
            id: randId(),
            body: formatJson(body.result),
          },
        ]);
      }

      setSending(false);
      void fetchHistory();
    } catch (error) {
      const message = String(error);
      setLastError(message);
      setChatLog((rows) => [
        ...rows,
        { kind: "error", id: randId(), body: message },
      ]);
      setSending(false);
    }
  }, [dryRun, fetchHistory, handleAgentEvent, input, readOnly, sending, strictMode, token]);

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

  const appBannerTitle = publicCfg?.banner_label ?? publicCfg?.app_name;
  const suggestions = useMemo(
    () => promptSuggestions(schema, appBannerTitle),
    [schema, appBannerTitle],
  );

  const activeProviderName =
    publicCfg?.active_provider ?? publicCfg?.default_provider ?? "not configured";
  const activeProvider = publicCfg?.provider_statuses?.find(
    (p) => p.name === activeProviderName,
  );


  /* ---------- render ---------- */

  const refreshAll = useCallback(() => {
    void fetchCfg();
    void fetchSchema();
    void fetchTools();
    void fetchHistory();
    connectWs();
  }, [fetchCfg, fetchSchema, fetchTools, fetchHistory, connectWs]);

  return (
    <div className="app-bg flex h-full min-h-0 text-fg">
      {/* left rail */}
      <aside className="flex w-[60px] flex-none flex-col items-center border-r border-border bg-surface py-3">
        <div className="mb-4 flex h-8 w-8 items-center justify-center rounded-md bg-fg text-[11px] font-bold text-bg">
          ap
        </div>
        <div className="flex flex-1 flex-col gap-2">
          <RailButton active={tab === "chat"} onClick={() => setTab("chat")} label="Chat">
            <IconChat />
          </RailButton>
          <RailButton active={tab === "tools"} onClick={() => setTab("tools")} label="Tools">
            <IconTools />
          </RailButton>
          <RailButton
            active={tab === "history"}
            onClick={() => setTab("history")}
            label="History"
          >
            <IconHistory />
          </RailButton>
          <RailButton
            active={tab === "settings"}
            onClick={() => setTab("settings")}
            label="Settings"
          >
            <IconSettings />
          </RailButton>
        </div>
        <div className="pb-1" title={`socket ${wsStatus}`}>
          <span className={`dot dot-${wsStatus === "open" ? "live" : wsStatus === "err" ? "err" : "idle"}`} />
        </div>
      </aside>

      {/* main workspace */}
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        {/* top bar: slim, real */}
        <header className="flex flex-none items-center gap-3 border-b border-border bg-surface px-4 py-2">
          <h1 className="text-[13px] font-semibold text-fg">
            appctl <span className="text-muted font-normal ml-1">/ {appBannerTitle ?? "app"}</span>
          </h1>
          <span className="hidden text-[12px] text-muted sm:inline">
            {sourceLabel(publicCfg?.sync_source ?? schema?.source)}
            {(publicCfg?.base_url ?? schema?.base_url) && (
              <>
                {" · "}
                <span className="font-mono text-fg-dim">
                  {publicCfg?.base_url ?? schema?.base_url}
                </span>
              </>
            )}
          </span>
          <div className="ml-auto flex items-center gap-2">
            {activeProvider ? (
              <span className="inline-flex items-center gap-1.5 rounded-md border border-border bg-surface px-2 py-1 text-[11px] text-muted">
                <span className="font-medium text-fg">{activeProviderName}</span>
                <span>·</span>
                <span className="font-mono text-[11px]">{activeProvider.model}</span>
                {!activeProvider.verified && (
                  <span className="rounded bg-amber-500/10 px-1 text-[10px] text-amber-400">
                    unconfirmed
                  </span>
                )}
              </span>
            ) : (
              <span className="rounded-md border border-amber-500/20 bg-amber-500/10 px-2 py-1 text-[11px] text-amber-400">
                no provider
              </span>
            )}
            <button
              type="button"
              onClick={refreshAll}
              title="Refresh"
              aria-label="Refresh"
              className="flex h-7 w-7 items-center justify-center rounded-md border border-border bg-surface text-muted transition hover:border-border-strong hover:text-fg"
            >
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
                <path d="M21 12a9 9 0 1 1-3-6.7" />
                <path d="M21 4v5h-5" />
              </svg>
            </button>
          </div>
        </header>

        {lastError && (
          <div className="flex flex-none items-center gap-2 border-b border-rose-400/20 bg-rose-400/5 px-4 py-1.5 text-[12px] text-rose-100">
            <span className="font-semibold">error</span>
            <span className="flex-1 truncate">{lastError}</span>
            <button
              type="button"
              onClick={() => setLastError(null)}
              className="text-rose-200 hover:text-rose-100"
            >
              dismiss
            </button>
          </div>
        )}

        {/* body */}
        <main className="flex min-h-0 flex-1 flex-col overflow-hidden">
          {tab === "chat" && (
            <ChatWorkspace
              chatLog={chatLog}
              scrollRef={scrollRef}
              input={input}
              setInput={setInput}
              sending={sending}
              sendChat={sendChat}
              readOnly={readOnly}
              dryRun={dryRun}
              strictMode={strictMode}
              setReadOnly={setReadOnly}
              setDryRun={setDryRun}
              setStrictMode={setStrictMode}
              serverReadOnly={publicCfg?.read_only ?? false}
              serverDryRun={publicCfg?.dry_run ?? false}
              serverStrictMode={publicCfg?.strict ?? false}
              suggestions={suggestions}
              wsStatus={wsStatus}
              connectWs={connectWs}
            />
          )}
          {tab === "tools" && <ToolsPanel actions={actions} tools={tools} />}
          {tab === "history" && <HistoryPanel history={history} />}
          {tab === "settings" && (
            <SettingsPanel
              token={token}
              saveToken={saveToken}
              publicCfg={publicCfg}
              schema={schema}
              summary={summary}
              refreshAll={refreshAll}
            />
          )}
        </main>
      </div>
    </div>
  );
}

/* ---------- chat workspace ---------- */

function ChatWorkspace({
  chatLog,
  scrollRef,
  input,
  setInput,
  sending,
  sendChat,
  readOnly,
  dryRun,
  strictMode,
  setReadOnly,
  setDryRun,
  setStrictMode,
  serverReadOnly,
  serverDryRun,
  serverStrictMode,
  suggestions,
  wsStatus,
  connectWs,
}: {
  chatLog: ChatEntry[];
  scrollRef: React.MutableRefObject<HTMLDivElement | null>;
  input: string;
  setInput: (v: string) => void;
  sending: boolean;
  sendChat: () => void;
  readOnly: boolean;
  dryRun: boolean;
  strictMode: boolean;
  setReadOnly: (v: boolean) => void;
  setDryRun: (v: boolean) => void;
  setStrictMode: (v: boolean) => void;
  serverReadOnly: boolean;
  serverDryRun: boolean;
  serverStrictMode: boolean;
  suggestions: string[];
  wsStatus: string;
  connectWs: () => void;
}) {
  const isEmpty = chatLog.length === 0;
  const readOnlyLocked = serverReadOnly;
  const dryRunLocked = serverDryRun;
  const strictLocked = serverStrictMode;
  return (
    <section className="flex min-h-0 flex-1 flex-col">
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-[860px] space-y-5 px-6 py-6">
          {isEmpty ? (
            <EmptyHero />
          ) : (
            chatLog.map((entry) => {
              if (entry.kind === "user") {
                return <UserMessage key={entry.id} body={entry.body} />;
              }
              if (entry.kind === "assistant") {
                return (
                  <AssistantMessage
                    key={entry.id}
                    body={entry.body}
                    streaming={entry.streaming}
                  />
                );
              }
              if (entry.kind === "tool_call") {
                return (
                  <ToolCard
                    key={entry.id}
                    name={entry.name}
                    args={entry.args}
                    status={entry.status}
                    duration_ms={entry.duration_ms}
                    result={entry.result}
                  />
                );
              }
              if (entry.kind === "notice") {
                return <NoticeMessage key={entry.id} body={entry.body} />;
              }
              return <ErrorMessage key={entry.id} body={entry.body} />;
            })
          )}
          {sending && !isEmpty && (
            <div className="flex items-center gap-2 pl-10 text-[12px] text-muted">
              <span className="h-1.5 w-1.5 animate-pulse-dot rounded-full bg-fg" />
              thinking…
            </div>
          )}
        </div>
      </div>

      {/* composer pinned to viewport bottom */}
      <div className="flex-none bg-bg px-4 pb-6 pt-4">
        <div className="mx-auto w-full max-w-[860px]">
          {isEmpty && suggestions.length > 0 && (
            <div className="mb-3 flex flex-wrap gap-2">
              {suggestions.map((p) => (
                <button
                  key={p}
                  type="button"
                  onClick={() => setInput(p)}
                  className="rounded-md border border-border bg-surface px-3 py-1.5 text-left text-[12px] text-muted transition hover:border-border-strong hover:text-fg"
                >
                  {p}
                </button>
              ))}
            </div>
          )}

          <div className="rounded-lg border border-border bg-surface shadow-sm transition focus-within:border-border-strong focus-within:ring-1 focus-within:ring-border-strong">
            <textarea
              aria-label="Chat message"
              className="block w-full resize-none bg-transparent px-4 pt-3 pb-2 text-[14px] leading-relaxed text-fg outline-none placeholder:text-muted"
              rows={2}
              placeholder="Message appctl…"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  sendChat();
                }
              }}
            />
            <div className="flex items-center gap-2 px-2 pb-2 pt-1">
              <Toggle
                label="Read-only"
                hint={
                  readOnlyLocked
                    ? "Server is already enforcing read-only mode."
                    : "Blocks any write or delete action."
                }
                checked={readOnly}
                disabled={readOnlyLocked}
                onChange={setReadOnly}
              />
              <Toggle
                label="Dry-run"
                hint={
                  dryRunLocked
                    ? "Server is already enforcing dry-run mode."
                    : "Shows what would happen without executing."
                }
                checked={dryRun}
                disabled={dryRunLocked}
                onChange={setDryRun}
              />
              <Toggle
                label="Strict"
                hint={
                  strictLocked
                    ? "Server is already enforcing strict mode."
                    : "Blocks inferred HTTP tools until verified by doctor."
                }
                checked={strictMode}
                disabled={strictLocked}
                onChange={setStrictMode}
              />
              <div className="ml-auto flex items-center gap-3 text-[12px] text-muted">
                {wsStatus !== "open" && (
                  <button
                    type="button"
                    onClick={connectWs}
                    className="hover:text-fg underline decoration-dotted underline-offset-2"
                  >
                    reconnect
                  </button>
                )}
                <button
                  type="button"
                  onClick={sendChat}
                  disabled={sending || !input.trim()}
                  className="inline-flex items-center gap-1.5 rounded-md bg-fg px-3 py-1.5 font-medium text-bg transition hover:bg-gray-200 disabled:cursor-not-allowed disabled:bg-border-strong disabled:text-muted"
                >
                  {sending ? "..." : "Send"}
                </button>
              </div>
            </div>
          </div>

          <div className="mt-2 flex items-center justify-between px-1 text-[11px] text-muted">
            <span>appctl operator console</span>
            <span className="flex items-center gap-1.5">
              <span className={`dot dot-${wsStatus === "open" ? "live" : wsStatus === "err" ? "err" : "idle"}`} />
              {wsStatus}
            </span>
          </div>
        </div>
      </div>
    </section>
  );
}

function EmptyHero() {
  return (
    <div className="pt-12 pb-4 text-center">
      <h2 className="text-[14px] font-medium text-fg">Chat</h2>
      <p className="mt-2 text-[13px] text-muted">Messages go to the model; appctl runs the tool calls.</p>
    </div>
  );
}

function StatMini({
  label,
  value,
  tone,
}: {
  label: string;
  value: number;
  tone?: "warn" | "danger";
}) {
  const color =
    tone === "warn" ? "text-amber-400" : tone === "danger" ? "text-rose-400" : "text-fg";
  return (
    <div className="rounded-lg border border-border bg-surface p-4">
      <div className="text-[12px] font-medium text-muted">
        {label}
      </div>
      <div className={`mt-1 text-2xl font-semibold tracking-tight ${color}`}>{value}</div>
    </div>
  );
}

/* ---------- tools panel ---------- */

function ToolsPanel({
  actions,
  tools,
}: {
  actions: (Action & { resourceName: string })[];
  tools: ToolDef[];
}) {
  const [query, setQuery] = useState("");
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return actions;
    return actions.filter(
      (a) =>
        a.name.toLowerCase().includes(q) ||
        a.resourceName.toLowerCase().includes(q) ||
        (a.description ?? "").toLowerCase().includes(q),
    );
  }, [actions, query]);

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-6 py-6">
      <div className="mx-auto w-full max-w-[1200px]">
        <div className="mb-5 flex flex-wrap items-end justify-between gap-3">
          <div>
            <h2 className="text-[18px] font-semibold text-fg">Tools</h2>
            <p className="mt-1 text-[13px] text-muted">
              From the synced schema: name, safety, and how the call is made.
            </p>
          </div>
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search tools…"
            className="w-[280px] rounded-md border border-border bg-surface px-3 py-1.5 text-[13px] text-fg outline-none placeholder:text-muted focus:border-border-strong"
          />
        </div>

        {filtered.length === 0 ? (
          <div className="rounded-lg border border-border bg-surface p-10 text-center text-[13px] text-muted">
            No tools match your search. Try another term or run{" "}
            <code className="font-mono text-fg">appctl sync</code>.
          </div>
        ) : (
          <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-2">
            {filtered.map((action) => {
              const tool = tools.find((item) => item.name === action.name);
              const required =
                tool?.input_schema?.required ??
                action.parameters.filter((f) => f.required).map((f) => f.name);
              const st = safetyTone(action.safety);
              const pt = provenanceTone(action.provenance);
              return (
                <article key={action.name} className="rounded-lg border border-border bg-surface p-5">
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="min-w-0">
                      <div className="text-[11px] font-medium text-muted">
                        {action.resourceName}
                      </div>
                      <h3 className="mt-1 truncate font-mono text-[14px] text-fg">
                        {action.name}
                      </h3>
                      <p className="mt-1.5 text-[13px] leading-relaxed text-muted">
                        {action.description ?? tool?.description ?? "No description."}
                      </p>
                    </div>
                    <div className="flex flex-none flex-wrap gap-1.5">
                      <span className={`rounded-md border px-2 py-0.5 text-[11px] ${st.cls}`}>
                        {st.label}
                      </span>
                      <span className={`rounded-md border px-2 py-0.5 text-[11px] ${pt.cls}`}>
                        {pt.label}
                      </span>
                    </div>
                  </div>
                  <div className="mt-4 rounded-md border border-border bg-panel p-3 font-mono text-[12px] text-fg-dim">
                    {transportLabel(action.transport)}
                  </div>
                  <div className="mt-3 grid grid-cols-2 gap-3 text-[12px]">
                    <KV
                      k="Required"
                      v={required.length > 0 ? required.join(", ") : "none"}
                      mono={required.length > 0}
                    />
                    <KV k="Params" v={String(action.parameters.length)} />
                  </div>
                </article>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

/* ---------- history panel ---------- */

function HistoryPanel({ history }: { history: HistoryEntry[] }) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-8 py-8">
      <div className="mx-auto w-full max-w-[1200px]">
        <div className="mb-6 flex items-center justify-between">
          <div>
            <h2 className="text-[20px] font-semibold text-fg">History</h2>
            <p className="mt-1 text-[13px] text-muted">Recorded tool calls (same data as <code className="font-mono">appctl history</code>).</p>
          </div>
        </div>

        {history.length === 0 ? (
          <div className="rounded-lg border border-border bg-surface p-12 text-center text-[13px] text-muted">
            No executions recorded yet.
          </div>
        ) : (
          <div className="rounded-lg border border-border bg-surface overflow-hidden">
            <table className="w-full text-left text-[13px]">
              <thead className="border-b border-border bg-panel text-[11px] uppercase tracking-wider text-muted">
                <tr>
                  <th className="px-4 py-3 font-medium">Status</th>
                  <th className="px-4 py-3 font-medium">Session</th>
                  <th className="px-4 py-3 font-medium">Tool</th>
                  <th className="px-4 py-3 font-medium">Timestamp</th>
                  <th className="px-4 py-3 font-medium text-right">Details</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border">
                {history.map((h) => {
                  const isExpanded = expanded[h.id];
                  return (
                    <Fragment key={h.id}>
                      <tr className="hover:bg-panel/50 transition group">
                        <td className="px-4 py-3">
                          <div className="flex items-center gap-2">
                            <span
                              className={`h-2 w-2 rounded-full ${h.status === "ok" ? "bg-emerald-400" : "bg-rose-400"}`}
                            />
                            <span className="text-fg-dim">{h.status === "ok" ? "Success" : "Error"}</span>
                          </div>
                        </td>
                        <td className="px-4 py-3">
                          <div className="flex flex-col">
                            <span className="font-mono text-fg">
                              {h.session_name || "interactive"}
                            </span>
                            <span className="text-[11px] text-muted font-mono">
                              {h.session_id}
                            </span>
                          </div>
                        </td>
                        <td className="px-4 py-3 font-mono text-fg">{h.tool}</td>
                        <td className="px-4 py-3 text-muted font-mono text-[12px]">{formatTs(h.ts)}</td>
                        <td className="px-4 py-3 text-right">
                          <button
                            onClick={() => setExpanded((prev) => ({ ...prev, [h.id]: !isExpanded }))}
                            className="text-[12px] font-medium text-muted hover:text-fg transition"
                          >
                            {isExpanded ? "Hide" : "View"}
                          </button>
                        </td>
                      </tr>
                      {isExpanded && (
                        <tr className="bg-panel border-t-0">
                          <td colSpan={5} className="p-4">
                            <div className="grid gap-4 lg:grid-cols-2">
                              <div>
                                <div className="mb-2 text-[11px] font-medium text-muted uppercase tracking-wider">Arguments</div>
                                <pre className="whitespace-pre-wrap break-words rounded border border-border bg-surface p-3 font-mono text-[11px] leading-relaxed text-fg-dim">
                                  {previewJson(h.arguments_json, 1000)}
                                </pre>
                              </div>
                              <div>
                                <div className="mb-2 text-[11px] font-medium text-muted uppercase tracking-wider">Response</div>
                                <pre className="whitespace-pre-wrap break-words rounded border border-border bg-surface p-3 font-mono text-[11px] leading-relaxed text-fg-dim">
                                  {previewJson(h.response_json, 1000)}
                                </pre>
                              </div>
                            </div>
                          </td>
                        </tr>
                      )}
                    </Fragment>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
}

/* ---------- settings panel ---------- */

function SettingsPanel({
  token,
  saveToken,
  publicCfg,
  schema,
  summary,
  refreshAll,
}: {
  token: string;
  saveToken: (v: string) => void;
  publicCfg: PublicConfig | null;
  schema: SchemaShape | null;
  summary: { resources: number; actionCount: number; writes: number; destructive: number };
  refreshAll: () => void;
}) {
  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-6 py-6">
      <div className="mx-auto w-full max-w-[1000px]">
        <div className="mb-8">
          <h2 className="text-[18px] font-semibold text-fg">Settings</h2>
          <p className="mt-1 text-[13px] text-muted">
            Manage daemon configuration, providers, and project sync state.
          </p>
        </div>

        <div className="divide-y divide-border border-t border-border">
          {/* Usage & Limits */}
          <section className="grid gap-6 py-8 md:grid-cols-[240px_1fr]">
            <div>
              <h3 className="text-[14px] font-semibold text-fg">Usage & Limits</h3>
              <p className="mt-1 text-[13px] text-muted">
                Overview of the tools and resources available to the agent.
              </p>
            </div>
            <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
              <StatMini label="Resources" value={summary.resources} />
              <StatMini label="Actions" value={summary.actionCount} />
              <StatMini label="Writes" value={summary.writes} tone="warn" />
              <StatMini label="Destructive" value={summary.destructive} tone="danger" />
            </div>
          </section>

          {/* Providers */}
          <section className="grid gap-6 py-8 md:grid-cols-[240px_1fr]">
            <div>
              <h3 className="text-[14px] font-semibold text-fg">AI Providers</h3>
              <p className="mt-1 text-[13px] text-muted">
                Configured models and their connection status.
              </p>
            </div>
            <div className="grid gap-4 md:grid-cols-2">
              {(publicCfg?.provider_statuses?.length ?? 0) === 0 ? (
                <div className="rounded-md border border-border bg-surface col-span-full p-5 text-[13px] text-muted">
                  No providers configured yet. Run <code className="font-mono text-fg">appctl init</code> to add one.
                </div>
              ) : (
                publicCfg?.provider_statuses?.map((provider) => (
                  <ProviderCard
                    key={provider.name}
                    provider={provider}
                    isActive={provider.name === publicCfg.active_provider}
                  />
                ))
              )}
            </div>
          </section>

          {/* Authentication */}
          <section className="grid gap-6 py-8 md:grid-cols-[240px_1fr]">
            <div>
              <h3 className="text-[14px] font-semibold text-fg">Authentication</h3>
              <p className="mt-1 text-[13px] text-muted">
                Secure access to the local daemon API.
              </p>
            </div>
            <div className="max-w-md">
              <label className="block">
                <span className="text-[12px] font-medium text-muted">Bearer token</span>
                <input
                  type="password"
                  className="mt-2 w-full rounded-md border border-border bg-surface px-3 py-2 text-[13px] text-fg outline-none focus:border-border-strong"
                  value={token}
                  onChange={(e) => saveToken(e.target.value)}
                  placeholder="Only needed if started with --token"
                />
              </label>
              <div className="mt-4 flex flex-wrap gap-3">
                <button
                  type="button"
                  onClick={refreshAll}
                  className="inline-flex items-center gap-2 rounded-md bg-fg px-3 py-1.5 text-[12px] font-medium text-bg transition hover:bg-gray-200"
                >
                  Refresh runtime
                </button>
                <button
                  type="button"
                  onClick={() => saveToken("")}
                  className="rounded-md border border-border bg-surface px-3 py-1.5 text-[12px] font-medium text-fg transition hover:border-border-strong hover:text-fg"
                >
                  Clear token
                </button>
              </div>
            </div>
          </section>

          {/* Project Configuration */}
          <section className="grid gap-6 py-8 md:grid-cols-[240px_1fr]">
            <div>
              <h3 className="text-[14px] font-semibold text-fg">Project Configuration</h3>
              <p className="mt-1 text-[13px] text-muted">
                What this daemon knows about your synced app.
              </p>
            </div>
            <div className="rounded-lg border border-border bg-surface p-5 max-w-2xl">
              <div className="space-y-4">
                <KV k="Description" v={publicCfg?.description ?? "not set"} />
                <KV k="Schema source" v={sourceLabel(schema?.source ?? publicCfg?.sync_source)} />
                <KV
                  k="Target URL"
                  v={
                    <span className="break-all font-mono text-[12px]">
                      {schema?.base_url ?? publicCfg?.base_url ?? "not set"}
                    </span>
                  }
                />
                <KV
                  k="Server defaults"
                  v={`read-only ${publicCfg?.read_only ? "on" : "off"} · dry-run ${publicCfg?.dry_run ? "on" : "off"} · strict ${publicCfg?.strict ? "on" : "off"}`}
                />
                <KV
                  k="Request safety"
                  v={`The browser can add extra safety, but it cannot relax server-enforced flags. Mutating confirmation is ${publicCfg?.confirm_default ? "auto-approved by this server" : "controlled by the server process"}.`}
                />
                <KV
                  k="Local files"
                  v={
                    <span className="font-mono text-[12px]">
                      .appctl/config.toml · schema.json · tools.json · history.db
                    </span>
                  }
                />
              </div>
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}

function ProviderCard({
  provider,
  isActive,
}: {
  provider: ProviderRuntimeStatus;
  isActive: boolean;
}) {
  return (
    <article
      className={`rounded-lg border bg-surface p-4 ${isActive ? "border-fg" : "border-border"}`}
    >
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h4 className="truncate text-[14px] font-semibold text-fg">{provider.name}</h4>
            {isActive && <Pill tone="accent">active</Pill>}
          </div>
          <div className="mt-0.5 truncate text-[12px] text-muted">
            {provider.kind} · {authKindLabel(provider.auth_status.kind)}
          </div>
        </div>
        <div className="flex flex-wrap gap-1.5">
          {provider.auth_status.configured ? (
            <Pill tone="success">configured</Pill>
          ) : (
            <Pill tone="warn">action needed</Pill>
          )}
          {provider.verified ? (
            <Pill tone="success">connection confirmed</Pill>
          ) : (
            <Pill tone="warn">connection not confirmed</Pill>
          )}
        </div>
      </div>
      <div className="mt-4 grid grid-cols-2 gap-3 text-[12px]">
        <KV k="Model" v={provider.model} mono />
        <KV
          k="Base URL"
          v={<span className="break-all font-mono text-[11px]">{provider.base_url}</span>}
        />
        {provider.auth_status.secret_ref && (
          <KV k="Secret ref" v={provider.auth_status.secret_ref} mono />
        )}
        {provider.auth_status.profile && <KV k="Profile" v={provider.auth_status.profile} mono />}
        {provider.auth_status.project_id && (
          <KV k="Project" v={provider.auth_status.project_id} mono />
        )}
        <KV k="Expires" v={formatExpiry(provider.auth_status.expires_at)} />
      </div>

      {!provider.auth_status.configured && provider.auth_status.recovery_hint && (
        <div className="mt-3 rounded-xl border border-amber-400/30 bg-amber-400/10 p-3 text-[12px] leading-relaxed text-amber-100">
          {provider.auth_status.recovery_hint}
        </div>
      )}
      {provider.auth_status.configured && !provider.verified && (
        <div className="mt-3 rounded-xl border border-amber-400/30 bg-amber-400/10 p-3 text-[12px] leading-relaxed text-amber-100">
          Config and key are saved, but the last live check didn’t succeed. To confirm the
          connection, run:
          <pre className="mt-2 whitespace-pre-wrap break-words font-mono text-[11.5px] text-amber-50">
            appctl auth provider login {provider.name}
          </pre>
        </div>
      )}
    </article>
  );
}
