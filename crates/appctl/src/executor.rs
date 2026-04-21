use anyhow::{Context, Result, bail};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sqlx::{Column, Row};

use crate::{
    config::{AppConfig, ConfigPaths, load_secret},
    safety::SafetyMode,
    schema::{Action, AuthStrategy, DatabaseKind, HttpMethod, Schema, SqlOperation, Transport},
};

#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub session_id: String,
    pub safety: SafetyMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub tool_name: String,
    pub arguments: Value,
    pub request_snapshot: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub output: Value,
    pub request_snapshot: Value,
}

pub struct Executor {
    client: reqwest::Client,
    config: AppConfig,
}

impl Executor {
    pub fn new(paths: &ConfigPaths) -> Result<Self> {
        let config = AppConfig::load_or_init(paths)?;
        Ok(Self {
            client: reqwest::Client::new(),
            config,
        })
    }

    pub async fn execute(
        &self,
        schema: &Schema,
        context: ExecutionContext,
        mut request: ExecutionRequest,
    ) -> Result<ExecutionResult> {
        let action = schema
            .action(&request.tool_name)
            .with_context(|| format!("tool '{}' not found", request.tool_name))?;
        context.safety.check(action, &request.arguments)?;

        request.request_snapshot = self
            .snapshot_before(schema, action, &request.arguments)
            .await?;

        if context.safety.dry_run {
            return Ok(ExecutionResult {
                output: json!({
                    "dry_run": true,
                    "tool": action.name,
                    "arguments": request.arguments,
                    "request_snapshot": request.request_snapshot,
                }),
                request_snapshot: request.request_snapshot,
            });
        }

        let mut result = match &action.transport {
            Transport::Http { .. } => self.execute_http(schema, action, &request.arguments).await,
            Transport::Sql { .. } => self.execute_sql(action, &request.arguments).await,
            Transport::Form { .. } => self.execute_form(action, &request.arguments).await,
            Transport::Mcp { .. } => self.execute_mcp(action, &request.arguments).await,
        }?;
        result.request_snapshot = request.request_snapshot;
        Ok(result)
    }

    async fn snapshot_before(
        &self,
        schema: &Schema,
        action: &Action,
        arguments: &Value,
    ) -> Result<Value> {
        if !action.name.starts_with("update_") && !action.name.starts_with("delete_") {
            return Ok(Value::Object(Map::new()));
        }

        let pre_image = match &action.transport {
            Transport::Http {
                method: _, path, ..
            } => {
                if let Some(id) = arguments.get("id") {
                    let get_path = path.replace("{id}", id.as_str().unwrap_or(&id.to_string()));
                    match self
                        .http_json(schema, HttpMethod::GET, &get_path, &Value::Null)
                        .await
                    {
                        Ok(json) => json,
                        Err(_) => Value::Null,
                    }
                } else {
                    Value::Null
                }
            }
            Transport::Sql {
                table,
                primary_key,
                database_kind,
                ..
            } => {
                if let (Some(pk), Some(id)) = (
                    primary_key,
                    arguments.get(primary_key.as_deref().unwrap_or("id")),
                ) {
                    self.fetch_sql_row(database_kind, table, pk, id)
                        .await
                        .unwrap_or(Value::Null)
                } else {
                    Value::Null
                }
            }
            _ => Value::Null,
        };

        Ok(json!({ "pre_image": pre_image }))
    }

    async fn execute_http(
        &self,
        schema: &Schema,
        action: &Action,
        arguments: &Value,
    ) -> Result<ExecutionResult> {
        let Transport::Http {
            method,
            path,
            query,
        } = &action.transport
        else {
            unreachable!();
        };
        let response = self
            .http_json_with_query(schema, method.clone(), path, query, arguments)
            .await?;
        Ok(ExecutionResult {
            output: response,
            request_snapshot: Value::Null,
        })
    }

