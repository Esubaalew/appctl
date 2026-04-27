use std::io::{self, BufRead, Write};

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    config::{AppConfig, ConfigPaths},
    executor::{ExecutionContext, ExecutionRequest, Executor, tool_result_is_error},
    safety::SafetyMode,
    sync::{load_runtime_tools, load_schema},
    tool_result_format::tool_result_capped_for_clients,
    tools::ToolDef,
};

#[derive(Debug, Clone)]
pub struct McpServeOptions {
    pub read_only: bool,
    pub dry_run: bool,
    pub strict: bool,
    pub confirm: bool,
}

#[derive(Debug, Deserialize)]
struct ToolCallParams {
    name: String,
    #[serde(default)]
    arguments: Value,
}

pub async fn run_mcp_server(paths: ConfigPaths, options: McpServeOptions) -> Result<()> {
    let schema = load_schema(&paths)?;
    let config = AppConfig::load_or_init(&paths)?;
    let tools = load_runtime_tools(&paths, &config)?;
    let executor = Executor::new(&paths)?;

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line.context("failed to read MCP input line")?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(_method) = request.get("method").and_then(Value::as_str) else {
            continue;
        };

        let Some(response) =
            handle_mcp_request(&schema, &config, &tools, &executor, &options, request).await
        else {
            continue;
        };

        writeln!(stdout, "{response}")?;
        stdout.flush()?;
    }

    Ok(())
}

async fn handle_mcp_request(
    schema: &crate::schema::Schema,
    config: &AppConfig,
    tools: &[ToolDef],
    executor: &Executor,
    options: &McpServeOptions,
    request: Value,
) -> Option<Value> {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str)?;

    Some(match method {
        "initialize" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "appctl-mcp",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            }
        }),
        "tools/list" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": tools.iter().map(|tool| json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema,
                })).collect::<Vec<_>>()
            }
        }),
        "tools/call" => {
            let params = request
                .get("params")
                .cloned()
                .unwrap_or(Value::Object(Default::default()));
            match serde_json::from_value::<ToolCallParams>(params) {
                Ok(params) => {
                    let resolved_name = config.resolve_tool_name(&params.name).to_string();
                    let safety = SafetyMode {
                        read_only: options.read_only,
                        dry_run: options.dry_run,
                        confirm: options.confirm,
                        strict: options.strict,
                    };
                    let Some(action) = schema.action(&resolved_name) else {
                        return Some(json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": { "code": -32602, "message": format!("unknown tool '{}'", params.name) }
                        }));
                    };
                    if let Err(e) = safety.check(action, &params.arguments) {
                        return Some(json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": { "code": -32000, "message": e.to_string() }
                        }));
                    }
                    let execution = executor
                        .execute(
                            schema,
                            ExecutionContext {
                                session_id: Uuid::new_v4().to_string(),
                                session_name: None,
                                safety,
                            },
                            ExecutionRequest::new(resolved_name, params.arguments),
                        )
                        .await;
                    match execution {
                        Ok(result) => {
                            let (text, structured) = match tool_result_capped_for_clients(
                                &result.output,
                                &config.behavior,
                            ) {
                                Ok(pair) => pair,
                                Err(_) => (
                                    serde_json::to_string_pretty(&result.output)
                                        .unwrap_or_else(|_| result.output.to_string()),
                                    result.output.clone(),
                                ),
                            };
                            json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": {
                                    "content": [{
                                        "type": "text",
                                        "text": text,
                                    }],
                                    "structuredContent": structured,
                                    "isError": tool_result_is_error(&result.output),
                                }
                            })
                        }
                        Err(err) => json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32000,
                                "message": err.to_string(),
                            }
                        }),
                    }
                }
                Err(err) => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32602,
                        "message": format!("invalid tools/call params: {err}"),
                    }
                }),
            }
        }
        _ if id.is_null() => return None,
        _ => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": format!("method not found: {method}"),
            }
        }),
    })
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::{
        config::{AppConfig, BehaviorConfig, TargetConfig},
        schema::{
            Action, AuthStrategy, Field, FieldType, HttpMethod, ParameterLocation, Provenance,
            Resource, Safety, Schema, SyncSource, Transport, Verb,
        },
        tools::schema_to_tools,
    };

    #[tokio::test]
    async fn handle_tools_list_and_call() {
        let dir = tempdir().unwrap();
        let paths = ConfigPaths::new(dir.path().join(".appctl"));
        let config = AppConfig {
            default: "ollama".to_string(),
            providers: AppConfig::default().providers,
            target: TargetConfig::default(),
            cloud: Default::default(),
            behavior: BehaviorConfig::default(),
            tooling: Default::default(),
            display_name: None,
            description: None,
        };
        config.save(&paths).unwrap();

        let schema = Schema {
            source: SyncSource::Openapi,
            base_url: Some("https://example.test".to_string()),
            auth: AuthStrategy::None,
            resources: vec![Resource {
                name: "widget".to_string(),
                description: Some("Widget".to_string()),
                fields: Vec::new(),
                actions: vec![Action {
                    name: "create_widget".to_string(),
                    description: Some("Create widget".to_string()),
                    verb: Verb::Create,
                    transport: Transport::Http {
                        method: HttpMethod::POST,
                        path: "/widgets".to_string(),
                        query: Vec::new(),
                    },
                    parameters: vec![Field {
                        name: "name".to_string(),
                        description: Some("Widget name".to_string()),
                        field_type: FieldType::String,
                        required: true,
                        location: Some(ParameterLocation::Body),
                        default: None,
                        enum_values: Vec::new(),
                    }],
                    safety: Safety::Mutating,
                    resource: Some("widget".to_string()),
                    provenance: Provenance::Declared,
                    metadata: Default::default(),
                }],
            }],
            metadata: Default::default(),
        };
        let tools = schema_to_tools(&schema);
        let executor = Executor::new(&paths).unwrap();
        let options = McpServeOptions {
            read_only: false,
            dry_run: true,
            strict: false,
            confirm: true,
        };

        let list = handle_mcp_request(
            &schema,
            &AppConfig::default(),
            &tools,
            &executor,
            &options,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
        )
        .await
        .unwrap();
        assert_eq!(list["result"]["tools"][0]["name"], "create_widget");

        let call = handle_mcp_request(
            &schema,
            &AppConfig::default(),
            &tools,
            &executor,
            &options,
            json!({
                "jsonrpc":"2.0",
                "id":2,
                "method":"tools/call",
                "params":{"name":"create_widget","arguments":{"name":"Demo"}}
            }),
        )
        .await
        .unwrap();
        assert_eq!(call["result"]["isError"], false);
        assert_eq!(call["result"]["structuredContent"]["dry_run"], true);
    }
}
