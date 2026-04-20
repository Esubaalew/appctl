use appctl::schema::{SyncSource, Verb};
use appctl::sync::SyncPlugin;
use appctl::sync::rails::RailsSync;

#[tokio::test]
async fn rails_schema_and_routes_produce_resources() {
    let fixture =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rails_app");
    let schema = RailsSync::new(fixture, Some("http://localhost:3000".to_string()))
        .introspect()
        .await
        .expect("rails introspection succeeds");

    assert_eq!(schema.source, SyncSource::Rails);

    let post = schema
        .resources
        .iter()
        .find(|r| r.name == "post")
        .expect("posts resource");
    assert!(post.fields.iter().any(|f| f.name == "title"));
    assert!(
        post.actions
            .iter()
            .any(|a| matches!(a.verb, Verb::List) && a.name.contains("list"))
    );
    assert!(post.actions.iter().any(|a| matches!(a.verb, Verb::Delete)));

    let comment = schema
        .resources
        .iter()
        .find(|r| r.name == "comment")
        .expect("comments resource");
    assert!(comment.actions.iter().any(|a| matches!(a.verb, Verb::List)));
}
