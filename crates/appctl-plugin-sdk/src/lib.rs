//! `appctl-plugin-sdk`
//!
//! Stable schema types and the dynamic-plugin ABI for the `appctl` CLI agent.
//!
//! Consumers fall into two groups:
//!
//! * **Built-in sync plugins** inside the main `appctl` crate — they implement
//!   the async [`SyncPlugin`] trait directly.
//! * **Out-of-process dynamic plugins** shipped as `cdylib`s living under
//!   `~/.appctl/plugins/` — they expose a single `#[no_mangle] extern "C"`
//!   entry point returning a [`PluginManifest`] plus a JSON-in / JSON-out
//!   `introspect` function. See [`declare_plugin!`].

pub mod ffi;
pub mod schema;

pub use ffi::{PluginManifest, PluginVtable, SDK_ABI_VERSION};
pub use schema::*;

use anyhow::Result;
use async_trait::async_trait;

/// Runtime inputs passed to a sync plugin.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SyncInput {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub options: serde_json::Map<String, serde_json::Value>,
}

/// A sync plugin introspects an application and returns a [`Schema`].
///
/// Built-in plugins in the main `appctl` crate implement this trait directly.
/// Dynamic plugins use the C ABI in [`ffi`] and are adapted by the loader.
#[async_trait]
pub trait SyncPlugin: Send + Sync {
    /// Stable identifier, e.g. `"openapi"`, `"rails"`, `"airtable"`.
    fn name(&self) -> &str;

    /// Introspect the target and build a schema.
    async fn introspect(&self) -> Result<Schema>;
}

/// Convenience re-export for plugin authors.
pub mod prelude {
    pub use super::ffi::{PluginManifest, PluginVtable, SDK_ABI_VERSION};
    pub use super::schema::*;
    pub use super::{SyncInput, SyncPlugin};
    pub use crate::declare_plugin;
    pub use anyhow::{Result, anyhow, bail};
    pub use async_trait::async_trait;
}
