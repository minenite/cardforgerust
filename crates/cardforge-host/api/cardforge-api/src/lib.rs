//! CardForge API for Horizon 26.3.
//!
//! This is the plugin-facing surface: events, players, a [`Plugin`] trait, and
//! a [`Server`] handle. It is inspired by Bukkit/Paper (and by
//! [minenite/cardforge](https://github.com/minenite/cardforge)), but it is
//! **not** a Java plugin host. Existing `.jar` plugins do not load here.
//!
//! Plugins are Rust crates that implement [`Plugin`] and are registered with
//! the host at server start. Dynamic `.so` plugins use a **C ABI** (see
//! [`declare_plugin`]) — never pass `Box<dyn Plugin>` across the cdylib
//! boundary (Horizon's mimalloc vs the `.so` allocator will SIGSEGV).

#![deny(missing_docs)]

mod actions;
mod command;
mod event;
mod perm;
mod player;
mod plugin;
mod server;
mod text;

pub use actions::{HostAction, MenuClickAction, MenuItem, MenuSlot, OnlinePlayer};
pub use command::{Command, CommandSender};
pub use event::{
    AsyncEvent, Cancellable, Event, EventPriority, GameTickEvent, PlayerChatEvent, PlayerJoinEvent,
    PlayerQuitEvent, PlayerRespawnEvent,
};
pub use perm::{PermissibleId, PermissionProvider, PermissionService};
pub use player::Player;
pub use plugin::{
    Plugin, PluginContext, PluginDescription, PluginError, PluginResult, PluginServices,
};
pub use server::{Server, ServerSnapshot};
pub use text::colorize;

/// ABI version for dynamic plugins (`cardforge_plugin_abi`).
pub const DYNAMIC_PLUGIN_ABI: u32 = 1;

/// Symbol: `extern "C" fn() -> u32` — must return [`DYNAMIC_PLUGIN_ABI`].
pub const DYNAMIC_PLUGIN_ABI_SYM: &[u8] = b"cardforge_plugin_abi\0";

/// Symbol: `extern "C" fn() -> *mut ()` — non-null means the library initialized.
pub const DYNAMIC_PLUGIN_ENTRY: &[u8] = b"cardforge_create_plugin\0";

/// Symbol: `extern "C" fn() -> *const c_char` — NUL-terminated plugin id.
pub const DYNAMIC_PLUGIN_ID_SYM: &[u8] = b"cardforge_plugin_id\0";

/// Symbol: `extern "C" fn() -> *const c_char` — display name.
pub const DYNAMIC_PLUGIN_NAME_SYM: &[u8] = b"cardforge_plugin_name\0";

/// Symbol: `extern "C" fn() -> *const c_char` — semver string.
pub const DYNAMIC_PLUGIN_VERSION_SYM: &[u8] = b"cardforge_plugin_version\0";

/// Helper for cdylib plugins: export the CardForge C ABI for `$ty` (metadata only).
///
/// The host builds a shim [`Plugin`] on its own heap from these symbols. Do **not**
/// return a Rust trait object across the boundary.
#[macro_export]
macro_rules! declare_plugin {
    ($ty:ty) => {
        /// ABI version probe.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn cardforge_plugin_abi() -> u32 {
            $crate::DYNAMIC_PLUGIN_ABI
        }

        /// # Safety
        /// Called once by the CardForge host when loading this `.so`.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn cardforge_create_plugin() -> *mut () {
            // Touch Default so `$ty: Default` stays required; return a non-null sentinel.
            let _ = <$ty as Default>::default();
            1 as *mut ()
        }

        /// NUL-terminated plugin id (lives in a process-static `CString`).
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn cardforge_plugin_id() -> *const core::ffi::c_char {
            static INIT: std::sync::OnceLock<std::ffi::CString> = std::sync::OnceLock::new();
            INIT.get_or_init(|| {
                let id = <$ty as Default>::default().description().id;
                std::ffi::CString::new(id).expect("plugin id")
            })
            .as_ptr()
        }

        /// NUL-terminated display name.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn cardforge_plugin_name() -> *const core::ffi::c_char {
            static INIT: std::sync::OnceLock<std::ffi::CString> = std::sync::OnceLock::new();
            INIT.get_or_init(|| {
                let name = <$ty as Default>::default().description().name;
                std::ffi::CString::new(name).expect("plugin name")
            })
            .as_ptr()
        }

        /// NUL-terminated version.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn cardforge_plugin_version() -> *const core::ffi::c_char {
            static INIT: std::sync::OnceLock<std::ffi::CString> = std::sync::OnceLock::new();
            INIT.get_or_init(|| {
                let ver = <$ty as Default>::default().description().version;
                std::ffi::CString::new(ver).expect("plugin version")
            })
            .as_ptr()
        }
    };
}

/// Minecraft / Horizon game version this API targets.
pub const GAME_VERSION: &str = "26.3";

/// CardForge API crate version (semver of this crate).
pub const API_VERSION: &str = env!("CARGO_PKG_VERSION");
