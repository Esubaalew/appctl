use appctl::schema::{SyncSource, Verb};
use appctl::sync::SyncPlugin;
use appctl::sync::strapi::StrapiSync;

#[tokio::test]
async fn strapi_schema_json_produces_resources() {
    let fixture =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/strapi_app");
    let schema = StrapiSync::new(fixture, Some("http://localhost:1337".to_string()))
        .introspect()
        .await
        .expect("strapi introspection succeeds");

    assert_eq!(schema.source, SyncSource::Strapi);
    let article = schema
        .resources
        .iter()
        .find(|r| r.name == "article")
        .expect("article resource");
    assert!(
        article
            .fields
            .iter()
            .any(|f| f.name == "title" && f.required)
    );
    assert_eq!(article.actions.len(), 5);
    assert!(
        article
            .actions
            .iter()
            .any(|a| matches!(a.verb, Verb::Create))
    );
}
