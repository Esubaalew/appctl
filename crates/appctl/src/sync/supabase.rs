//! `--supabase <url>` sync: thin adapter over the OpenAPI doc served by
//! PostgREST at `<url>/rest/v1/?apikey=<anon>`.

use anyhow::Result;

use crate::schema::{AuthStrategy, Schema, SyncSource};

use super::{SyncPlugin, openapi::OpenApiSync};

pub struct SupabaseSync {
    base_url: String,
    anon_key_ref: String,
}

impl SupabaseSync {
    pub fn new(base_url: String, anon_key_ref: String) -> Self {
        Self {
            base_url,
            anon_key_ref,
        }
    }
}

#[async_trait::async_trait]
impl SyncPlugin for SupabaseSync {
    async fn introspect(&self) -> Result<Schema> {
        let trimmed = self.base_url.trim_end_matches('/');
        let rest_base = format!("{trimmed}/rest/v1");
        // PostgREST exposes its OpenAPI at the bare REST root.
        let openapi_url = rest_base.clone();

        let mut schema = OpenApiSync::new(openapi_url).introspect().await?;
        schema.source = SyncSource::Supabase;
        schema.base_url = Some(rest_base);
        schema.auth = AuthStrategy::ApiKey {
            header: "apikey".to_string(),
            env_ref: self.anon_key_ref.clone(),
        };
        Ok(schema)
    }
}
