use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use aws_sdk_dynamodb::types::AttributeValue;
use futures::TryStreamExt;
use mongodb::bson::{Document, doc, oid::ObjectId};
use redis::AsyncCommands;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use reqwest_cookie_store::CookieStoreMutex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sqlx::{Column, Row};

use crate::{
    auth::gcloud,
    config::{AppConfig, ConfigPaths, load_secret},
    safety::SafetyMode,
    schema::{
        Action, ApiKeyLocation, AuthStrategy, DatabaseKind, Field, HttpMethod, NoSqlOperation,
        ParameterLocation, Schema, SqlOperation, Transport,
    },
};

pub fn tool_http_status(value: &Value) -> Option<u16> {
    value
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|status| u16::try_from(status).ok())
}

pub fn tool_result_summary(value: &Value) -> Option<&str> {
    value.get("summary").and_then(Value::as_str)
}

pub fn tool_result_is_error(value: &Value) -> bool {
    value
        .get("ok")
        .and_then(Value::as_bool)
        .is_some_and(|ok| !ok)
        || tool_http_status(value).is_some_and(|status| status >= 400)
}

#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub session_id: String,
    pub session_name: Option<String>,
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
        let cookie_store = runtime_cookie_store(paths, &config);
        let mut builder = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60));
        if let Some(cookie_store) = cookie_store {
            builder = builder.cookie_provider(cookie_store);
        }
        let client = builder.build().context("failed to build HTTP client")?;
        Ok(Self { client, config })
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
        // Safety / confirmation is enforced at the call site (e.g. `run_agent`, MCP) so
        // interactive prompts are not interleaved with terminal spinners. Callers that
        // need checks must run `SafetyMode::check` before `execute`.

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
            Transport::NoSql { .. } => self.execute_nosql(action, &request.arguments).await,
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
                schema,
                primary_key,
                database_kind,
                ..
            } => {
                if let (Some(pk), Some(id)) = (
                    primary_key,
                    arguments.get(primary_key.as_deref().unwrap_or("id")),
                ) {
                    self.fetch_sql_row(database_kind, schema.as_deref(), table, pk, id)
                        .await
                        .unwrap_or(Value::Null)
                } else {
                    Value::Null
                }
            }
            Transport::NoSql {
                collection,
                primary_key,
                secondary_key,
                database_kind,
                ..
            } => {
                if let Some(pk) = primary_key
                    && let Some(id) = arguments.get(pk)
                {
                    self.fetch_nosql_row(
                        database_kind,
                        collection,
                        pk,
                        secondary_key.as_deref(),
                        id,
                        arguments,
                    )
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
            .http_json_with_query(
                schema,
                method.clone(),
                path,
                query,
                &action.parameters,
                arguments,
            )
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
            schema,
            table,
            operation,
            primary_key,
        } = &action.transport
        else {
            unreachable!();
        };

        let connection_string = runtime_database_source(&self.config).unwrap_or_default();

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
                execute_sql_postgres(
                    &pool,
                    schema.as_deref(),
                    table,
                    operation,
                    primary_key.as_deref(),
                    arguments,
                )
                .await
            }
            DatabaseKind::Mysql => {
                let pool = sqlx::mysql::MySqlPoolOptions::new()
                    .max_connections(5)
                    .connect(&connection_string)
                    .await?;
                execute_sql_mysql(
                    &pool,
                    schema.as_deref(),
                    table,
                    operation,
                    primary_key.as_deref(),
                    arguments,
                )
                .await
            }
            DatabaseKind::Sqlite => {
                let pool = sqlx::sqlite::SqlitePoolOptions::new()
                    .max_connections(5)
                    .connect(&connection_string)
                    .await?;
                execute_sql_sqlite(&pool, table, operation, primary_key.as_deref(), arguments).await
            }
            DatabaseKind::Mongodb
            | DatabaseKind::Redis
            | DatabaseKind::Firestore
            | DatabaseKind::Dynamodb => {
                bail!("non-SQL datastore passed to SQL executor")
            }
        }
    }

    async fn execute_nosql(&self, action: &Action, arguments: &Value) -> Result<ExecutionResult> {
        let Transport::NoSql {
            database_kind,
            collection,
            operation,
            primary_key,
            secondary_key,
        } = &action.transport
        else {
            unreachable!();
        };

        let source = runtime_database_source(&self.config)
            .context("database connection string not configured; set target.database_url in .appctl/config.toml")?;

        match database_kind {
            DatabaseKind::Mongodb => {
                execute_nosql_mongodb(
                    &source,
                    collection,
                    operation,
                    primary_key.as_deref(),
                    arguments,
                )
                .await
            }
            DatabaseKind::Redis => {
                execute_nosql_redis(&source, operation, primary_key.as_deref(), arguments).await
            }
            DatabaseKind::Firestore => {
                execute_nosql_firestore(
                    &self.client,
                    &source,
                    operation,
                    collection,
                    primary_key.as_deref(),
                    arguments,
                )
                .await
            }
            DatabaseKind::Dynamodb => {
                execute_nosql_dynamodb(
                    &source,
                    collection,
                    operation,
                    primary_key.as_deref(),
                    secondary_key.as_deref(),
                    arguments,
                )
                .await
            }
            DatabaseKind::Postgres | DatabaseKind::Mysql | DatabaseKind::Sqlite => {
                bail!("SQL datastore passed to NoSQL executor")
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
        self.http_json_with_query(schema, method, path, &[], &[], body)
            .await
    }

    async fn http_json_with_query(
        &self,
        schema: &Schema,
        method: HttpMethod,
        path: &str,
        query_fields: &[String],
        parameters: &[Field],
        arguments: &Value,
    ) -> Result<Value> {
        let base_url = schema
            .base_url
            .clone()
            .or_else(|| runtime_base_url(&self.config))
            .context(
                "no API base URL for HTTP tools. Do one of: re-sync with `--base-url` (e.g. \
                 `appctl sync --flask <dir> --base-url http://127.0.0.1:5000 --force`), or set \
                 `target.base_url` in .appctl/config.toml, or set env `APPCTL_BASE_URL`",
            )?;
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
                if optional_empty_query_value(parameters, name, &value) {
                    continue;
                }
                query.push((name.clone(), value_to_string(&value)));
            } else if let Some(raw) = self.config.target.default_query.get(name) {
                let resolved = resolve_target_default_query_value(raw).with_context(|| {
                    format!("resolving [target].default_query[{name}] (use env:VAR for env vars)")
                })?;
                if resolved.trim().is_empty() {
                    continue;
                }
                query.push((name.clone(), resolved));
            }
        }

        let mut request = self.client.request(reqwest_method(&method), &url);
        let mut headers = build_headers(
            &schema.auth,
            &self.config,
            schema.metadata.get("auth_header"),
        )?;
        for field in parameters {
            if !matches!(field.location, Some(ParameterLocation::Header)) {
                continue;
            }
            if let Some(value) = body_map.remove(&field.name) {
                if optional_empty_field_value(field, &value) {
                    continue;
                }
                headers.insert(
                    HeaderName::from_bytes(field.name.as_bytes())?,
                    HeaderValue::from_str(&value_to_string(&value))?,
                );
            }
        }
        drop_optional_empty_fields(&mut body_map, parameters);
        append_query_auth(&schema.auth, &mut query)?;
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
        let summary = summarize_http_status(status.as_u16(), &method, path);
        Ok(json!({
            "ok": status.is_success(),
            "status": status.as_u16(),
            "classification": classify_http_status(status.as_u16()),
            "summary": summary,
            "data": parsed
        }))
    }

    async fn fetch_sql_row(
        &self,
        database_kind: &DatabaseKind,
        schema: Option<&str>,
        table: &str,
        primary_key: &str,
        id: &Value,
    ) -> Result<Value> {
        let connection_string =
            runtime_database_source(&self.config).context("database connection string missing")?;
        let pkq = sql_ident_ansi(primary_key);
        match database_kind {
            DatabaseKind::Postgres => {
                let qt = sql_qualified_table_ansi(&DatabaseKind::Postgres, schema, table);
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .connect(&connection_string)
                    .await?;
                let sql = format!(
                    "select row_to_json(t) as row from (select * from {qt} where {pkq} = $1 limit 1) t"
                );
                let row: Option<Value> = sqlx::query_scalar(&sql)
                    .bind(value_to_string(id))
                    .fetch_optional(&pool)
                    .await?;
                Ok(row.unwrap_or(Value::Null))
            }
            DatabaseKind::Mysql => {
                let qt = sql_qualified_table_ansi(&DatabaseKind::Mysql, schema, table);
                let pool = sqlx::mysql::MySqlPoolOptions::new()
                    .connect(&connection_string)
                    .await?;
                let sql = format!("select * from {qt} where {pkq} = ? limit 1");
                let rows = sqlx::query(&sql)
                    .bind(value_to_string(id))
                    .fetch_all(&pool)
                    .await?;
                Ok(rows_to_json(rows)
                    .as_array()
                    .and_then(|rows| rows.first().cloned())
                    .unwrap_or(Value::Null))
            }
            DatabaseKind::Sqlite => {
                let qt = sql_ident_ansi(table);
                let pool = sqlx::sqlite::SqlitePoolOptions::new()
                    .connect(&connection_string)
                    .await?;
                let sql = format!("select * from {qt} where {pkq} = ? limit 1");
                let rows = sqlx::query(&sql)
                    .bind(value_to_string(id))
                    .fetch_all(&pool)
                    .await?;
                Ok(rows_to_json_sqlite(rows)
                    .as_array()
                    .and_then(|rows| rows.first().cloned())
                    .unwrap_or(Value::Null))
            }
            DatabaseKind::Mongodb
            | DatabaseKind::Redis
            | DatabaseKind::Firestore
            | DatabaseKind::Dynamodb => Ok(Value::Null),
        }
    }

    async fn fetch_nosql_row(
        &self,
        database_kind: &DatabaseKind,
        collection: &str,
        primary_key: &str,
        secondary_key: Option<&str>,
        id: &Value,
        arguments: &Value,
    ) -> Result<Value> {
        let source =
            runtime_database_source(&self.config).context("database connection string missing")?;
        match database_kind {
            DatabaseKind::Mongodb => execute_nosql_mongodb(
                &source,
                collection,
                &NoSqlOperation::GetByPk,
                Some(primary_key),
                &json!({ primary_key: id.clone() }),
            )
            .await
            .map(|result| result.output),
            DatabaseKind::Redis => execute_nosql_redis(
                &source,
                &NoSqlOperation::GetByPk,
                Some(primary_key),
                &json!({ primary_key: id.clone() }),
            )
            .await
            .map(|result| result.output),
            DatabaseKind::Firestore => execute_nosql_firestore(
                &self.client,
                &source,
                &NoSqlOperation::GetByPk,
                collection,
                Some(primary_key),
                &json!({ primary_key: id.clone() }),
            )
            .await
            .map(|result| result.output),
            DatabaseKind::Dynamodb => {
                let mut payload = Map::new();
                payload.insert(primary_key.to_string(), id.clone());
                if let Some(secondary_key) = secondary_key
                    && let Some(value) = arguments.get(secondary_key)
                {
                    payload.insert(secondary_key.to_string(), value.clone());
                }
                execute_nosql_dynamodb(
                    &source,
                    collection,
                    &NoSqlOperation::GetByPk,
                    Some(primary_key),
                    secondary_key,
                    &Value::Object(payload),
                )
                .await
                .map(|result| result.output)
            }
            DatabaseKind::Postgres | DatabaseKind::Mysql | DatabaseKind::Sqlite => Ok(Value::Null),
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

/// Literal value, or `env:NAME` to read from [`std::env::var`], for [`crate::config::TargetConfig::default_query`].
fn resolve_target_default_query_value(raw: &str) -> Result<String> {
    const PREFIX: &str = "env:";
    if let Some(name) = raw.strip_prefix(PREFIX) {
        let name = name.trim();
        return std::env::var(name).with_context(|| {
            format!("environment variable '{name}' (from [target].default_query) is not set")
        });
    }
    Ok(raw.to_string())
}

fn optional_empty_query_value(parameters: &[Field], name: &str, value: &Value) -> bool {
    parameters
        .iter()
        .find(|field| {
            field.name == name && matches!(field.location, Some(ParameterLocation::Query))
        })
        .is_some_and(|field| optional_empty_field_value(field, value))
}

fn optional_empty_field_value(field: &Field, value: &Value) -> bool {
    !field.required && value.as_str().is_some_and(|s| s.trim().is_empty())
}

fn drop_optional_empty_fields(body_map: &mut Map<String, Value>, parameters: &[Field]) {
    for field in parameters {
        if matches!(
            field.location,
            Some(ParameterLocation::Query | ParameterLocation::Header)
        ) {
            continue;
        }
        if body_map
            .get(&field.name)
            .is_some_and(|value| optional_empty_field_value(field, value))
        {
            body_map.remove(&field.name);
        }
    }
}

pub(crate) fn build_headers(
    auth: &AuthStrategy,
    config: &AppConfig,
    inline_auth_header: Option<&Value>,
) -> Result<HeaderMap> {
    build_headers_with_target_oauth(auth, config, inline_auth_header, |provider| {
        crate::auth::oauth::load_access_token(provider)
    })
}

fn build_headers_with_target_oauth(
    auth: &AuthStrategy,
    config: &AppConfig,
    inline_auth_header: Option<&Value>,
    load_target_token: impl Fn(&str) -> Option<String>,
) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    if let Some(provider) = config
        .target
        .oauth_provider
        .as_deref()
        .map(str::trim)
        .filter(|provider| !provider.is_empty())
    {
        if let Some(token) = load_target_token(provider) {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}"))
                    .context("invalid OAuth bearer token")?,
            );
            return Ok(headers);
        }
    }

    if let Some(auth_header) = &config.target.auth_header {
        insert_runtime_auth_header(&mut headers, auth_header)?;
        return Ok(headers);
    }

    if let Some(header_value) = inline_auth_header.and_then(Value::as_str) {
        insert_runtime_auth_header(&mut headers, header_value)?;
        return Ok(headers);
    }

    match auth {
        AuthStrategy::None => {}
        AuthStrategy::ApiKey {
            header,
            env_ref,
            location,
        } => {
            let key = secret_or_env(env_ref)?;
            match location {
                ApiKeyLocation::Header => {
                    headers.insert(
                        HeaderName::from_bytes(header.as_bytes())?,
                        HeaderValue::from_str(&key)?,
                    );
                }
                ApiKeyLocation::Cookie => {
                    headers.insert(
                        HeaderName::from_static("cookie"),
                        HeaderValue::from_str(&format!("{header}={key}"))?,
                    );
                }
                ApiKeyLocation::Query => {}
            }
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

fn append_query_auth(auth: &AuthStrategy, query: &mut Vec<(String, String)>) -> Result<()> {
    if let AuthStrategy::ApiKey {
        header,
        env_ref,
        location: ApiKeyLocation::Query,
    } = auth
    {
        query.push((header.clone(), secret_or_env(env_ref)?));
    }
    Ok(())
}

fn insert_runtime_auth_header(headers: &mut HeaderMap, raw: &str) -> Result<()> {
    if let Some((name, value)) = raw.split_once(':') {
        let name = name.trim();
        if !name.is_empty() {
            headers.insert(
                HeaderName::from_bytes(name.as_bytes())?,
                HeaderValue::from_str(&expand_runtime_header_value(value.trim())?)?,
            );
            return Ok(());
        }
    }
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&expand_runtime_header_value(raw.trim())?)?,
    );
    Ok(())
}

