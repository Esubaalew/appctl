//! Re-exports of the stable schema types defined in `appctl-plugin-sdk`.
//!
//! Keeping the alias lets the rest of the binary continue to speak
//! `crate::schema::Schema` even though the canonical definitions live in the
//! SDK crate.
pub use appctl_plugin_sdk::schema::*;
