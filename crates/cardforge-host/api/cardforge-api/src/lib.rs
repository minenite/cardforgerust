//! CardForge API for Horizon 26.3.
//!
//! This is the plugin-facing surface: events, players, a [`Plugin`] trait, and
//! a [`Server`] handle. It is inspired by Bukkit/Paper (and by
//! [minenite/cardforge](https://github.com/minenite/cardforge)), but it is
//! **not** a Java plugin host. Existing `.jar` plugins do not load here.
//!
//! Plugins are Rust crates that implement [`Plugin`] and are registered with
//! the host at server start.

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

/// ABI symbol dynamic plugins must export (`extern "C" fn() -> *mut ()`).
///
/// The pointer is `Box<Box<dyn Plugin>>` leaked via [`Box::into_raw`].
pub const DYNAMIC_PLUGIN_ENTRY: &[u8] = b"cardforge_create_plugin\0";

/// Helper for cdylib plugins: export the CardForge entry symbol for `$ty: Plugin + Default`.
#[macro_export]
macro_rules! declare_plugin {
    ($ty:ty) => {
        /// # Safety
        /// Called once by the CardForge host when loading this `.so`.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn cardforge_create_plugin() -> *mut () {
            let plugin: Box<dyn $crate::Plugin> = Box::new(<$ty as Default>::default());
            Box::into_raw(Box::new(plugin)) as *mut ()
        }
    };
}

/// Minecraft / Horizon game version this API targets.
pub const GAME_VERSION: &str = "26.3";

/// CardForge API crate version (semver of this crate).
pub const API_VERSION: &str = env!("CARGO_PKG_VERSION");