fn expand_runtime_header_value(raw: &str) -> Result<String> {
    if let Some(name) = raw.strip_prefix("env:") {
        let name = name.trim();
        return std::env::var(name).with_context(|| {
            format!("environment variable '{name}' (from auth header) is not set")
        });
    }
    if let Some(name) = raw.strip_prefix("keychain:") {
        let name = name.trim();
        return load_secret(name)
            .with_context(|| format!("keychain secret '{name}' (from auth header) is not set"));
    }
    if let Some(rest) = raw.strip_prefix("Bearer ")
        && let Some(name) = rest.trim().strip_prefix("env:")
    {
        let name = name.trim();
        let token = std::env::var(name).with_context(|| {
            format!("environment variable '{name}' (from Bearer auth header) is not set")
        })?;
        return Ok(format!("Bearer {token}"));
    }
    if let Some(rest) = raw.strip_prefix("Bearer ")
        && let Some(name) = rest.trim().strip_prefix("keychain:")
    {
        let name = name.trim();
        let token = load_secret(name).with_context(|| {
            format!("keychain secret '{name}' (from Bearer auth header) is not set")
        })?;
        return Ok(format!("Bearer {token}"));
    }
    Ok(raw.to_string())
}

fn secret_or_env(name: &str) -> Result<String> {
    if let Ok(value) = std::env::var(name) {
        if !value.trim().is_empty() {
            return Ok(value);
        }
    }
    load_secret(name).with_context(|| {
        format!(
            "missing credentials for OpenAPI security ref '{name}' (e.g. HTTP Basic). \
Set environment variable {name}, store the secret in the keychain (appctl service), \
or bypass per-scheme auth with [target].auth_header in .appctl/config.toml (e.g. Authorization: Bearer env:TOKEN)"
        )
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

const SQL_LIST_DEFAULT_LIMIT: u32 = 100;
const SQL_LIST_MAX_LIMIT: u32 = 500;
const SQL_LIST_MAX_OFFSET: u32 = 1_000_000;

#[derive(Debug, Clone)]
struct SqlListArgs {
    /// `AND` equality filters. `Value::Null` means `IS NULL` for that column.
    filters: Vec<(String, Value)>,
    limit: u32,
    offset: u32,
}

fn parse_sql_list_arguments(arguments: &Value) -> Result<SqlListArgs> {
    let Some(obj) = arguments.as_object() else {
        return Ok(SqlListArgs {
            filters: Vec::new(),
            limit: SQL_LIST_DEFAULT_LIMIT,
            offset: 0,
        });
    };

    let mut filters: Vec<(String, Value)> = Vec::new();
    if let Some(f) = obj.get("filter") {
        if !f.is_null() {
            let m = f
                .as_object()
                .context("list: filter must be a JSON object mapping column names to values")?;
            for (k, v) in m {
                if k.is_empty() {
                    bail!("list: empty column name in filter");
                }
                filters.push((k.clone(), v.clone()));
            }
        }
    }
    filters.sort_by(|a, b| a.0.cmp(&b.0));

    let limit = obj
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32)
        .unwrap_or(SQL_LIST_DEFAULT_LIMIT)
        .clamp(1, SQL_LIST_MAX_LIMIT);

    let offset = obj
        .get("offset")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32)
        .unwrap_or(0)
        .min(SQL_LIST_MAX_OFFSET);

    Ok(SqlListArgs {
        filters,
        limit,
        offset,
    })
}

fn classify_http_status(status: u16) -> &'static str {
    match status {
        401 => "unauthorized",
        403 => "forbidden",
        404 => "not_found",
        405 => "method_not_allowed",
        409 => "conflict",
        422 => "validation_error",
        500..=599 => "server_error",
        400..=499 => "client_error",
        _ => "ok",
    }
}

