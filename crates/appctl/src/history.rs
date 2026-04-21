use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    config::ConfigPaths,
    executor::{ExecutionContext, ExecutionRequest, ExecutionResult, Executor},
    safety::SafetyMode,
    sync::load_schema,
};

#[derive(Debug, Clone)]
pub struct HistoryStore {
    connection: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub ts: DateTime<Utc>,
    pub session_id: String,
    pub tool: String,
    pub arguments_json: Value,
    pub request_snapshot_json: Option<Value>,
    pub response_json: Option<Value>,
    pub status: String,
    pub undone: bool,
}

#[derive(Debug, Clone)]
pub struct HistoryCommand {
    pub last: usize,
    pub undo: Option<i64>,
}

impl HistoryStore {
    pub fn open(paths: &ConfigPaths) -> Result<Self> {
        paths.ensure()?;
        let connection = Connection::open(&paths.history)
            .with_context(|| format!("failed to open {}", paths.history.display()))?;
        let store = Self {
            connection: Arc::new(Mutex::new(connection)),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        self.connection.lock().unwrap().execute_batch(
            "create table if not exists actions (
                id integer primary key,
                ts text not null,
                session_id text not null,
                tool text not null,
                arguments_json text not null,
                request_snapshot_json text,
                response_json text,
                status text not null,
                undone integer default 0
            );",
        )?;
        Ok(())
    }

    pub fn log(
        &self,
        session_id: &str,
        request: &ExecutionRequest,
        result: &ExecutionResult,
        status: &str,
    ) -> Result<i64> {
        let arguments_json = serde_json::to_string(&request.arguments)?;
        let request_snapshot_json = serde_json::to_string(&result.request_snapshot)?;
        let response_json = serde_json::to_string(&result.output)?;
        let ts = Utc::now().to_rfc3339();

        let connection = self.connection.lock().unwrap();
        connection.execute(
            "insert into actions (ts, session_id, tool, arguments_json, request_snapshot_json, response_json, status)
             values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                ts,
                session_id,
                request.tool_name,
                arguments_json,
                request_snapshot_json,
                response_json,
                status
            ],
        )?;
        Ok(connection.last_insert_rowid())
    }

    pub fn list(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let connection = self.connection.lock().unwrap();
        let mut stmt = connection.prepare(
            "select id, ts, session_id, tool, arguments_json, request_snapshot_json, response_json, status, undone
             from actions order by id desc limit ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |row| {
            let ts: String = row.get(1)?;
            let request_snapshot: Option<String> = row.get(5)?;
            let response: Option<String> = row.get(6)?;

            Ok(HistoryEntry {
                id: row.get(0)?,
                ts: DateTime::parse_from_rfc3339(&ts)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                session_id: row.get(2)?,
                tool: row.get(3)?,
                arguments_json: serde_json::from_str(&row.get::<_, String>(4)?)
                    .unwrap_or(Value::Null),
                request_snapshot_json: request_snapshot
                    .as_deref()
                    .and_then(|raw| serde_json::from_str(raw).ok()),
                response_json: response
                    .as_deref()
                    .and_then(|raw| serde_json::from_str(raw).ok()),
                status: row.get(7)?,
                undone: row.get::<_, i64>(8)? == 1,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    pub fn get(&self, id: i64) -> Result<HistoryEntry> {
        self.list(10_000)?
            .into_iter()
            .find(|entry| entry.id == id)
            .with_context(|| format!("history entry {} not found", id))
    }

    pub fn mark_undone(&self, id: i64) -> Result<()> {
        self.connection
            .lock()
            .unwrap()
            .execute("update actions set undone = 1 where id = ?1", params![id])?;
        Ok(())
    }
}

pub async fn run_history_command(paths: &ConfigPaths, command: HistoryCommand) -> Result<()> {
    let store = HistoryStore::open(paths)?;
    if let Some(id) = command.undo {
        let schema = load_schema(paths)?;
        let entry = store.get(id)?;
        let Some(inverse_tool) = derive_inverse_tool(&entry) else {
            bail!("history entry {} cannot be undone automatically", id);
        };

        let executor = Executor::new(paths)?;
        let session_id = format!("undo-{}", Uuid::new_v4());
        let result = executor
            .execute(
                &schema,
                ExecutionContext {
                    session_id: session_id.clone(),
                    safety: SafetyMode {
                        read_only: false,
                        dry_run: false,
                        confirm: true,
                    },
                },
                inverse_tool,
            )
            .await?;
        store.mark_undone(id)?;
        println!("{}", serde_json::to_string_pretty(&result.output)?);
        return Ok(());
    }

    for entry in store.list(command.last)? {
        println!(
            "#{:>4} {} {} {}{}",
            entry.id,
            entry.ts.to_rfc3339(),
            entry.tool,
            entry.status,
            if entry.undone { " (undone)" } else { "" }
        );
    }
    Ok(())
}

fn derive_inverse_tool(entry: &HistoryEntry) -> Option<ExecutionRequest> {
    let arguments = entry.arguments_json.as_object()?.clone();
    if let Some(tool) = entry.tool.strip_prefix("create_") {
        let response = entry.response_json.as_ref()?.as_object()?;
        let id = response
            .get("id")
            .or_else(|| response.get("pk"))
            .cloned()
            .or_else(|| response.get("data").and_then(|v| v.get("id")).cloned())?;
        return Some(ExecutionRequest::new(
            format!("delete_{tool}"),
            Value::Object(serde_json::Map::from_iter([("id".to_string(), id)])),
        ));
    }

    if let Some(tool) = entry.tool.strip_prefix("delete_") {
        let snapshot = entry
            .request_snapshot_json
            .as_ref()?
            .get("pre_image")?
            .clone();
        return Some(ExecutionRequest::new(format!("create_{tool}"), snapshot));
    }

    if let Some(tool) = entry.tool.strip_prefix("update_") {
        let snapshot = entry
            .request_snapshot_json
            .as_ref()?
            .get("pre_image")?
            .clone();
        let mut payload = arguments;
        if let Some(previous) = snapshot.as_object() {
            for (key, value) in previous {
                payload.insert(key.clone(), value.clone());
            }
        }
        return Some(ExecutionRequest::new(
            format!("update_{tool}"),
            Value::Object(payload),
        ));
    }

    None
}