    async fn execute_form(&self, action: &Action, arguments: &Value) -> Result<ExecutionResult> {
        let Transport::Form {
            method,
            action: url,
        } = &action.transport
        else {
            unreachable!();
        };
        let mut request = self.client.request(reqwest_method(method), url);
        if let Some(map) = arguments.as_object() {
            request = request.form(map);
        }
        let response = request.send().await?;
        Ok(ExecutionResult {
            output: json!({
                "status": response.status().as_u16(),
                "url": response.url().as_str(),
            }),
            request_snapshot: Value::Null,
        })
    }

    async fn execute_mcp(&self, action: &Action, arguments: &Value) -> Result<ExecutionResult> {
        let Transport::Mcp { server_url } = &action.transport else {
            unreachable!();
        };
        let tool = arguments
            .get("tool")
            .and_then(Value::as_str)
            .context("missing remote MCP tool name")?;
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": tool,
                "arguments": arguments.get("arguments").cloned().unwrap_or(Value::Object(Map::new()))
            }
        });
        let response = self
            .client
            .post(server_url)
            .json(&request)
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(ExecutionResult {
            output: response,
            request_snapshot: Value::Null,
        })
    }

    async fn execute_sql(&self, action: &Action, arguments: &Value) -> Result<ExecutionResult> {
        let Transport::Sql {
            database_kind,
            table,
            operation,
            primary_key,
        } = &action.transport
        else {
            unreachable!();
        };

        let connection_string = self
            .config
            .target
            .database_url
            .clone()
            .or_else(|| self.config.target.base_url.clone())
            .or_else(|| self.config.target.auth_header.clone())
            .unwrap_or_default();

        if connection_string.is_empty() {
            bail!(
                "database connection string not configured; set target.database_url in .appctl/config.toml"
            );
        }

        match database_kind {
            DatabaseKind::Postgres => {
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .max_connections(5)
                    .connect(&connection_string)
                    .await?;
                execute_sql_postgres(&pool, table, operation, primary_key.as_deref(), arguments)
                    .await
            }
            DatabaseKind::Mysql => {
                let pool = sqlx::mysql::MySqlPoolOptions::new()
                    .max_connections(5)
                    .connect(&connection_string)
                    .await?;
                execute_sql_mysql(&pool, table, operation, primary_key.as_deref(), arguments).await
            }
        }
    }

    async fn http_json(
        &self,
        schema: &Schema,
        method: HttpMethod,
        path: &str,
        body: &Value,
    ) -> Result<Value> {
        self.http_json_with_query(schema, method, path, &[], body)
            .await
    }

    async fn http_json_with_query(
        &self,
        schema: &Schema,
        method: HttpMethod,
        path: &str,
        query_fields: &[String],
        arguments: &Value,
    ) -> Result<Value> {
        let base_url = schema
            .base_url
            .clone()
            .or_else(|| self.config.target.base_url.clone())
            .context("schema has no base URL; pass --base-url or set target.base_url")?;
        let mut url = format!("{}{}", base_url.trim_end_matches('/'), path);
        let mut body_map = arguments.as_object().cloned().unwrap_or_default();

        for (key, value) in body_map.clone() {
            let placeholder = format!("{{{key}}}");
            if url.contains(&placeholder) {
                url = url.replace(&placeholder, &value_to_string(&value));
                body_map.remove(&key);
            }
        }

        let mut query = Vec::<(String, String)>::new();
        for name in query_fields {
            if let Some(value) = body_map.remove(name) {
                query.push((name.clone(), value_to_string(&value)));
            }
        }

        let mut request = self.client.request(reqwest_method(&method), &url);
        let headers = build_headers(
            &schema.auth,
            &self.config,
            schema.metadata.get("auth_header"),
        )?;
        request = request.headers(headers);
        if !query.is_empty() {
            request = request.query(&query);
        }
        if !body_map.is_empty() && !matches!(method, HttpMethod::GET | HttpMethod::DELETE) {
            request = request.json(&Value::Object(body_map));
        }
        let response = request.send().await?;
        let status = response.status();
        let text = response.text().await?;
        let parsed = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "text": text }));
        Ok(json!({
            "status": status.as_u16(),
            "data": parsed
        }))
    }

    async fn fetch_sql_row(
        &self,
        database_kind: &DatabaseKind,
        table: &str,
        primary_key: &str,
        id: &Value,
    ) -> Result<Value> {
        let connection_string = self
            .config
            .target
            .database_url
            .clone()
            .or_else(|| self.config.target.base_url.clone())
            .context("database connection string missing")?;
        match database_kind {
            DatabaseKind::Postgres => {
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .connect(&connection_string)
                    .await?;
                let sql = format!(
                    "select row_to_json(t) as row from (select * from {table} where {primary_key} = $1 limit 1) t"
                );
                let row: Option<Value> = sqlx::query_scalar(&sql)
                    .bind(value_to_string(id))
                    .fetch_optional(&pool)
                    .await?;
                Ok(row.unwrap_or(Value::Null))
            }
            DatabaseKind::Mysql => Ok(Value::Null),
        }
    }
}

