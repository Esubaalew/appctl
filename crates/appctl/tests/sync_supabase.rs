use appctl::schema::{AuthStrategy, SyncSource};
use appctl::sync::SyncPlugin;
use appctl::sync::supabase::SupabaseSync;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn supabase_postgrest_openapi_adapter() {
    let server = MockServer::start().await;
    let openapi = json!({
        "openapi": "3.0.1",
        "info": { "title": "PostgREST", "version": "1.0" },
        "servers": [{ "url": format!("{}/rest/v1", server.uri()) }],
        "paths": {
            "/todos": {
                "get": {
                    "summary": "list todos",
                    "operationId": "todos_list",
                    "responses": { "200": { "description": "ok" } }
                }
            }
        },
        "components": {}
    });

    Mock::given(method("GET"))
        .and(path("/rest/v1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(openapi))
        .mount(&server)
        .await;

    let schema = SupabaseSync::new(server.uri(), "SUPABASE_ANON_KEY".to_string())
        .introspect()
        .await
        .expect("supabase introspection succeeds");

    assert_eq!(schema.source, SyncSource::Supabase);
    assert!(matches!(schema.auth, AuthStrategy::ApiKey { ref header, .. } if header == "apikey"));
    assert!(
        schema
            .resources
            .iter()
            .any(|r| r.actions.iter().any(|a| a.name.contains("todos")))
    );
}