fn runtime_base_url(config: &AppConfig) -> Option<String> {
    config
        .target
        .base_url_env
        .as_deref()
        .and_then(|name| std::env::var(name).ok())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| std::env::var("APPCTL_BASE_URL").ok())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| config.target.base_url.clone())
}

fn runtime_cookie_store(paths: &ConfigPaths, config: &AppConfig) -> Option<Arc<CookieStoreMutex>> {
    let session_file = session_file_path(paths, config)?;
    let store = std::fs::File::open(&session_file)
        .map(std::io::BufReader::new)
        .ok()
        .and_then(|reader| cookie_store::serde::json::load(reader).ok())
        .unwrap_or_default();
    Some(Arc::new(CookieStoreMutex::new(store)))
}

fn session_file_path(paths: &ConfigPaths, _config: &AppConfig) -> Option<std::path::PathBuf> {
    let schema = crate::sync::load_schema(paths).ok()?;
    match schema.auth {
        AuthStrategy::Cookie {
            env_ref: _,
            session_file,
        } => session_file.map(std::path::PathBuf::from),
        _ => None,
    }
}

/// SQL double-quoted identifier (SQL-92; safe for **reserved words** like `order`, `user` in SQLite & PostgreSQL).
fn sql_ident_ansi(s: &str) -> String {
    format!("\"{}\"", s.replace('\"', "\"\""))
}

