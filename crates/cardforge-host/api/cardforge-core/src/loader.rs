//! Dynamic `cdylib` plugin loader (`cardforge_create_plugin` entry).

use cardforge_api::{Plugin, DYNAMIC_PLUGIN_ENTRY};
use libloading::Library;
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
    /// Missing `cardforge_create_plugin` symbol.
    #[error("missing cardforge_create_plugin in {}", path.display())]
    MissingEntry {
        /// Library path.
        path: PathBuf,
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
}

/// Keep the `Library` alive for as long as the `Plugin` vtable is used.
pub struct DynamicPlugin {
    plugin: Box<dyn Plugin>,
    /// Must outlive `plugin`.
    _lib: Library,
    /// Source path for logs.
    pub path: PathBuf,
}

impl DynamicPlugin {
    /// The loaded plugin object.
    pub fn plugin_mut(&mut self) -> &mut dyn Plugin {
        self.plugin.as_mut()
    }

    /// Take ownership of the plugin while retaining the library (via forget pattern).
    /// Prefer [`CardForge::register_dynamic`] which stores both.
    pub fn into_parts(self) -> (Box<dyn Plugin>, Library, PathBuf) {
        (self.plugin, self._lib, self.path)
    }
}

type CreateFn = unsafe extern "C" fn() -> *mut ();

/// Load one `.so` / `.dylib` / `.dll` that exports `cardforge_create_plugin`.
pub unsafe fn load_plugin(path: &Path) -> Result<DynamicPlugin, DynamicLoadError> {
    let lib = unsafe { Library::new(path) }.map_err(|source| DynamicLoadError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    let create: libloading::Symbol<'_, CreateFn> = unsafe { lib.get(DYNAMIC_PLUGIN_ENTRY) }
        .map_err(|source| DynamicLoadError::MissingEntry {
            path: path.to_path_buf(),
            source,
        })?;
    let raw = unsafe { create() };
    if raw.is_null() {
        return Err(DynamicLoadError::NullPlugin {
            path: path.to_path_buf(),
        });
    }
    let plugin: Box<dyn Plugin> = *unsafe { Box::from_raw(raw as *mut Box<dyn Plugin>) };
    Ok(DynamicPlugin {
        plugin,
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
