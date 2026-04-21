use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    auth::provider::ProviderAuthConfig,
    config::{ConfigPaths, read_json, write_json},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CloudProviderConnections {
    #[serde(default)]
    pub connections: Vec<SyncedProviderConnection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedProviderConnection {
    pub provider: String,
    pub auth: ProviderAuthConfig,
    #[serde(default)]
    pub synced_at: Option<i64>,
}

pub fn load_synced_connections(paths: &ConfigPaths) -> Result<CloudProviderConnections> {
    if !paths.provider_connections.exists() {
        return Ok(CloudProviderConnections::default());
    }
    read_json(&paths.provider_connections).with_context(|| {
        format!(
            "failed to read {}",
            paths.provider_connections.display()
        )
    })
}

pub fn save_synced_connections(
    paths: &ConfigPaths,
    connections: &CloudProviderConnections,
) -> Result<()> {
    write_json(&paths.provider_connections, connections)
}

pub fn load_synced_provider_connection(
    paths: &ConfigPaths,
    provider: &str,
) -> Result<Option<SyncedProviderConnection>> {
    let connections = load_synced_connections(paths)?;
    Ok(connections
        .connections
        .into_iter()
        .find(|connection| connection.provider == provider))
}
