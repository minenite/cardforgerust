//! Plugin trait and description.

use crate::actions::{HostAction, OnlinePlayer};
use crate::command::Command;
use crate::event::{
    EventPriority, GameTickEvent, PlayerChatEvent, PlayerJoinEvent, PlayerQuitEvent,
    PlayerRespawnEvent,
};
use crate::perm::PermissionService;
use crate::server::Server;
use std::sync::Arc;
use std::path::PathBuf;
use thiserror::Error;

/// Metadata from a plugin's manifest (`cardforge-plugin.toml` later; today set in code).
#[derive(Clone, Debug)]
pub struct PluginDescription {
    /// Unique plugin id (`[a-z0-9_]+`), used in logs and load order.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Semver version string.
    pub version: String,
    /// Authors.
    pub authors: Vec<String>,
    /// One-line description.
    pub description: String,
    /// Other plugin ids that must load first.
    pub depend: Vec<String>,
    /// Soft dependencies (load first when present).
    pub soft_depend: Vec<String>,
}

impl PluginDescription {
    /// Start a description with the required fields.
    pub fn new(id: impl Into<String>, name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            authors: Vec::new(),
            description: String::new(),
            depend: Vec::new(),
            soft_depend: Vec::new(),
        }
    }
}

/// Per-plugin folder and logger name handed to [`Plugin::on_enable`].
#[derive(Clone, Debug)]
pub struct PluginContext {
    /// Directory for this plugin's config and data (`plugins/<id>/`).
    pub data_folder: PathBuf,
    /// Plugin id (same as [`PluginDescription::id`]).
    pub id: String,
}

/// Services the host exposes while a plugin loads (event registration, server handle).
pub trait PluginServices: Send + Sync {
    /// Live server snapshot handle.
    fn server(&self) -> &Server;

    /// Shared permission table (Bukkit nodes + Vault-shaped [`crate::PermissionProvider`]).
    fn permissions(&self) -> &PermissionService;

    /// Players currently online (updated by the host each tick / command).
    fn online_players(&self) -> Vec<OnlinePlayer>;

    /// Find an online player by exact (case-insensitive) name.
    fn player_by_name(&self, name: &str) -> Option<OnlinePlayer> {
        let want = name.to_ascii_lowercase();
        self.online_players()
            .into_iter()
            .find(|p| p.name.to_ascii_lowercase() == want)
    }

    /// Queue work for the Horizon host (teleport, menus, broadcast, …).
    fn enqueue(&self, action: HostAction);

    /// Cloneable sink for command closures registered in [`Plugin::on_load`].
    fn action_sink(&self) -> Arc<dyn Fn(HostAction) + Send + Sync>;

    /// Listen for player join.
    fn listen_player_join(
        &self,
        plugin: &str,
        priority: EventPriority,
        handler: Box<dyn Fn(&mut PlayerJoinEvent) + Send + Sync>,
    );

    /// Listen for player quit.
    fn listen_player_quit(
        &self,
        plugin: &str,
        priority: EventPriority,
        handler: Box<dyn Fn(&mut PlayerQuitEvent) + Send + Sync>,
    );

    /// Listen for chat (cancellable).
    fn listen_player_chat(
        &self,
        plugin: &str,
        priority: EventPriority,
        handler: Box<dyn Fn(&mut PlayerChatEvent) + Send + Sync>,
    );

    /// Listen for respawn.
    fn listen_player_respawn(
        &self,
        plugin: &str,
        priority: EventPriority,
        handler: Box<dyn Fn(&mut PlayerRespawnEvent) + Send + Sync>,
    );

    /// Listen for server ticks.
    fn listen_server_tick(
        &self,
        plugin: &str,
        priority: EventPriority,
        handler: Box<dyn Fn(&mut GameTickEvent) + Send + Sync>,
    );

    /// Register a slash command (tab-complete + dispatch).
    fn register_command(&self, command: Command);
}

/// Result alias for plugin enable/disable.
pub type PluginResult<T = ()> = Result<T, PluginError>;

/// Errors from enabling or talking to a plugin.
#[derive(Debug, Error)]
pub enum PluginError {
    /// The plugin refused to enable.
    #[error("plugin `{id}` failed to enable: {reason}")]
    EnableFailed {
        /// Plugin id.
        id: String,
        /// Why.
        reason: String,
    },
    /// Duplicate id in the plugin list.
    #[error("duplicate plugin id `{0}`")]
    DuplicateId(String),
    /// A hard dependency is missing.
    #[error("plugin `{id}` needs `{missing}`")]
    MissingDepend {
        /// Plugin that needs the dependency.
        id: String,
        /// Missing dependency id.
        missing: String,
    },
    /// I/O while preparing the data folder.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// A CardForge plugin. Implement this on a Rust type and register it with the host.
///
/// Lifecycle:
/// 1. Host constructs the plugin (or loads a future cdylib).
/// 2. [`Plugin::on_load`] — register event listeners / commands.
/// 3. [`Plugin::on_enable`] — worlds may already be open; start work.
/// 4. … server runs …
/// 5. [`Plugin::on_disable`] — flush state; do not use the server after return.
pub trait Plugin: Send + Sync {
    /// Static description for this plugin.
    fn description(&self) -> PluginDescription;

    /// Called once after construction, before enable. Register listeners here.
    fn on_load(&mut self, _services: &dyn PluginServices) -> PluginResult {
        Ok(())
    }

    /// Called when the plugin becomes active.
    fn on_enable(&mut self, _ctx: &PluginContext, _server: &Server) -> PluginResult {
        Ok(())
    }

    /// Called when the plugin is shutting down.
    fn on_disable(&mut self, _server: &Server) -> PluginResult {
        Ok(())
    }
}