fn sql_qualified_table_ansi(kind: &DatabaseKind, schema: Option<&str>, table: &str) -> String {
    match kind {
        DatabaseKind::Postgres | DatabaseKind::Mysql => {
            if let Some(s) = schema {
                if s.is_empty() {
                    sql_ident_ansi(table)
                } else {
                    format!("{}.{}", sql_ident_ansi(s), sql_ident_ansi(table))
                }
            } else {
                sql_ident_ansi(table)
            }
        }
        _ => sql_ident_ansi(table),
    }
}

/// MySQL / MariaDB style `` `identifier` `` (reserved words, etc.).
fn sql_ident_mysql_quoted(s: &str) -> String {
    format!("`{}`", s.replace('`', "``"))
}

fn sql_qualified_table_mysql(schema: Option<&str>, table: &str) -> String {
    if let Some(s) = schema {
        if s.is_empty() {
            sql_ident_mysql_quoted(table)
        } else {
            format!(
                "{}.{}",
                sql_ident_mysql_quoted(s),
                sql_ident_mysql_quoted(table)
            )
        }
    } else {
        sql_ident_mysql_quoted(table)
    }
}

fn summarize_http_status(status: u16, method: &HttpMethod, path: &str) -> String {
    let method = reqwest_method(method).as_str().to_string();
    match status {
        401 => format!(
            "HTTP 401 Unauthorized for {method} {path}. The app rejected the request because credentials or session state are missing or invalid."
        ),
        403 => format!(
            "HTTP 403 Forbidden for {method} {path}. The app understood the request but refused it for the current user or token."
        ),
        404 => format!(
            "HTTP 404 Not Found for {method} {path}. The route or resource could not be found."
        ),
        405 => format!(
            "HTTP 405 Method Not Allowed for {method} {path}. The server rejected this HTTP method for the route. This can mean a route mismatch or backend policy; it does not prove missing admin access."
        ),
        409 => format!(
            "HTTP 409 Conflict for {method} {path}. The request conflicts with the current server state."
        ),
        422 => format!(
            "HTTP 422 Unprocessable Entity for {method} {path}. The route was reached, but the app rejected the input payload."
        ),
        500..=599 => format!(
            "HTTP {status} server error for {method} {path}. The request reached the app, but the backend failed while handling it."
        ),
        400..=499 => format!(
            "HTTP {status} client error for {method} {path}. The app rejected the request, but the exact cause is app-specific."
        ),
        _ => format!("HTTP {status} for {method} {path}."),
    }
}

