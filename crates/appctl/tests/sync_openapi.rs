use std::path::PathBuf;

use appctl::{
    schema::{AuthStrategy, Safety},
    sync::{SyncPlugin, openapi::OpenApiSync},
};

#[tokio::test]
async fn parses_openapi_fixture_into_resources_and_actions() {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/openapi/petstore.json");
    let schema = OpenApiSync::new(fixture.display().to_string(), None)
        .introspect()
        .await
        .expect("openapi sync succeeds");

    assert_eq!(schema.resources.len(), 1);
    assert_eq!(schema.resources[0].name, "pets");
    assert_eq!(schema.resources[0].actions.len(), 5);
    assert!(matches!(schema.auth, AuthStrategy::Bearer { .. }));

    let create = schema.action("create_pets").expect("create action exists");
    assert_eq!(create.parameters.len(), 3);
    assert_eq!(create.safety, Safety::Mutating);
}
