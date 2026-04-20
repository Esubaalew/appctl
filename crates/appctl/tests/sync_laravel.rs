use appctl::schema::{SyncSource, Verb};
use appctl::sync::SyncPlugin;
use appctl::sync::laravel::LaravelSync;

#[tokio::test]
async fn laravel_migrations_and_routes_produce_resources() {
    let fixture =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/laravel_app");
    let schema = LaravelSync::new(fixture, Some("http://localhost:8000".to_string()))
        .introspect()
        .await
        .expect("laravel introspection succeeds");

    assert_eq!(schema.source, SyncSource::Laravel);
    let post = schema
        .resources
        .iter()
        .find(|r| r.name == "post")
        .expect("posts resource from migration");
    assert!(post.fields.iter().any(|f| f.name == "title"));
    assert!(post.actions.iter().any(|a| matches!(a.verb, Verb::List)));
    assert!(post.actions.iter().any(|a| matches!(a.verb, Verb::Delete)));
}
