//! Run against a real Postgres when `APPCTL_TEST_POSTGRES_URL` is set.
use appctl::sync::{SyncPlugin, SyncRequest, db::DbSync, run_sync};
use tempfile::tempdir;

#[tokio::test]
#[ignore = "set APPCTL_TEST_POSTGRES_URL to run (see examples/demos/db-postgres)"]
async fn sync_postgres_demo_url_writes_schema() {
    let url =
        std::env::var("APPCTL_TEST_POSTGRES_URL").expect("APPCTL_TEST_POSTGRES_URL must be set");
    let schema = DbSync::new(url.clone())
        .introspect()
        .await
        .expect("db sync");
    assert!(!schema.resources.is_empty());

    let dir = tempdir().unwrap();
    let paths = appctl::config::ConfigPaths::new(dir.path().join(".appctl"));
    run_sync(
        paths,
        SyncRequest {
            db: Some(url),
            force: true,
            ..Default::default()
        },
    )
    .await
    .expect("run_sync");
}
