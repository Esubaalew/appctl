use serde_json::Map;

use crate::schema::{
    Action, AuthStrategy, Field, FieldType, ParameterLocation, Provenance, Resource, Safety,
    Schema, SyncSource, Transport, Verb,
};

use super::SyncPlugin;

pub struct McpSync {
    server_url: String,
}

impl McpSync {
    pub fn new(server_url: String) -> Self {
        Self { server_url }
    }
}

#[async_trait::async_trait]
impl SyncPlugin for McpSync {
    async fn introspect(&self) -> anyhow::Result<Schema> {
        Ok(Schema {
            source: SyncSource::Mcp,
            base_url: Some(self.server_url.clone()),
            auth: AuthStrategy::Bearer {
                env_ref: "mcp_server_token".to_string(),
            },
            resources: vec![Resource {
                name: "mcp".to_string(),
                description: Some("Passthrough tool for a remote MCP server".to_string()),
                fields: Vec::new(),
                actions: vec![Action {
                    name: "call_remote_mcp_tool".to_string(),
                    description: Some("Call a remote MCP tool by name".to_string()),
                    verb: Verb::Custom,
                    transport: Transport::Mcp {
                        server_url: self.server_url.clone(),
                    },
                    parameters: vec![
                        Field {
                            name: "tool".to_string(),
                            description: Some("Remote MCP tool name".to_string()),
                            field_type: FieldType::String,
                            required: true,
                            location: Some(ParameterLocation::Body),
                            default: None,
                            enum_values: Vec::new(),
                        },
                        Field {
                            name: "arguments".to_string(),
                            description: Some("JSON arguments for the remote MCP tool".to_string()),
                            field_type: FieldType::Json,
                            required: false,
                            location: Some(ParameterLocation::Body),
                            default: None,
                            enum_values: Vec::new(),
                        },
                    ],
                    safety: Safety::Mutating,
                    resource: Some("mcp".to_string()),
                    provenance: Provenance::Declared,
                    metadata: Map::new(),
                }],
            }],
            metadata: Map::new(),
        })
    }
}
