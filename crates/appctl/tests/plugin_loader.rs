//! Build the `appctl-airtable` example plugin as a cdylib and load it through
//! `appctl::plugins::DynamicPlugin` to prove the C ABI works end to end.

use std::{path::PathBuf, process::Command};

use appctl::plugins::DynamicPlugin;
use appctl_plugin_sdk::SyncInput;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn target_lib_path(profile: &str) -> PathBuf {
    let base = workspace_root().join("target").join(profile);
    let stem = if cfg!(target_os = "windows") {
        "appctl_airtable.dll"
    } else if cfg!(target_os = "macos") {
        "libappctl_airtable.dylib"
    } else {
        "libappctl_airtable.so"
    };
    base.join(stem)
}

#[test]
fn airtable_example_plugin_roundtrips_through_c_abi() {
    let status = Command::new(env!("CARGO"))
        .args(["build", "-p", "appctl-airtable"])
        .current_dir(workspace_root())
        .status()
        .expect("cargo build of airtable plugin");
    assert!(status.success(), "cargo build failed");

    let lib = target_lib_path("debug");
    assert!(
        lib.exists(),
        "expected cdylib at {} but it was missing",
        lib.display()
    );

    let plugin = DynamicPlugin::load(&lib).expect("dynamic plugin loads");
    assert_eq!(plugin.name, "airtable");
    assert_eq!(plugin.version, "0.1.0");

    let schema = plugin
        .introspect(&SyncInput::default())
        .expect("plugin introspect returns a schema");
    assert!(
        schema
            .resources
            .iter()
            .any(|r| r.actions.iter().any(|a| a.name == "airtable_list"))
    );
}
