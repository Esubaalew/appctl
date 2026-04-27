//! Structured events emitted while the agent runs (terminal UI, WebSocket, etc.).

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Ok,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    UserPrompt {
        text: String,
    },
    AwaitingInput,
    /// Incremental assistant text (unused until LLM streaming is wired).
    AssistantDelta {
        text: String,
    },
    AssistantMessage {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
    ToolResult {
        id: String,
        result: Value,
        status: ToolStatus,
        #[serde(default)]
        duration_ms: u64,
    },
    Error {
        message: String,
    },
    SessionState {
        session_id: String,
        transcript_len: usize,
        #[serde(default)]
        resumed: bool,
    },
    ContextNotice {
        message: String,
    },
    Done,
}
