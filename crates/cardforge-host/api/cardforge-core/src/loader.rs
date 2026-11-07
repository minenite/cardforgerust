//! Dynamic `cdylib` plugin loader (C ABI — see `cardforge_api::declare_plugin`).
//!
//! Horizon uses mimalloc. A plugin `.so` uses the system allocator. Crossing the
//! boundary with `Box<dyn Plugin>` / Rust `String` drops SIGSEGVs in `mi_free`.
//! We only read C strings from the `.so` and build a host-heap [`DynamicShim`].

use cardforge_api::{
    colorize, Command, DYNAMIC_PLUGIN_ABI, DYNAMIC_PLUGIN_ABI_SYM, DYNAMIC_PLUGIN_ENTRY,
    DYNAMIC_PLUGIN_ID_SYM, DYNAMIC_PLUGIN_NAME_SYM, DYNAMIC_PLUGIN_VERSION_SYM, Plugin,
    PluginContext, PluginDescription, PluginResult, PluginServices, Server,
};
use libloading::Library;
use std::ffi::{CStr, c_char, c_void};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors while opening or instantiating a dynamic plugin.
#[derive(Debug, Error)]
pub enum DynamicLoadError {
    /// Shared library failed to open.
    #[error("open {}: {source}", path.display())]
    Open {
        /// Path that failed.
        path: PathBuf,
        /// Underlying loader error.
        #[source]
        source: libloading::Error,
    },
    /// Missing required symbol.
    #[error("missing {symbol} in {}", path.display())]
    MissingEntry {
        /// Library path.
        path: PathBuf,
        /// Symbol name.
        symbol: String,
        /// Symbol lookup error.
        #[source]
        source: libloading::Error,
    },
    /// Entry returned a null pointer.
    #[error("cardforge_create_plugin returned null in {}", path.display())]
    NullPlugin {
        /// Library path.
        path: PathBuf,
    },
    /// ABI mismatch.
    #[error("plugin ABI {got} != host {DYNAMIC_PLUGIN_ABI} in {}", path.display())]
    AbiMismatch {
        /// Library path.
        path: PathBuf,
        /// Version the `.so` reported.
        got: u32,
    },
    /// Plugin id was empty or invalid.
    #[error("invalid plugin id in {}", path.display())]
    BadId {
        /// Library path.
        path: PathBuf,
    },
}

type AbiFn = unsafe extern "C" fn() -> u32;
type CreateFn = unsafe extern "C" fn() -> *mut c_void;
type CStrFn = unsafe extern "C" fn() -> *const c_char;

/// Host-owned shim built from a dynamic library's C metadata.
pub struct DynamicShim {
    id: String,
    name: String,
    version: String,
    path: PathBuf,
}

impl Plugin for DynamicShim {
    fn description(&self) -> PluginDescription {
        let mut d = PluginDescription::new(&self.id, &self.name, &self.version);
        d.authors = vec!["dynamic".into()];
        d.description = format!("Loaded from {}", self.path.display());
        d
    }

    fn on_load(&mut self, services: &dyn PluginServices) -> PluginResult {
        let id = self.id.clone();
        let id_for_cmd = id.clone();
        let aliases = if id == "example_so" {
            vec!["exampleso".to_string(), "exso".to_string()]
        } else {
            Vec::<String>::new()
        };
        services.register_command(
            Command::new(
                id.clone(),
                id.clone(),
                format!("Ping dynamic plugin {id}"),
                id.clone(),
                move |_s, _l, _a| {
                    vec![colorize(&format!(
                        "&dDynamic plugin &f{id_for_cmd}&d is loaded."
                    ))]
                },
            )
            .aliases(aliases),
        );
        Ok(())
    }

    fn on_enable(&mut self, ctx: &PluginContext, _server: &Server) -> PluginResult {
        log::info!(
            "cardforge: dynamic shim `{}` enabled ({})",
            self.id,
            ctx.data_folder.display()
        );
        Ok(())
    }
}

