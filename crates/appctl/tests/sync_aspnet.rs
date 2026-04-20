use appctl::schema::{Safety, SyncSource, Verb};
use appctl::sync::SyncPlugin;
use appctl::sync::aspnet::AspNetSync;

#[tokio::test]
async fn aspnet_controller_attribute_scan() {
    let fixture =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/aspnet_app");
    let schema = AspNetSync::new(fixture, Some("http://localhost:5000".to_string()))
        .introspect()
        .await
        .expect("aspnet introspection succeeds");

    assert_eq!(schema.source, SyncSource::Aspnet);

    let posts = schema
        .resources
        .iter()
        .find(|r| r.name == "Posts")
        .expect("Posts controller resource");
    assert!(posts.actions.iter().any(|a| matches!(a.verb, Verb::List)));
    assert!(posts.actions.iter().any(|a| matches!(a.verb, Verb::Get)));
    assert!(posts.actions.iter().any(|a| matches!(a.verb, Verb::Create)));
    assert!(
        posts
            .actions
            .iter()
            .any(|a| matches!(a.verb, Verb::Delete) && a.safety == Safety::Destructive)
    );
}
