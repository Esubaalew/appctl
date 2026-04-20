use appctl::sync::{SyncPlugin, db::DbSync};

#[tokio::test]
#[ignore = "set APPCTL_TEST_POSTGRES_URL to run against a live postgres instance"]
async fn introspects_postgres_schema_when_database_url_is_available() {
    let url =
        std::env::var("APPCTL_TEST_POSTGRES_URL").expect("APPCTL_TEST_POSTGRES_URL must be set");
    let schema = DbSync::new(url)
        .introspect()
        .await
        .expect("db sync succeeds");
    assert!(!schema.resources.is_empty());
}
