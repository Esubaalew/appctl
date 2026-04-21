//! Example `appctl` plugin that produces a tiny schema describing an Airtable
//! base. Ships as a `cdylib` meant to live in `~/.appctl/plugins/`.
//!
//! This example does not actually hit Airtable; it emits a static demo schema
//! so it can be loaded and verified in tests without network access.

use appctl_plugin_sdk::prelude::*;
use serde_json::Map;

async fn introspect(_input: SyncInput) -> Result<Schema> {
    let fields = vec![
        Field {
            name: "id".to_string(),
            description: Some("Airtable record id".to_string()),
            field_type: FieldType::String,
            required: true,
            location: Some(ParameterLocation::Path),
            default: None,
            enum_values: Vec::new(),
        },
        Field {
            name: "Name".to_string(),
            description: None,
            field_type: FieldType::String,
            required: false,
            location: Some(ParameterLocation::Body),
            default: None,
            enum_values: Vec::new(),
        },
    ];

    let actions = vec![
        Action {
            name: "airtable_list".to_string(),
            description: Some("List records".to_string()),
            verb: Verb::List,
            transport: Transport::Http {
                method: HttpMethod::GET,
                path: "/v0/{base_id}/{table}".to_string(),
                query: Vec::new(),
            },
            parameters: Vec::new(),
            safety: Safety::ReadOnly,
            resource: Some("record".to_string()),
            metadata: Map::new(),
        },
        Action {
            name: "airtable_get".to_string(),
            description: Some("Fetch a single record".to_string()),
            verb: Verb::Get,
            transport: Transport::Http {
                method: HttpMethod::GET,
                path: "/v0/{base_id}/{table}/{id}".to_string(),
                query: Vec::new(),
            },
            parameters: fields[..1].to_vec(),
            safety: Safety::ReadOnly,
            resource: Some("record".to_string()),
            metadata: Map::new(),
        },
    ];

    Ok(Schema {
        source: SyncSource::Plugin,
        base_url: Some("https://api.airtable.com".to_string()),
        auth: AuthStrategy::Bearer {
            env_ref: "AIRTABLE_API_TOKEN".to_string(),
        },
        resources: vec![Resource {
            name: "record".to_string(),
            description: Some("An Airtable record".to_string()),
            fields,
            actions,
        }],
        metadata: Map::new(),
    })
}

declare_plugin! {
    name: "airtable",
    version: "0.1.0",
    description: "Demo Airtable plugin for appctl",
    introspect: introspect,
}