impl ExecutionRequest {
    pub fn new(tool_name: String, arguments: Value) -> Self {
        Self {
            tool_name,
            arguments,
            request_snapshot: Value::Object(Map::new()),
        }
    }
}

fn build_headers(
    auth: &AuthStrategy,
    config: &AppConfig,
    inline_auth_header: Option<&Value>,
) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    if let Some(header_value) = inline_auth_header.and_then(Value::as_str) {
        headers.insert(AUTHORIZATION, HeaderValue::from_str(header_value)?);
    }

    if let Some(auth_header) = &config.target.auth_header {
        headers.insert(AUTHORIZATION, HeaderValue::from_str(auth_header)?);
    }

    match auth {
        AuthStrategy::None => {}
        AuthStrategy::ApiKey { header, env_ref } => {
            let key = secret_or_env(env_ref)?;
            headers.insert(
                HeaderName::from_bytes(header.as_bytes())?,
                HeaderValue::from_str(&key)?,
            );
        }
        AuthStrategy::Bearer { env_ref } => {
            let token = secret_or_env(env_ref)?;
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}"))?,
            );
        }
        AuthStrategy::Basic {
            username_ref,
            password_ref,
        } => {
            let username = secret_or_env(username_ref)?;
            let password = secret_or_env(password_ref)?;
            let encoded = {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"))
            };
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Basic {encoded}"))?,
            );
        }
        AuthStrategy::Cookie {
            env_ref,
            session_file: _,
        } => {
            if let Some(env_ref) = env_ref {
                let cookie = secret_or_env(env_ref)?;
                headers.insert(
                    HeaderName::from_static("cookie"),
                    HeaderValue::from_str(&cookie)?,
                );
            }
        }
        AuthStrategy::OAuth2 { provider, .. } => {
            if let Some(token) =
                crate::auth::oauth::load_access_token(provider.as_deref().unwrap_or("default"))
            {
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {token}"))?,
                );
            }
        }
    }
    Ok(headers)
}

fn secret_or_env(name: &str) -> Result<String> {
    load_secret(name).or_else(|_| {
        std::env::var(name).with_context(|| format!("missing secret or env var '{name}'"))
    })
}