/// Keep the `Library` alive for as long as symbols might be used.
pub struct DynamicPlugin {
    /// Host-heap plugin shim.
    pub shim: Box<dyn Plugin>,
    /// Must outlive any future symbol calls.
    _lib: Library,
    /// Source path for logs.
    pub path: PathBuf,
}

impl DynamicPlugin {
    /// Split into parts for [`CardForge::register_dynamic`].
    pub fn into_parts(self) -> (Box<dyn Plugin>, Library, PathBuf) {
        (self.shim, self._lib, self.path)
    }
}

unsafe fn read_cstr(f: CStrFn, fallback: &str) -> String {
    let ptr = unsafe { f() };
    if ptr.is_null() {
        return fallback.to_string();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

/// Load one `.so` / `.dylib` / `.dll` that exports the CardForge C ABI.
pub unsafe fn load_plugin(path: &Path) -> Result<DynamicPlugin, DynamicLoadError> {
    let lib = unsafe { Library::new(path) }.map_err(|source| DynamicLoadError::Open {
        path: path.to_path_buf(),
        source,
    })?;

    let abi_fn: libloading::Symbol<'_, AbiFn> = match unsafe { lib.get(DYNAMIC_PLUGIN_ABI_SYM) } {
        Ok(s) => s,
        Err(source) => {
            // Older example builds without abi symbol — treat as mismatch.
            return Err(DynamicLoadError::MissingEntry {
                path: path.to_path_buf(),
                symbol: "cardforge_plugin_abi".into(),
                source,
            });
        }
    };
    let abi = unsafe { abi_fn() };
    if abi != DYNAMIC_PLUGIN_ABI {
        return Err(DynamicLoadError::AbiMismatch {
            path: path.to_path_buf(),
            got: abi,
        });
    }

    let create: libloading::Symbol<'_, CreateFn> = unsafe { lib.get(DYNAMIC_PLUGIN_ENTRY) }
        .map_err(|source| DynamicLoadError::MissingEntry {
            path: path.to_path_buf(),
            symbol: "cardforge_create_plugin".into(),
            source,
        })?;
    let raw = unsafe { create() };
    if raw.is_null() {
        return Err(DynamicLoadError::NullPlugin {
            path: path.to_path_buf(),
        });
    }

    let id_fn: libloading::Symbol<'_, CStrFn> = unsafe { lib.get(DYNAMIC_PLUGIN_ID_SYM) }
        .map_err(|source| DynamicLoadError::MissingEntry {
            path: path.to_path_buf(),
            symbol: "cardforge_plugin_id".into(),
            source,
        })?;
    let name_fn: libloading::Symbol<'_, CStrFn> = unsafe { lib.get(DYNAMIC_PLUGIN_NAME_SYM) }
        .map_err(|source| DynamicLoadError::MissingEntry {
            path: path.to_path_buf(),
            symbol: "cardforge_plugin_name".into(),
            source,
        })?;
    let ver_fn: libloading::Symbol<'_, CStrFn> = unsafe { lib.get(DYNAMIC_PLUGIN_VERSION_SYM) }
        .map_err(|source| DynamicLoadError::MissingEntry {
            path: path.to_path_buf(),
            symbol: "cardforge_plugin_version".into(),
            source,
        })?;

    let id = unsafe { read_cstr(*id_fn, "") };
    if id.is_empty() || id.contains('\0') {
        return Err(DynamicLoadError::BadId {
            path: path.to_path_buf(),
        });
    }
    let name = unsafe { read_cstr(*name_fn, &id) };
    let version = unsafe { read_cstr(*ver_fn, "0.0.0") };

    let shim: Box<dyn Plugin> = Box::new(DynamicShim {
        id,
        name,
        version,
        path: path.to_path_buf(),
    });

    Ok(DynamicPlugin {
        shim,
        _lib: lib,
        path: path.to_path_buf(),
    })
}

/// Discover plugin libraries directly under `dir` (not recursive).
pub fn discover(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for ent in rd.flatten() {
        let path = ent.path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name.ends_with(".so") || name.ends_with(".dylib") || name.ends_with(".dll") {
            out.push(path);
        }
    }
    out.sort();
    out
}
