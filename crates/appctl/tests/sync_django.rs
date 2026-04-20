use std::path::PathBuf;

use appctl::sync::{SyncPlugin, django::DjangoSync};

#[tokio::test]
async fn parses_django_models_and_urls_fixture() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/django_app");
    let schema = DjangoSync::new(fixture, Some("https://django.example.test".to_string()))
        .introspect()
        .await
        .expect("django sync succeeds");

    assert!(
        schema
            .resources
            .iter()
            .any(|resource| resource.name == "parcel")
    );
    assert!(
        schema
            .resources
            .iter()
            .any(|resource| resource.name == "customer")
    );

    let parcel = schema
        .resources
        .iter()
        .find(|resource| resource.name == "parcel")
        .expect("parcel resource exists");
    assert!(
        parcel
            .actions
            .iter()
            .any(|action| action.name == "create_parcel")
    );
    assert!(
        parcel
            .fields
            .iter()
            .any(|field| field.name == "tracking_number")
    );
}
