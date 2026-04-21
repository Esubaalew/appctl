//! `--supabase <url>` sync: thin adapter over the OpenAPI doc served by
//! PostgREST.
//!
//! Hosted Supabase exposes the PostgREST OpenAPI document at `/rest/v1/`.
//! A bare PostgREST instance exposes it at `/`. We probe both so the same
//! flag works against either.

use anyhow::{Result, anyhow};

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

async fn looks_like_openapi(url: &str) -> bool {
    let Ok(resp) = reqwest::get(url).await else {
        return false;
    };
    if !resp.status().is_success() {
        return false;
    }
    let Ok(text) = resp.text().await else {
        return false;
    };
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    doc.get("paths").and_then(|v| v.as_object()).is_some()
        && (doc.get("swagger").is_some() || doc.get("openapi").is_some())
}

#[async_trait::async_trait]
impl SyncPlugin for SupabaseSync {
    async fn introspect(&self) -> Result<Schema> {
        let trimmed = self.base_url.trim_end_matches('/');
        let rest_v1 = format!("{trimmed}/rest/v1");
        let root = trimmed.to_string();

        // Probe Supabase layout first, fall back to bare PostgREST.
        let (openapi_url, effective_base) = if looks_like_openapi(&rest_v1).await {
            (rest_v1.clone(), rest_v1)
        } else if looks_like_openapi(&root).await {
            (root.clone(), root)
        } else {
            return Err(anyhow!(
                "no PostgREST OpenAPI document found at {rest_v1} or {root}. \
                 For hosted Supabase pass the project URL \
                 (https://<project>.supabase.co). For bare PostgREST pass the \
                 root URL that serves the swagger doc."
            ));
        };

        let mut schema = OpenApiSync::new(openapi_url).introspect().await?;
        schema.source = SyncSource::Supabase;
        schema.base_url = Some(effective_base);
        schema.auth = AuthStrategy::ApiKey {
            header: "apikey".to_string(),
            env_ref: self.anon_key_ref.clone(),
        };
        Ok(schema)
    }
}