fn reqwest_method(method: &HttpMethod) -> reqwest::Method {
    match method {
        HttpMethod::GET => reqwest::Method::GET,
        HttpMethod::POST => reqwest::Method::POST,
        HttpMethod::PUT => reqwest::Method::PUT,
        HttpMethod::PATCH => reqwest::Method::PATCH,
        HttpMethod::DELETE => reqwest::Method::DELETE,
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

async fn execute_sql_postgres(
    pool: &sqlx::Pool<sqlx::Postgres>,
    table: &str,
    operation: &SqlOperation,
    primary_key: Option<&str>,
    arguments: &Value,
) -> Result<ExecutionResult> {
    let primary_key = primary_key.unwrap_or("id");
    match operation {
        SqlOperation::Select => {
            let sql = format!(
                "select coalesce(json_agg(t), '[]'::json) as rows from (select * from {table} limit 100) t"
            );
            let rows: Value = sqlx::query_scalar(&sql).fetch_one(pool).await?;
            Ok(ExecutionResult {
                output: rows,
                request_snapshot: Value::Null,
            })
        }
        SqlOperation::GetByPk => {
            let id = arguments
                .get(primary_key)
                .or_else(|| arguments.get("id"))
                .context("missing primary key argument")?;
            let sql = format!(
                "select row_to_json(t) as row from (select * from {table} where {primary_key} = $1 limit 1) t"
            );
            let row: Option<Value> = sqlx::query_scalar(&sql)
                .bind(value_to_string(id))
                .fetch_optional(pool)
                .await?;
            Ok(ExecutionResult {
                output: row.unwrap_or(Value::Null),
                request_snapshot: Value::Null,
            })
        }
        SqlOperation::Insert => {
            let payload = arguments
                .as_object()
                .context("insert expects a JSON object")?;
            let columns: Vec<_> = payload.keys().cloned().collect();
            let values: Vec<_> = payload.values().cloned().collect();
            let placeholders = (1..=columns.len())
                .map(|i| format!("${i}"))
                .collect::<Vec<_>>();
            let sql = format!(
                "insert into {table} ({}) values ({}) returning row_to_json({table})",
                columns.join(", "),
                placeholders.join(", ")
            );
            let mut query = sqlx::query_scalar::<_, Value>(&sql);
            for value in values {
                query = query.bind(value_to_string(&value));
            }
            let row = query.fetch_one(pool).await?;
            Ok(ExecutionResult {
                output: row,
                request_snapshot: Value::Null,
            })
        }
        SqlOperation::UpdateByPk => {
            let payload = arguments
                .as_object()
                .context("update expects a JSON object")?;
            let id = payload
                .get(primary_key)
                .or_else(|| payload.get("id"))
                .context("missing primary key argument")?;
            let mut columns = Vec::new();
            let mut values = Vec::new();
            for (key, value) in payload {
                if key != primary_key && key != "id" {
                    columns.push(key.clone());
                    values.push(value.clone());
                }
            }
            if columns.is_empty() {
                bail!("update requires at least one mutable field");
            }
            let assignments = columns
                .iter()
                .enumerate()
                .map(|(index, column)| format!("{column} = ${}", index + 1))
                .collect::<Vec<_>>();
            let sql = format!(
                "update {table} set {} where {primary_key} = ${} returning row_to_json({table})",
                assignments.join(", "),
                columns.len() + 1
            );
            let mut query = sqlx::query_scalar::<_, Value>(&sql);
            for value in values {
                query = query.bind(value_to_string(&value));
            }
            query = query.bind(value_to_string(id));
            let row = query.fetch_one(pool).await?;
            Ok(ExecutionResult {
                output: row,
                request_snapshot: Value::Null,
            })
        }
        SqlOperation::DeleteByPk => {
            let id = arguments
                .get(primary_key)
                .or_else(|| arguments.get("id"))
                .context("missing primary key argument")?;
            let sql = format!(
                "delete from {table} where {primary_key} = $1 returning json_build_object('deleted', true, '{primary_key}', {primary_key})"
            );
            let row: Value = sqlx::query_scalar(&sql)
                .bind(value_to_string(id))
                .fetch_one(pool)
                .await?;
            Ok(ExecutionResult {
                output: row,
                request_snapshot: Value::Null,
            })
        }
    }
}

async fn execute_sql_mysql(
    pool: &sqlx::Pool<sqlx::MySql>,
    table: &str,
    operation: &SqlOperation,
    primary_key: Option<&str>,
    arguments: &Value,
) -> Result<ExecutionResult> {
    let primary_key = primary_key.unwrap_or("id");
    match operation {
        SqlOperation::Select => {
            let sql = format!("select * from {table} limit 100");
            let rows = sqlx::query(&sql).fetch_all(pool).await?;
            Ok(ExecutionResult {
                output: rows_to_json(rows),
                request_snapshot: Value::Null,
            })
        }
        SqlOperation::GetByPk => {
            let id = arguments
                .get(primary_key)
                .or_else(|| arguments.get("id"))
                .context("missing primary key argument")?;
            let sql = format!("select * from {table} where {primary_key} = ? limit 1");
            let rows = sqlx::query(&sql)
                .bind(value_to_string(id))
                .fetch_all(pool)
                .await?;
            Ok(ExecutionResult {
                output: rows_to_json(rows),
                request_snapshot: Value::Null,
            })
        }
        SqlOperation::Insert => {
            let payload = arguments
                .as_object()
                .context("insert expects a JSON object")?;
            let columns: Vec<_> = payload.keys().cloned().collect();
            let placeholders = columns.iter().map(|_| "?").collect::<Vec<_>>();
            let sql = format!(
                "insert into {table} ({}) values ({})",
                columns.join(", "),
                placeholders.join(", ")
            );
            let mut query = sqlx::query(&sql);
            for value in payload.values() {
                query = query.bind(value_to_string(value));
            }
            let result = query.execute(pool).await?;
            Ok(ExecutionResult {
                output: json!({ "rows_affected": result.rows_affected(), "last_insert_id": result.last_insert_id() }),
                request_snapshot: Value::Null,
            })
        }
        SqlOperation::UpdateByPk => {
            let payload = arguments
                .as_object()
                .context("update expects a JSON object")?;
            let id = payload
                .get(primary_key)
                .or_else(|| payload.get("id"))
                .context("missing primary key argument")?;
            let columns = payload
                .keys()
                .filter(|key| key.as_str() != primary_key && key.as_str() != "id")
                .cloned()
                .collect::<Vec<_>>();
            if columns.is_empty() {
                bail!("update requires at least one mutable field");
            }
            let assignments = columns
                .iter()
                .map(|column| format!("{column} = ?"))
                .collect::<Vec<_>>();
            let sql = format!(
                "update {table} set {} where {primary_key} = ?",
                assignments.join(", ")
            );
            let mut query = sqlx::query(&sql);
            for column in &columns {
                query = query.bind(value_to_string(payload.get(column).unwrap()));
            }
            let result = query.bind(value_to_string(id)).execute(pool).await?;
            Ok(ExecutionResult {
                output: json!({ "rows_affected": result.rows_affected() }),
                request_snapshot: Value::Null,
            })
        }
        SqlOperation::DeleteByPk => {
            let id = arguments
                .get(primary_key)
                .or_else(|| arguments.get("id"))
                .context("missing primary key argument")?;
            let sql = format!("delete from {table} where {primary_key} = ?");
            let result = sqlx::query(&sql)
                .bind(value_to_string(id))
                .execute(pool)
                .await?;
            Ok(ExecutionResult {
                output: json!({ "rows_affected": result.rows_affected(), "deleted": true }),
                request_snapshot: Value::Null,
            })
        }
    }
}

fn rows_to_json(rows: Vec<sqlx::mysql::MySqlRow>) -> Value {
    let mut out = Vec::new();
    for row in rows {
        let mut obj = Map::new();
        for column in row.columns() {
            let name = column.name();
            let value = row
                .try_get::<String, _>(name)
                .map(Value::String)
                .or_else(|_| row.try_get::<i64, _>(name).map(Value::from))
                .or_else(|_| row.try_get::<f64, _>(name).map(Value::from))
                .or_else(|_| row.try_get::<bool, _>(name).map(Value::from))
                .unwrap_or(Value::Null);
            obj.insert(name.to_string(), value);
        }
        out.push(Value::Object(obj));
    }
    Value::Array(out)
}
