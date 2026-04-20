use appctl::config::ConfigPaths;
use appctl::schema::{AuthStrategy, SyncSource};
use appctl::sync::SyncPlugin;
use appctl::sync::SyncRequest;
use appctl::sync::url::UrlSync;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn url_sync_logs_in_then_discovers_forms() {
    let server = MockServer::start().await;

    let login_body = r#"<!doctype html>
<html><head><meta name="csrf-token" content="abc123"/></head><body>
<form id="login" action="/login" method="post">
  <input type="hidden" name="csrfmiddlewaretoken" value="abc123"/>
  <input name="email" type="email"/>
  <input name="password" type="password"/>
  <button type="submit">Sign in</button>
</form>
</body></html>"#
        .to_string();
    Mock::given(method("GET"))
        .and(path("/login"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(login_body, "text/html"))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/login"))
        .respond_with(
            ResponseTemplate::new(302)
                .append_header("Set-Cookie", "session=authenticated; Path=/")
                .append_header("Location", "/dashboard"),
        )
        .mount(&server)
        .await;

    let dashboard_body = r#"<!doctype html>
<html><body>
<form action="/api/posts" method="post">
  <input name="title"/>
  <textarea name="body"></textarea>
</form>
</body></html>"#;
    Mock::given(method("GET"))
        .and(path("/dashboard"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(dashboard_body, "text/html"))
        .mount(&server)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let paths = ConfigPaths::new(dir.path().join(".appctl"));
    paths.ensure().unwrap();

    let request = SyncRequest {
        login_url: Some(format!("{}/login", server.uri())),
        login_user: Some("me@example.com".to_string()),
        login_password: Some("hunter2".to_string()),
        ..Default::default()
    };

    let schema = UrlSync::new(format!("{}/dashboard", server.uri()), &paths, &request)
        .unwrap()
        .introspect()
        .await
        .expect("url sync with login");

    assert_eq!(schema.source, SyncSource::Url);
    assert!(matches!(schema.auth, AuthStrategy::Cookie { .. }));
    assert!(!schema.resources.is_empty());
    assert!(paths.root.join("session.json").exists());
}
