//! Sync `web/dist` (repo root) into `embedded-web/dist` so `cargo publish` and local builds match.
use std::fs;
use std::io;
use std::path::Path;

fn main() -> io::Result<()> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_web = manifest_dir.join("../../web/dist");
    let embedded = manifest_dir.join("embedded-web/dist");

    if repo_web.is_dir() {
        sync_dir(&repo_web, &embedded)?;
        println!("cargo:rerun-if-changed=../../web/dist");
    } else if !embedded.is_dir() {
        fs::create_dir_all(&embedded)?;
        fs::write(
            embedded.join("index.html"),
            concat!(
                "<!doctype html><meta charset=\"utf-8\"/><title>appctl</title>",
                "<p>Web UI bundle missing. From the repo root run: <code>cd web && npm ci && npm run build</code></p>",
            ),
        )?;
    }

    Ok(())
}

fn sync_dir(src: &Path, dst: &Path) -> io::Result<()> {
    if dst.exists() {
        fs::remove_dir_all(dst)?;
    }
    copy_dir_all(src, dst)
}

fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
