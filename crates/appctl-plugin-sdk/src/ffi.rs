//! Stable C ABI for dynamic `appctl` sync plugins.
//!
//! The host (`appctl` binary) loads a `cdylib` built against this crate,
//! calls `appctl_plugin_register`, and receives a [`PluginManifest`] containing
//! a [`PluginVtable`] of extern "C" function pointers.
//!
//! JSON is used as the wire format between host and plugin so the ABI does
//! not depend on matching Rust toolchains or serde versions.

use serde::{Deserialize, Serialize};
use std::os::raw::{c_char, c_int};

/// Bumped whenever the vtable shape changes. Plugins must refuse to load
/// if the host reports a different value.
pub const SDK_ABI_VERSION: u32 = 1;

/// Static metadata returned by every plugin.
#[repr(C)]
pub struct PluginManifest {
    /// Must equal [`SDK_ABI_VERSION`]; checked by the host.
    pub abi_version: u32,
    /// Null-terminated UTF-8 name, e.g. `"airtable"`.
    pub name: *const c_char,
    /// Null-terminated UTF-8 semver.
    pub version: *const c_char,
    /// Null-terminated UTF-8 human description.
    pub description: *const c_char,
    /// Vtable with the plugin's operations.
    pub vtable: PluginVtable,
}

// SAFETY: all pointers are to 'static CStrs owned by the plugin image.
unsafe impl Send for PluginManifest {}
unsafe impl Sync for PluginManifest {}

/// Function pointer table. Every function takes and returns heap-allocated
/// null-terminated UTF-8 JSON strings the host must free via `free_string`.
#[repr(C)]
pub struct PluginVtable {
    /// Introspect the target.
    ///
    /// `input_json` is a UTF-8 JSON document matching [`crate::SyncInput`].
    /// On success `*out_json` points to a plugin-owned JSON string of a
    /// [`crate::schema::Schema`]; on failure `*out_json` is an error message.
    /// Returns 0 on success, non-zero on error.
    pub introspect:
        unsafe extern "C" fn(input_json: *const c_char, out_json: *mut *mut c_char) -> c_int,

    /// Free a string previously returned by this plugin.
    pub free_string: unsafe extern "C" fn(ptr: *mut c_char),
}

/// JSON envelope returned by the plugin's introspect function on success.
#[derive(Debug, Serialize, Deserialize)]
pub struct IntrospectResponse {
    pub schema: crate::schema::Schema,
}

/// Declare a plugin. Generates the `appctl_plugin_register` entrypoint plus
/// the `introspect` / `free_string` wrappers from a user-provided async
/// function of the form `async fn(SyncInput) -> anyhow::Result<Schema>`.
///
/// ```ignore
/// use appctl_plugin_sdk::prelude::*;
///
/// async fn my_introspect(_input: SyncInput) -> Result<Schema> { todo!() }
///
/// declare_plugin! {
///     name: "airtable",
///     version: "0.1.0",
///     description: "Sync an Airtable base",
///     introspect: my_introspect,
/// }
/// ```
#[macro_export]
macro_rules! declare_plugin {
    (
        name: $name:expr,
        version: $version:expr,
        description: $desc:expr,
        introspect: $introspect:path $(,)?
    ) => {
        const _APPCTL_PLUGIN_NAME: &[u8] = concat!($name, "\0").as_bytes();
        const _APPCTL_PLUGIN_VERSION: &[u8] = concat!($version, "\0").as_bytes();
        const _APPCTL_PLUGIN_DESC: &[u8] = concat!($desc, "\0").as_bytes();

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn appctl_plugin_register() -> *const $crate::ffi::PluginManifest {
            static MANIFEST: $crate::ffi::PluginManifest = $crate::ffi::PluginManifest {
                abi_version: $crate::ffi::SDK_ABI_VERSION,
                name: _APPCTL_PLUGIN_NAME.as_ptr() as *const ::std::os::raw::c_char,
                version: _APPCTL_PLUGIN_VERSION.as_ptr() as *const ::std::os::raw::c_char,
                description: _APPCTL_PLUGIN_DESC.as_ptr() as *const ::std::os::raw::c_char,
                vtable: $crate::ffi::PluginVtable {
                    introspect: _appctl_plugin_introspect_cabi,
                    free_string: _appctl_plugin_free_string_cabi,
                },
            };
            &MANIFEST as *const _
        }

        unsafe extern "C" fn _appctl_plugin_introspect_cabi(
            input_json: *const ::std::os::raw::c_char,
            out_json: *mut *mut ::std::os::raw::c_char,
        ) -> ::std::os::raw::c_int {
            let input_str = if input_json.is_null() {
                "{}".to_string()
            } else {
                unsafe { ::std::ffi::CStr::from_ptr(input_json) }
                    .to_string_lossy()
                    .into_owned()
            };

            let res: ::std::result::Result<$crate::schema::Schema, ::anyhow::Error> = (|| {
                let input: $crate::SyncInput = ::serde_json::from_str(&input_str)?;
                let rt = ::tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                rt.block_on($introspect(input))
            })();

            let (code, payload) = match res {
                Ok(schema) => {
                    let env = $crate::ffi::IntrospectResponse { schema };
                    match ::serde_json::to_string(&env) {
                        Ok(json) => (0, json),
                        Err(err) => (1, format!("{{\"error\":\"serialize failed: {}\"}}", err)),
                    }
                }
                Err(err) => (
                    1,
                    format!(
                        "{{\"error\":{}}}",
                        ::serde_json::to_string(&err.to_string())
                            .unwrap_or_else(|_| "\"unknown\"".into())
                    ),
                ),
            };

            match ::std::ffi::CString::new(payload) {
                Ok(cstr) => unsafe {
                    *out_json = cstr.into_raw();
                    code
                },
                Err(_) => unsafe {
                    *out_json = ::std::ptr::null_mut();
                    2
                },
            }
        }

        unsafe extern "C" fn _appctl_plugin_free_string_cabi(ptr: *mut ::std::os::raw::c_char) {
            if !ptr.is_null() {
                unsafe {
                    drop(::std::ffi::CString::from_raw(ptr));
                }
            }
        }
    };
}

/// Used by the host to recover a string returned from a plugin.
///
/// # Safety
///
/// `ptr` must have been returned by the corresponding plugin's
/// `introspect` function and freed using its `free_string` entry.
pub unsafe fn cstr_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        Some(
            unsafe { std::ffi::CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}