async fn execute_sql_postgres(
    pool: &sqlx::Pool<sqlx::Postgres>,
    schema: Option<&str>,
    table: &str,
    operation: &SqlOperation,
    primary_key: Option<&str>,
    arguments: &Value,
) -> Result<ExecutionResult> {
    let primary_key = primary_key.unwrap_or("id");
    let qt = sql_qualified_table_ansi(&DatabaseKind::Postgres, schema, table);
    let pkq = sql_ident_ansi(primary_key);
    match operation {
        SqlOperation::Select => {
            let SqlListArgs {
                filters,
                limit,
                offset,
            } = parse_sql_list_arguments(arguments)?;
            let mut clauses = Vec::new();
            let mut bind_idx = 1u32;
            let mut bind_values: Vec<String> = Vec::new();
            for (col, val) in &filters {
                let cq = sql_ident_ansi(col);
                if val.is_null() {
                    clauses.push(format!("{cq} is null"));
                } else {
                    clauses.push(format!("{cq} = ${bind_idx}"));
                    bind_idx += 1;
                    bind_values.push(value_to_string(val));
                }
            }
            let where_part = if clauses.is_empty() {
                String::new()
            } else {
                format!(" where {}", clauses.join(" and "))
            };
            let lim_ph = bind_idx;
            let off_ph = bind_idx + 1;
            let inner = format!("select * from {qt}{where_part} limit ${lim_ph} offset ${off_ph}");
            let sql = format!("select coalesce(json_agg(t), '[]'::json) as rows from ({inner}) t");
            let mut q = sqlx::query_scalar::<_, Value>(&sql);
            for b in bind_values {
                q = q.bind(b);
            }
            q = q.bind(i64::from(limit)).bind(i64::from(offset));
            let rows: Value = q.fetch_one(pool).await?;
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
                "select row_to_json(t) as row from (select * from {qt} where {pkq} = $1 limit 1) t"
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
            let col_list = columns
                .iter()
                .map(|c| sql_ident_ansi(c))
                .collect::<Vec<_>>()
                .join(", ");
            let placeholders = (1..=columns.len())
                .map(|i| format!("${i}"))
                .collect::<Vec<_>>();
            let sql = format!(
                "insert into {qt} as appctl_r ({col_list}) values ({}) returning row_to_json(appctl_r)",
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
                .map(|(index, column)| {
                    format!("appctl_r.{} = ${}", sql_ident_ansi(column), index + 1)
                })
                .collect::<Vec<_>>();
            let sql = format!(
                "update {qt} as appctl_r set {} where appctl_r.{pkq} = ${} returning row_to_json(appctl_r)",
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
            let sql = format!("delete from {qt} as z where z.{pkq} = $1 returning row_to_json(z)");
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
    schema: Option<&str>,
    table: &str,
    operation: &SqlOperation,
    primary_key: Option<&str>,
    arguments: &Value,
) -> Result<ExecutionResult> {
    let primary_key = primary_key.unwrap_or("id");
    let qt = sql_qualified_table_mysql(schema, table);
    let pkq = sql_ident_mysql_quoted(primary_key);
    match operation {
        SqlOperation::Select => {
            let SqlListArgs {
                filters,
                limit,
                offset,
            } = parse_sql_list_arguments(arguments)?;
            let mut clauses = Vec::new();
            let mut bind_values: Vec<String> = Vec::new();
            for (col, val) in &filters {
                let cq = sql_ident_mysql_quoted(col);
                if val.is_null() {
                    clauses.push(format!("{cq} is null"));
                } else {
                    clauses.push(format!("{cq} = ?"));
                    bind_values.push(value_to_string(val));
                }
            }
            let where_part = if clauses.is_empty() {
                String::new()
            } else {
                format!(" where {}", clauses.join(" and "))
            };
            let sql = format!("select * from {qt}{where_part} limit ? offset ?");
            let mut q = sqlx::query(&sql);
            for b in bind_values {
                q = q.bind(b);
            }
            q = q.bind(i64::from(limit)).bind(i64::from(offset));
            let rows = q.fetch_all(pool).await?;
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
            let sql = format!("select * from {qt} where {pkq} = ? limit 1");
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
            let col_list = columns
                .iter()
                .map(|c| sql_ident_mysql_quoted(c))
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "insert into {qt} ({col_list}) values ({})",
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
                .map(|column| format!("{} = ?", sql_ident_mysql_quoted(column)))
                .collect::<Vec<_>>();
            let sql = format!("update {qt} set {} where {pkq} = ?", assignments.join(", "));
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
            let sql = format!("delete from {qt} where {pkq} = ?");
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

async fn execute_sql_sqlite(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    table: &str,
    operation: &SqlOperation,
    primary_key: Option<&str>,
    arguments: &Value,
) -> Result<ExecutionResult> {
    let primary_key = primary_key.unwrap_or("id");
    let qt = sql_ident_ansi(table);
    let pkq = sql_ident_ansi(primary_key);
    match operation {
        SqlOperation::Select => {
            let SqlListArgs {
                filters,
                limit,
                offset,
            } = parse_sql_list_arguments(arguments)?;
            let mut clauses = Vec::new();
            let mut bind_values: Vec<String> = Vec::new();
            for (col, val) in &filters {
                let cq = sql_ident_ansi(col);
                if val.is_null() {
                    clauses.push(format!("{cq} is null"));
                } else {
                    clauses.push(format!("{cq} = ?"));
                    bind_values.push(value_to_string(val));
                }
            }
            let where_part = if clauses.is_empty() {
                String::new()
            } else {
                format!(" where {}", clauses.join(" and "))
            };
            let sql = format!("select * from {qt}{where_part} limit ? offset ?");
            let mut q = sqlx::query(&sql);
            for b in bind_values {
                q = q.bind(b);
            }
            q = q.bind(i64::from(limit)).bind(i64::from(offset));
            let rows = q.fetch_all(pool).await?;
            Ok(ExecutionResult {
                output: rows_to_json_sqlite(rows),
                request_snapshot: Value::Null,
            })
        }
        SqlOperation::GetByPk => {
            let id = arguments
                .get(primary_key)
                .or_else(|| arguments.get("id"))
                .context("missing primary key argument")?;
            let sql = format!("select * from {qt} where {pkq} = ? limit 1");
            let rows = sqlx::query(&sql)
                .bind(value_to_string(id))
                .fetch_all(pool)
                .await?;
            Ok(ExecutionResult {
                output: rows_to_json_sqlite(rows)
                    .as_array()
                    .and_then(|rows| rows.first().cloned())
                    .unwrap_or(Value::Null),
                request_snapshot: Value::Null,
            })
        }
        SqlOperation::Insert => {
            let payload = arguments
                .as_object()
                .context("insert expects a JSON object")?;
            let columns: Vec<_> = payload.keys().cloned().collect();
            let placeholders = columns.iter().map(|_| "?").collect::<Vec<_>>();
            let col_list = columns
                .iter()
                .map(|c| sql_ident_ansi(c))
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "insert into {qt} ({col_list}) values ({})",
                placeholders.join(", ")
            );
            let mut query = sqlx::query(&sql);
            for value in payload.values() {
                query = query.bind(value_to_string(value));
            }
            let result = query.execute(pool).await?;
            Ok(ExecutionResult {
                output: json!({ "rows_affected": result.rows_affected(), "last_insert_rowid": result.last_insert_rowid() }),
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
                .map(|column| format!("{} = ?", sql_ident_ansi(column)))
                .collect::<Vec<_>>();
            let sql = format!("update {qt} set {} where {pkq} = ?", assignments.join(", "));
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
            let sql = format!("delete from {qt} where {pkq} = ?");
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

fn rows_to_json_sqlite(rows: Vec<sqlx::sqlite::SqliteRow>) -> Value {
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

async fn execute_nosql_mongodb(
    source: &str,
    collection: &str,
    operation: &NoSqlOperation,
    primary_key: Option<&str>,
    arguments: &Value,
) -> Result<ExecutionResult> {
    let client = mongodb::Client::with_uri_str(source).await?;
    let db = client
        .default_database()
        .context("mongodb connection string must include a default database name")?;
    let coll = db.collection::<Document>(collection);
    let primary_key = primary_key.unwrap_or("_id");

    let output = match operation {
        NoSqlOperation::List => {
            let docs = coll
                .find(doc! {})
                .limit(50)
                .await?
                .try_collect::<Vec<_>>()
                .await?;
            Value::Array(docs.into_iter().map(mongodb_document_to_json).collect())
        }
        NoSqlOperation::GetByPk => {
            let id = arguments
                .get(primary_key)
                .or_else(|| arguments.get("id"))
                .context("missing primary key argument")?;
            let filter = mongo_filter(primary_key, id);
            coll.find_one(filter)
                .await?
                .map(mongodb_document_to_json)
                .unwrap_or(Value::Null)
        }
        NoSqlOperation::Insert => {
            let mut doc = json_to_mongodb_document(arguments.get("document").unwrap_or(arguments))?;
            if let Some(id) = arguments.get(primary_key).or_else(|| arguments.get("id")) {
                insert_mongo_id(&mut doc, primary_key, id);
            }
            let result = coll.insert_one(doc.clone()).await?;
            let inserted_id = bson_to_json(result.inserted_id);
            json!({ "inserted_id": inserted_id, "document": mongodb_document_to_json(doc) })
        }
        NoSqlOperation::UpdateByPk => {
            let id = arguments
                .get(primary_key)
                .or_else(|| arguments.get("id"))
                .context("missing primary key argument")?;
            let mut doc = json_to_mongodb_document(arguments.get("document").unwrap_or(arguments))?;
            insert_mongo_id(&mut doc, primary_key, id);
            coll.replace_one(mongo_filter(primary_key, id), doc.clone())
                .await?;
            mongodb_document_to_json(doc)
        }
        NoSqlOperation::DeleteByPk => {
            let id = arguments
                .get(primary_key)
                .or_else(|| arguments.get("id"))
                .context("missing primary key argument")?;
            let result = coll.delete_one(mongo_filter(primary_key, id)).await?;
            json!({ "deleted": result.deleted_count > 0, primary_key: id.clone() })
        }
    };

    Ok(ExecutionResult {
        output,
        request_snapshot: Value::Null,
    })
}

async fn execute_nosql_redis(
    source: &str,
    operation: &NoSqlOperation,
    primary_key: Option<&str>,
    arguments: &Value,
) -> Result<ExecutionResult> {
    let client = redis::Client::open(source)?;
    let mut conn = client.get_multiplexed_async_connection().await?;
    let primary_key = primary_key.unwrap_or("key");

    let output = match operation {
        NoSqlOperation::List => {
            let mut iter = conn.scan_match::<_, String>("*").await?;
            let mut values = Vec::new();
            while values.len() < 50 {
                let Some(key) = iter.next_item().await.transpose()? else {
                    break;
                };
                values.push(json!({ primary_key: key }));
            }
            Value::Array(values)
        }
        NoSqlOperation::GetByPk => {
            let key = arguments
                .get(primary_key)
                .or_else(|| arguments.get("id"))
                .context("missing key argument")?;
            let key = value_to_string(key);
            let value: Option<String> = conn.get(&key).await?;
            value.map(|raw| parse_jsonish(&raw)).unwrap_or(Value::Null)
        }
        NoSqlOperation::Insert | NoSqlOperation::UpdateByPk => {
            let key = arguments
                .get(primary_key)
                .or_else(|| arguments.get("id"))
                .context("missing key argument")?;
            let key = value_to_string(key);
            let payload = arguments.get("document").unwrap_or(arguments);
            let rendered = if payload.is_string() {
                payload.as_str().unwrap_or_default().to_string()
            } else {
                serde_json::to_string(payload)?
            };
            let _: () = conn.set(&key, rendered).await?;
            json!({ "stored": true, primary_key: key })
        }
        NoSqlOperation::DeleteByPk => {
            let key = arguments
                .get(primary_key)
                .or_else(|| arguments.get("id"))
                .context("missing key argument")?;
            let key = value_to_string(key);
            let deleted: i64 = conn.del(&key).await?;
            json!({ "deleted": deleted > 0, primary_key: key })
        }
    };

    Ok(ExecutionResult {
        output,
        request_snapshot: Value::Null,
    })
}

async fn execute_nosql_firestore(
    client: &reqwest::Client,
    source: &str,
    operation: &NoSqlOperation,
    collection: &str,
    primary_key: Option<&str>,
    arguments: &Value,
) -> Result<ExecutionResult> {
    let project = firestore_project(source)?;
    let token = gcloud::adc_access_token(Some(&project))?;
    let base = format!(
        "https://firestore.googleapis.com/v1/projects/{project}/databases/(default)/documents/{collection}"
    );
    let primary_key = primary_key.unwrap_or("id");

    let output = match operation {
        NoSqlOperation::List => {
            let response = client
                .get(&base)
                .bearer_auth(&token.access_token)
                .query(&[("pageSize", "50")])
                .send()
                .await?
                .json::<Value>()
                .await?;
            let docs = response
                .get("documents")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Value::Array(docs.into_iter().map(firestore_document_to_json).collect())
        }
        NoSqlOperation::GetByPk => {
            let id = arguments
                .get(primary_key)
                .or_else(|| arguments.get("id"))
                .context("missing primary key argument")?;
            let response = client
                .get(format!("{base}/{}", value_to_string(id)))
                .bearer_auth(&token.access_token)
                .send()
                .await?;
            if response.status().is_success() {
                firestore_document_to_json(response.json::<Value>().await?)
            } else {
                Value::Null
            }
        }
        NoSqlOperation::Insert => {
            let payload =
                firestore_fields_from_json(arguments.get("document").unwrap_or(arguments));
            let mut request = client
                .post(&base)
                .bearer_auth(&token.access_token)
                .json(&json!({ "fields": payload }));
            if let Some(id) = arguments.get(primary_key).or_else(|| arguments.get("id")) {
                request = request.query(&[("documentId", value_to_string(id))]);
            }
            firestore_document_to_json(request.send().await?.json::<Value>().await?)
        }
        NoSqlOperation::UpdateByPk => {
            let id = arguments
                .get(primary_key)
                .or_else(|| arguments.get("id"))
                .context("missing primary key argument")?;
            let payload =
                firestore_fields_from_json(arguments.get("document").unwrap_or(arguments));
            firestore_document_to_json(
                client
                    .patch(format!("{base}/{}", value_to_string(id)))
                    .bearer_auth(&token.access_token)
                    .json(&json!({ "fields": payload }))
                    .send()
                    .await?
                    .json::<Value>()
                    .await?,
            )
        }
        NoSqlOperation::DeleteByPk => {
            let id = arguments
                .get(primary_key)
                .or_else(|| arguments.get("id"))
                .context("missing primary key argument")?;
            client
                .delete(format!("{base}/{}", value_to_string(id)))
                .bearer_auth(&token.access_token)
                .send()
                .await?;
            json!({ "deleted": true, primary_key: id.clone() })
        }
    };

    Ok(ExecutionResult {
        output,
        request_snapshot: Value::Null,
    })
}

async fn execute_nosql_dynamodb(
    source: &str,
    collection: &str,
    operation: &NoSqlOperation,
    primary_key: Option<&str>,
    secondary_key: Option<&str>,
    arguments: &Value,
) -> Result<ExecutionResult> {
    let (region, endpoint) = dynamodb_runtime_config(source)?;
    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new(region));
    if let Some(endpoint) = endpoint {
        loader = loader.endpoint_url(endpoint);
    }
    let config = loader.load().await;
    let client = aws_sdk_dynamodb::Client::new(&config);
    let primary_key = primary_key.unwrap_or("id");

    let output = match operation {
        NoSqlOperation::List => {
            let response = client
                .scan()
                .table_name(collection)
                .limit(50)
                .send()
                .await?;
            Value::Array(response.items().iter().map(dynamo_item_to_json).collect())
        }
        NoSqlOperation::GetByPk => {
            let key = dynamo_key(arguments, primary_key, secondary_key)?;
            let response = client
                .get_item()
                .table_name(collection)
                .set_key(Some(key))
                .send()
                .await?;
            response
                .item()
                .map(dynamo_item_to_json)
                .unwrap_or(Value::Null)
        }
        NoSqlOperation::Insert | NoSqlOperation::UpdateByPk => {
            let item = dynamo_item(arguments.get("document").unwrap_or(arguments));
            client
                .put_item()
                .table_name(collection)
                .set_item(Some(item.clone()))
                .send()
                .await?;
            dynamo_item_to_json(&item)
        }
        NoSqlOperation::DeleteByPk => {
            let key = dynamo_key(arguments, primary_key, secondary_key)?;
            client
                .delete_item()
                .table_name(collection)
                .set_key(Some(key))
                .send()
                .await?;
            json!({ "deleted": true, primary_key: arguments.get(primary_key).cloned().unwrap_or(Value::Null) })
        }
    };

    Ok(ExecutionResult {
        output,
        request_snapshot: Value::Null,
    })
}

fn runtime_database_source(config: &AppConfig) -> Option<String> {
    config
        .target
        .database_url
        .clone()
        .or_else(|| config.target.base_url.clone())
        .or_else(|| config.target.auth_header.clone())
}

fn mongo_filter(primary_key: &str, id: &Value) -> Document {
    let mut filter = Document::new();
    if primary_key == "_id"
        && let Some(raw) = id.as_str()
        && let Ok(object_id) = ObjectId::parse_str(raw)
    {
        filter.insert("_id", object_id);
        return filter;
    }
    filter.insert(
        primary_key,
        mongodb::bson::to_bson(&id).unwrap_or(mongodb::bson::Bson::Null),
    );
    filter
}

fn insert_mongo_id(document: &mut Document, primary_key: &str, id: &Value) {
    if primary_key == "_id"
        && let Some(raw) = id.as_str()
        && let Ok(object_id) = ObjectId::parse_str(raw)
    {
        document.insert("_id", object_id);
        return;
    }
    document.insert(
        primary_key,
        mongodb::bson::to_bson(&id).unwrap_or(mongodb::bson::Bson::Null),
    );
}

fn json_to_mongodb_document(value: &Value) -> Result<Document> {
    match mongodb::bson::to_bson(value)? {
        mongodb::bson::Bson::Document(document) => Ok(document),
        _ => bail!("document payload must be a JSON object"),
    }
}

fn mongodb_document_to_json(document: Document) -> Value {
    mongodb::bson::from_bson(mongodb::bson::Bson::Document(document)).unwrap_or(Value::Null)
}

fn bson_to_json(value: mongodb::bson::Bson) -> Value {
    mongodb::bson::from_bson(value).unwrap_or(Value::Null)
}

fn parse_jsonish(value: &str) -> Value {
    serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()))
}

fn firestore_project(source: &str) -> Result<String> {
    let parsed = url::Url::parse(source).context("invalid firestore connection string")?;
    parsed
        .host_str()
        .map(str::to_string)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            parsed
                .path_segments()
                .and_then(|mut segments| segments.next().map(str::to_string))
        })
        .filter(|value| !value.is_empty())
        .or_else(gcloud::detect_project)
        .context("firestore connection string must include a project id or gcloud project")
}

fn firestore_document_to_json(value: Value) -> Value {
    let mut object = Map::new();
    if let Some(name) = value.get("name").and_then(Value::as_str)
        && let Some(id) = name.rsplit('/').next()
    {
        object.insert("id".to_string(), Value::String(id.to_string()));
    }
    let fields = value
        .get("fields")
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));
    object.insert("document".to_string(), firestore_fields_to_json(&fields));
    Value::Object(object)
}

fn firestore_fields_to_json(value: &Value) -> Value {
    let Some(fields) = value.as_object() else {
        return Value::Null;
    };
    let mut object = Map::new();
    for (key, raw) in fields {
        let decoded = if let Some(v) = raw.get("stringValue") {
            v.clone()
        } else if let Some(v) = raw.get("integerValue") {
            v.as_str()
                .and_then(|v| v.parse::<i64>().ok())
                .map(Value::from)
                .unwrap_or(Value::Null)
        } else if let Some(v) = raw.get("doubleValue") {
            v.clone()
        } else if let Some(v) = raw.get("booleanValue") {
            v.clone()
        } else if let Some(v) = raw.get("nullValue") {
            let _ = v;
            Value::Null
        } else if let Some(v) = raw.get("mapValue").and_then(|v| v.get("fields")) {
            firestore_fields_to_json(v)
        } else if let Some(v) = raw.get("arrayValue").and_then(|v| v.get("values")) {
            Value::Array(
                v.as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|item| {
                        firestore_fields_to_json(&json!({ "value": item }))["value"].clone()
                    })
                    .collect(),
            )
        } else {
            Value::Null
        };
        object.insert(key.clone(), decoded);
    }
    Value::Object(object)
}

fn firestore_fields_from_json(value: &Value) -> Value {
    let Some(object) = value.as_object() else {
        return Value::Object(Map::new());
    };
    Value::Object(
        object
            .iter()
            .map(|(key, value)| (key.clone(), firestore_value_from_json(value)))
            .collect(),
    )
}

fn firestore_value_from_json(value: &Value) -> Value {
    match value {
        Value::Null => json!({ "nullValue": null }),
        Value::Bool(v) => json!({ "booleanValue": v }),
        Value::Number(v) if v.is_i64() || v.is_u64() => {
            json!({ "integerValue": v.to_string() })
        }
        Value::Number(v) => json!({ "doubleValue": v }),
        Value::String(v) => json!({ "stringValue": v }),
        Value::Array(values) => json!({
            "arrayValue": { "values": values.iter().map(firestore_value_from_json).collect::<Vec<_>>() }
        }),
        Value::Object(map) => json!({
            "mapValue": { "fields": map.iter().map(|(k, v)| (k.clone(), firestore_value_from_json(v))).collect::<Map<_, _>>() }
        }),
    }
}

fn dynamodb_runtime_config(source: &str) -> Result<(String, Option<String>)> {
    let parsed = url::Url::parse(source).context("invalid dynamodb connection string")?;
    let region = parsed
        .host_str()
        .map(str::to_string)
        .filter(|value| !value.is_empty())
        .context("dynamodb connection string must include a region, e.g. dynamodb://us-east-1")?;
    let endpoint = parsed
        .query_pairs()
        .find(|(key, _)| key == "endpoint")
        .map(|(_, value)| value.to_string());
    Ok((region, endpoint))
}

fn dynamo_key(
    arguments: &Value,
    primary_key: &str,
    secondary_key: Option<&str>,
) -> Result<std::collections::HashMap<String, AttributeValue>> {
    let mut key = std::collections::HashMap::new();
    let primary = arguments
        .get(primary_key)
        .or_else(|| arguments.get("id"))
        .context("missing primary key argument")?;
    key.insert(primary_key.to_string(), json_to_dynamo_attr(primary));
    if let Some(secondary_key) = secondary_key {
        let secondary = arguments
            .get(secondary_key)
            .context("missing secondary key argument")?;
        key.insert(secondary_key.to_string(), json_to_dynamo_attr(secondary));
    }
    Ok(key)
}

fn dynamo_item(value: &Value) -> std::collections::HashMap<String, AttributeValue> {
    value
        .as_object()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|(key, value)| (key, json_to_dynamo_attr(&value)))
        .collect()
}

fn json_to_dynamo_attr(value: &Value) -> AttributeValue {
    match value {
        Value::Null => AttributeValue::Null(true),
        Value::Bool(v) => AttributeValue::Bool(*v),
        Value::Number(v) => AttributeValue::N(v.to_string()),
        Value::String(v) => AttributeValue::S(v.clone()),
        Value::Array(values) => AttributeValue::L(values.iter().map(json_to_dynamo_attr).collect()),
        Value::Object(map) => AttributeValue::M(
            map.iter()
                .map(|(key, value)| (key.clone(), json_to_dynamo_attr(value)))
                .collect(),
        ),
    }
}

fn dynamo_item_to_json(item: &std::collections::HashMap<String, AttributeValue>) -> Value {
    Value::Object(
        item.iter()
            .map(|(key, value)| (key.clone(), dynamo_attr_to_json(value)))
            .collect(),
    )
}

fn dynamo_attr_to_json(value: &AttributeValue) -> Value {
    match value {
        AttributeValue::S(v) => Value::String(v.clone()),
        AttributeValue::N(v) => {
            serde_json::from_str(v).unwrap_or_else(|_| Value::String(v.clone()))
        }
        AttributeValue::Bool(v) => Value::Bool(*v),
        AttributeValue::Null(_) => Value::Null,
        AttributeValue::L(values) => Value::Array(values.iter().map(dynamo_attr_to_json).collect()),
        AttributeValue::M(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), dynamo_attr_to_json(value)))
                .collect(),
        ),
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DatabaseKind, Field, HttpMethod, ParameterLocation, build_headers_with_target_oauth,
        drop_optional_empty_fields, dynamo_attr_to_json, firestore_fields_from_json,
        firestore_fields_to_json, optional_empty_query_value, parse_sql_list_arguments,
        resolve_target_default_query_value, sql_ident_ansi, sql_qualified_table_ansi,
        summarize_http_status, tool_result_is_error,
    };
    use crate::config::AppConfig;
    use crate::schema::AuthStrategy;
    use crate::schema::FieldType;
    use aws_sdk_dynamodb::types::AttributeValue;
    use reqwest::header::AUTHORIZATION;
    use serde_json::json;

    #[test]
    fn sql_ident_ansi_quotes_sqlite_reserved_table_name() {
        assert_eq!(sql_ident_ansi("order"), "\"order\"");
        assert_eq!(sql_ident_ansi("user"), "\"user\"");
        assert_eq!(sql_ident_ansi("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn sql_qualified_ansi_rejects_single_token_for_schema_table() {
        assert_eq!(
            sql_qualified_table_ansi(&DatabaseKind::Postgres, Some("app"), "orders"),
            r#""app"."orders""#
        );
        assert_eq!(
            sql_qualified_table_ansi(&DatabaseKind::Postgres, None, "orders"),
            r#""orders""#
        );
    }

    #[test]
    fn target_default_query_resolves_literal() {
        assert_eq!(
            resolve_target_default_query_value("literal-token").unwrap(),
            "literal-token"
        );
    }

    #[test]
    fn target_default_query_resolves_env() {
        let k = "APPCTL_EXECUTOR_TEST_DQ_9C3E";
        // SAFETY: test-only unique key; other tests do not use this var.
        unsafe {
            std::env::set_var(k, "from-env");
        }
        let r = resolve_target_default_query_value(&format!("env:{k}")).unwrap();
        unsafe {
            std::env::remove_var(k);
        }
        assert_eq!(r, "from-env");
    }

    #[test]
    fn optional_empty_query_params_are_omitted() {
        let params = vec![Field {
            name: "status".to_string(),
            description: None,
            field_type: FieldType::String,
            required: false,
            location: Some(ParameterLocation::Query),
            default: None,
            enum_values: Vec::new(),
        }];
        assert!(optional_empty_query_value(&params, "status", &json!("")));
        assert!(optional_empty_query_value(&params, "status", &json!("   ")));
        assert!(!optional_empty_query_value(
            &params,
            "status",
            &json!("active")
        ));
    }

    #[test]
    fn required_empty_query_params_are_kept() {
        let params = vec![Field {
            name: "status".to_string(),
            description: None,
            field_type: FieldType::String,
            required: true,
            location: Some(ParameterLocation::Query),
            default: None,
            enum_values: Vec::new(),
        }];
        assert!(!optional_empty_query_value(&params, "status", &json!("")));
    }

    #[test]
    fn optional_empty_body_fields_are_omitted() {
        let params = vec![
            Field {
                name: "title".to_string(),
                description: None,
                field_type: FieldType::String,
                required: true,
                location: None,
                default: None,
                enum_values: Vec::new(),
            },
            Field {
                name: "assignee_id".to_string(),
                description: None,
                field_type: FieldType::String,
                required: false,
                location: None,
                default: None,
                enum_values: Vec::new(),
            },
        ];
        let mut body = serde_json::Map::from_iter([
            ("title".to_string(), json!("Sample Task")),
            ("assignee_id".to_string(), json!("")),
        ]);
        drop_optional_empty_fields(&mut body, &params);
        assert_eq!(body.get("title"), Some(&json!("Sample Task")));
        assert!(!body.contains_key("assignee_id"));
    }

    #[test]
    fn http_405_summary_stays_ambiguous() {
        let summary = summarize_http_status(405, &HttpMethod::DELETE, "/admin/product/10/delete");
        assert!(summary.contains("405 Method Not Allowed"));
        assert!(summary.contains("does not prove missing admin access"));
    }

    #[test]
    fn non_success_http_tool_results_are_errors() {
        assert!(tool_result_is_error(&json!({
            "ok": false,
            "status": 405,
            "summary": "HTTP 405 Method Not Allowed"
        })));
        assert!(!tool_result_is_error(&json!({
            "ok": true,
            "status": 200
        })));
    }

    #[test]
    fn target_oauth_profile_beats_stale_auth_header() {
        let mut config = AppConfig::default();
        config.target.oauth_provider = Some("esubalew".to_string());
        config.target.auth_header = Some("Authorization: Bearer old-token".to_string());

        let headers =
            build_headers_with_target_oauth(&AuthStrategy::None, &config, None, |provider| {
                (provider == "esubalew").then(|| "fresh-token".to_string())
            })
            .unwrap();

        assert_eq!(
            headers.get(AUTHORIZATION).unwrap().to_str().unwrap(),
            "Bearer fresh-token"
        );
    }

    #[test]
    fn missing_target_oauth_token_falls_back_to_auth_header() {
        let mut config = AppConfig::default();
        config.target.oauth_provider = Some("esubalew".to_string());
        config.target.auth_header = Some("Authorization: Bearer fallback-token".to_string());

        let headers =
            build_headers_with_target_oauth(&AuthStrategy::None, &config, None, |_| None).unwrap();

        assert_eq!(
            headers.get(AUTHORIZATION).unwrap().to_str().unwrap(),
            "Bearer fallback-token"
        );
    }

    #[test]
    fn firestore_field_conversion_round_trips_simple_json() {
        let value = json!({
            "name": "Ada",
            "count": 3,
            "enabled": true
        });
        let encoded = firestore_fields_from_json(&value);
        let decoded = firestore_fields_to_json(&encoded);
        assert_eq!(decoded["name"], "Ada");
        assert_eq!(decoded["count"], 3);
        assert_eq!(decoded["enabled"], true);
    }

    #[test]
    fn dynamo_attribute_conversion_handles_nested_maps() {
        let value = AttributeValue::M(
            [(
                "profile".to_string(),
                AttributeValue::M(
                    [("name".to_string(), AttributeValue::S("Ada".to_string()))]
                        .into_iter()
                        .collect(),
                ),
            )]
            .into_iter()
            .collect(),
        );
        assert_eq!(
            dynamo_attr_to_json(&value),
            json!({ "profile": { "name": "Ada" } })
        );
    }

    #[test]
    fn parse_sql_list_defaults() {
        let a = parse_sql_list_arguments(&json!({})).unwrap();
        assert_eq!(a.limit, 100);
        assert_eq!(a.offset, 0);
        assert!(a.filters.is_empty());
    }

    #[test]
    fn parse_sql_list_filter_and_pagination() {
        let a = parse_sql_list_arguments(&json!({
            "filter": { "uic": "X", "old_code": "Y" },
            "limit": 10,
            "offset": 20
        }))
        .unwrap();
        assert_eq!(a.limit, 10);
        assert_eq!(a.offset, 20);
        assert_eq!(a.filters.len(), 2);
    }

    #[test]
    fn parse_sql_list_rejects_non_object_filter() {
        let e = parse_sql_list_arguments(&json!({ "filter": "nope" })).unwrap_err();
        assert!(e.to_string().contains("filter"), "{e}");
    }
}
