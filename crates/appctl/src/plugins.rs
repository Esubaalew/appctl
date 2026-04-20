//! Dynamic plugin loader for appctl.
//!
//! Scans `~/.appctl/plugins/` for cdylib files (`*.dylib`, `*.so`, `*.dll`)
//! built against `appctl-plugin-sdk`, loads them via `libloading`, verifies
//! the ABI version, and exposes them as [`DynamicPlugin`] instances.

use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow, bail};
use libloading::{Library, Symbol};

use appctl_plugin_sdk::ffi::{PluginManifest, SDK_ABI_VERSION};
use appctl_plugin_sdk::schema::Schema;

type RegisterFn = unsafe extern "C" fn() -> *const PluginManifest;

/// A loaded dynamic plugin.
pub struct DynamicPlugin {
    pub name: String,
    pub version: String,
    pub description: String,
    pub source_path: PathBuf,
    #[allow(dead_code)]
    library: Arc<Library>,
    manifest: *const PluginManifest,
}

// SAFETY: the underlying manifest/vtable pointers are static within the loaded
// library. Access is only performed after verifying the ABI version.
unsafe impl Send for DynamicPlugin {}
unsafe impl Sync for DynamicPlugin {}

impl DynamicPlugin {
    /// Load a single cdylib and extract its manifest.
    pub fn load(path: &Path) -> Result<Self> {
        let library = unsafe {
            Library::new(path)
                .with_context(|| format!("failed to load plugin at {}", path.display()))?
        };
        let library = Arc::new(library);
        let manifest_ptr: *const PluginManifest = unsafe {
            let register: Symbol<RegisterFn> =
                library.get(b"appctl_plugin_register").with_context(|| {
                    format!(
                        "plugin {} does not export appctl_plugin_register",
                        path.display()
                    )
                })?;
            register()
        };

        if manifest_ptr.is_null() {
            bail!("plugin {} returned null manifest", path.display());
        }
        let manifest: &PluginManifest = unsafe { &*manifest_ptr };
        if manifest.abi_version != SDK_ABI_VERSION {
            bail!(
                "plugin {} reports ABI version {} but host expects {}",
                path.display(),
                manifest.abi_version,
                SDK_ABI_VERSION
            );
        }

        let name = unsafe { cstr(manifest.name)? };
        let version = unsafe { cstr(manifest.version)? };
        let description = unsafe { cstr(manifest.description)? };

        Ok(Self {
            name,
            version,
            description,
            source_path: path.to_path_buf(),
            library,
            manifest: manifest_ptr,
        })
    }

    pub fn introspect(&self, input: &appctl_plugin_sdk::SyncInput) -> Result<Schema> {
        let input_json = serde_json::to_string(input)?;
        let input_c = CString::new(input_json)?;
        let mut out_ptr: *mut c_char = std::ptr::null_mut();
        let code = unsafe {
            let manifest: &PluginManifest = &*self.manifest;
            (manifest.vtable.introspect)(input_c.as_ptr(), &mut out_ptr as *mut *mut c_char)
        };
        if out_ptr.is_null() {
            bail!("plugin {} returned null response", self.name);
        }
        let output = unsafe { CStr::from_ptr(out_ptr) }
            .to_string_lossy()
            .into_owned();
        unsafe {
            let manifest: &PluginManifest = &*self.manifest;
            (manifest.vtable.free_string)(out_ptr);
        }
        if code != 0 {
            bail!("plugin {} errored: {}", self.name, output);
        }
        let envelope: appctl_plugin_sdk::ffi::IntrospectResponse =
            serde_json::from_str(&output).context("plugin returned invalid JSON")?;
        Ok(envelope.schema)
    }
}

impl Drop for DynamicPlugin {
    fn drop(&mut self) {
        // Keep the library alive until all plugin handles are dropped.
        // Arc<Library> handles this via reference counting; nothing to do.
    }
}

unsafe fn cstr(ptr: *const c_char) -> Result<String> {
    if ptr.is_null() {
        return Err(anyhow!("plugin returned null string"));
    }
    Ok(unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned())
}

/// Default plugin directory (`~/.appctl/plugins`).
pub fn plugin_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    Ok(home.join(".appctl").join("plugins"))
}

/// Load every plugin in the user's plugin directory.
pub fn discover() -> Result<Vec<DynamicPlugin>> {
    let dir = plugin_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut plugins = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        if !matches!(ext, "dylib" | "so" | "dll") {
            continue;
        }
        match DynamicPlugin::load(&path) {
            Ok(plugin) => plugins.push(plugin),
            Err(err) => tracing::warn!("skipping plugin {}: {err:#}", path.display()),
        }
    }
    Ok(plugins)
}
