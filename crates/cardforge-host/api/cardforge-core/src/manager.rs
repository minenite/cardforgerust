//! Plugin loader and CardForge façade the Horizon host drives.

use crate::bus::EventBus;
use crate::commands::{register_builtins, CommandMap, PluginInfo};
use crate::loader::{self, DynamicLoadError};
use cardforge_api::{
    Command, CommandSender, Event, EventPriority, GameTickEvent, HostAction, OnlinePlayer,
    PermissionService, PlayerChatEvent, PlayerJoinEvent, PlayerQuitEvent, PlayerRespawnEvent,
    Plugin, PluginContext, PluginError, PluginResult, PluginServices, Server, ServerSnapshot,
    API_VERSION, GAME_VERSION,
};
use libloading::Library;
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

/// Errors while loading the plugin set.
#[derive(Debug, Error)]
pub enum LoadError {
    /// Plugin-level failure.
    #[error(transparent)]
    Plugin(#[from] PluginError),
    /// Circular hard-dependency.
    #[error("plugin dependency cycle involving `{0}`")]
    Cycle(String),
    /// Dynamic library failure.
    #[error(transparent)]
    Dynamic(#[from] DynamicLoadError),
}

struct LoadedPlugin {
    id: String,
    plugin: Box<dyn Plugin>,
    enabled: bool,
}

/// The CardForge runtime: event bus + plugin list + server snapshot.
pub struct CardForge {
    server: Server,
    bus: EventBus,
    commands: CommandMap,
    permissions: PermissionService,
    plugins_dir: PathBuf,
    plugins: Mutex<Vec<LoadedPlugin>>,
    /// Keep cdylib handles alive while plugins run.
    dynamic_libs: Mutex<Vec<Library>>,
    /// Live roster for `/plugins` (shared with command handlers).
    roster: Arc<Mutex<Vec<PluginInfo>>>,
    /// Horizon host work queued by plugins.
    actions: Arc<Mutex<VecDeque<HostAction>>>,
    /// Online players (host refreshes).
    online: Mutex<Vec<OnlinePlayer>>,
}

impl CardForge {
    /// Create a runtime rooted at `plugins_dir` (created if missing).
    pub fn new(plugins_dir: impl Into<PathBuf>) -> std::io::Result<Self> {
        let plugins_dir = plugins_dir.into();
        std::fs::create_dir_all(&plugins_dir)?;
        let server = Server::new(ServerSnapshot {
            motd: "CardForge (Horizon)".to_string(),
            max_players: 20,
            players: Vec::new(),
            game_version: GAME_VERSION.to_string(),
        });
        let roster = Arc::new(Mutex::new(Vec::new()));
        let commands = CommandMap::new();
        let permissions = PermissionService::new();
        // Default: everyone can list plugins / version (Bukkit-like).
        permissions.set_default("cardforge.command.plugins", true);
        permissions.set_default("cardforge.command.version", true);
        permissions.set_default("cardforge.command.cardforge", true);
        let roster_for_cmds = Arc::clone(&roster);
        register_builtins(
            &commands,
            Arc::new(move || roster_for_cmds.lock().clone()),
            API_VERSION,
            GAME_VERSION,
        );
        Ok(Self {
            server,
            bus: EventBus::new(),
            commands,
            permissions,
            plugins_dir,
            plugins: Mutex::new(Vec::new()),
            dynamic_libs: Mutex::new(Vec::new()),
            roster,
            actions: Arc::new(Mutex::new(VecDeque::new())),
            online: Mutex::new(Vec::new()),
        })
    }

    /// Replace the online-player snapshot (host calls this each tick / command).
    pub fn set_online(&self, players: Vec<OnlinePlayer>) {
        *self.online.lock() = players;
    }

    /// Drain queued host actions (FIFO).
    pub fn take_actions(&self) -> Vec<HostAction> {
        self.actions.lock().drain(..).collect()
    }

    fn refresh_roster(&self) {
        let snap: Vec<PluginInfo> = self
            .plugins
            .lock()
            .iter()
            .map(|p| PluginInfo {
                description: p.plugin.description(),
                enabled: p.enabled,
            })
            .collect();
        *self.roster.lock() = snap;
    }

    /// Shared server handle for plugins.
    pub fn server(&self) -> &Server {
        &self.server
    }

    /// Event bus plugins register on during [`Plugin::on_load`].
    pub fn bus(&self) -> &EventBus {
        &self.bus
    }

    /// Command registry.
    pub fn commands(&self) -> &CommandMap {
        &self.commands
    }

    /// Bukkit-style permission nodes.
    pub fn permissions(&self) -> &PermissionService {
        &self.permissions
    }

    /// Snapshot of registered plugins for `/plugins`.
    pub fn plugin_infos(&self) -> Vec<PluginInfo> {
        self.roster.lock().clone()
    }

    /// Run a player/console command line (no leading slash). `Some(replies)` if handled.
    pub fn dispatch_command(&self, sender: &CommandSender, raw: &str) -> Option<Vec<String>> {
        self.commands
            .dispatch_with_perms(sender, raw, Some(&self.permissions))
    }

    /// Load every `*.so` / `*.dylib` / `*.dll` in the plugins directory and register them.
    pub fn load_dynamic_plugins(&self) -> Result<usize, LoadError> {
        let mut n = 0;
        for path in loader::discover(&self.plugins_dir) {
            match self.register_dynamic(&path) {
                Ok(()) => {
                    log::info!("cardforge: loaded dynamic plugin {}", path.display());
                    n += 1;
                }
                Err(err) => {
                    log::error!("cardforge: skip {}: {err}", path.display());
                }
            }
        }
        Ok(n)
    }

    /// Open one dynamic library and register its plugin (not enabled yet).
    pub fn register_dynamic(&self, path: &Path) -> Result<(), LoadError> {
        let loaded = unsafe { loader::load_plugin(path)? };
        let (plugin, lib, _) = loaded.into_parts();
        self.dynamic_libs.lock().push(lib);
        // Shim is host-allocated; safe to Drop normally.
        self.register(plugin)?;
        Ok(())
    }

    /// Usages to push into Horizon `register_commands`.
    pub fn command_usages(&self) -> Vec<(String, u8)> {
        self.commands.native_usages()
    }

    /// Directory that holds per-plugin data folders.
    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }

    /// Update MOTD / max players / online list from the Horizon host.
    pub fn set_snapshot(&self, snapshot: ServerSnapshot) {
        self.server.replace_snapshot(snapshot);
    }

    /// Register a plugin (not enabled yet). Call [`CardForge::enable_all`] after.
    pub fn register(&self, plugin: Box<dyn Plugin>) -> PluginResult {
        let desc = plugin.description();
        let mut list = self.plugins.lock();
        if list.iter().any(|p| p.id == desc.id) {
            return Err(PluginError::DuplicateId(desc.id));
        }
        list.push(LoadedPlugin {
            id: desc.id,
            plugin,
            enabled: false,
        });
        drop(list);
        self.refresh_roster();
        Ok(())
    }

    /// Load + enable every registered plugin in dependency order.
    pub fn enable_all(&self) -> Result<(), LoadError> {
        let order = {
            let list = self.plugins.lock();
            topological_order(&list)?
        };
        for id in order {
            self.enable_one(&id)?;
        }
        Ok(())
    }

    fn enable_one(&self, id: &str) -> Result<(), LoadError> {
        let mut list = self.plugins.lock();
        let Some(slot) = list.iter_mut().find(|p| p.id == id) else {
            return Ok(());
        };
        if slot.enabled {
            return Ok(());
        }
        let data_folder = self.plugins_dir.join(id);
        std::fs::create_dir_all(&data_folder).map_err(PluginError::from)?;
        let ctx = PluginContext {
            data_folder,
            id: id.to_string(),
        };
        slot.plugin.on_load(self)?;
        slot.plugin.on_enable(&ctx, &self.server).map_err(|e| {
            LoadError::Plugin(match e {
                PluginError::EnableFailed { .. } => e,
                other => PluginError::EnableFailed {
                    id: id.to_string(),
                    reason: other.to_string(),
                },
            })
        })?;
        slot.enabled = true;
        drop(list);
        self.refresh_roster();
        log::info!("cardforge: enabled plugin `{id}`");
        Ok(())
    }

    /// Disable every plugin (reverse enable order).
    pub fn disable_all(&self) {
        let mut list = self.plugins.lock();
        for slot in list.iter_mut().rev() {
            if !slot.enabled {
                continue;
            }
            if let Err(err) = slot.plugin.on_disable(&self.server) {
                log::error!("cardforge: disable `{}`: {err}", slot.id);
            } else {
                log::info!("cardforge: disabled plugin `{}`", slot.id);
            }
            slot.enabled = false;
        }
        drop(list);
        self.refresh_roster();
    }

    /// Dispatch an event through the bus.
    pub fn call<E: Event + 'static>(&self, event: &mut E) {
        self.bus.call(event);
    }
}

impl PluginServices for CardForge {
    fn server(&self) -> &Server {
        &self.server
    }

    fn permissions(&self) -> &PermissionService {
        &self.permissions
    }

    fn online_players(&self) -> Vec<OnlinePlayer> {
        self.online.lock().clone()
    }

    fn enqueue(&self, action: HostAction) {
        self.actions.lock().push_back(action);
    }

    fn action_sink(&self) -> Arc<dyn Fn(HostAction) + Send + Sync> {
        let actions = Arc::clone(&self.actions);
        Arc::new(move |action: HostAction| {
            actions.lock().push_back(action);
        })
    }

    fn listen_player_join(
        &self,
        plugin: &str,
        priority: EventPriority,
        handler: Box<dyn Fn(&mut PlayerJoinEvent) + Send + Sync>,
    ) {
        self.bus.listen::<PlayerJoinEvent, _>(plugin, priority, move |e| handler(e));
    }

    fn listen_player_quit(
        &self,
        plugin: &str,
        priority: EventPriority,
        handler: Box<dyn Fn(&mut PlayerQuitEvent) + Send + Sync>,
    ) {
        self.bus.listen::<PlayerQuitEvent, _>(plugin, priority, move |e| handler(e));
    }

    fn listen_player_chat(
        &self,
        plugin: &str,
        priority: EventPriority,
        handler: Box<dyn Fn(&mut PlayerChatEvent) + Send + Sync>,
    ) {
        self.bus.listen::<PlayerChatEvent, _>(plugin, priority, move |e| handler(e));
    }

    fn listen_player_respawn(
        &self,
        plugin: &str,
        priority: EventPriority,
        handler: Box<dyn Fn(&mut PlayerRespawnEvent) + Send + Sync>,
    ) {
        self.bus
            .listen::<PlayerRespawnEvent, _>(plugin, priority, move |e| handler(e));
    }

    fn listen_server_tick(
        &self,
        plugin: &str,
        priority: EventPriority,
        handler: Box<dyn Fn(&mut GameTickEvent) + Send + Sync>,
    ) {
        self.bus
            .listen::<GameTickEvent, _>(plugin, priority, move |e| handler(e));
    }

    fn register_command(&self, command: Command) {
        self.commands.register(command);
    }
}

fn topological_order(plugins: &[LoadedPlugin]) -> Result<Vec<String>, LoadError> {
    let ids: Vec<String> = plugins.iter().map(|p| p.id.clone()).collect();
    let present: HashMap<&str, ()> = ids.iter().map(|id| (id.as_str(), ())).collect();
    let mut deps: HashMap<String, Vec<String>> = HashMap::new();
    for p in plugins {
        let desc = p.plugin.description();
        for need in &desc.depend {
            if !present.contains_key(need.as_str()) {
                return Err(LoadError::Plugin(PluginError::MissingDepend {
                    id: desc.id.clone(),
                    missing: need.clone(),
                }));
            }
        }
        let mut requires = desc.depend.clone();
        for soft in desc.soft_depend {
            if present.contains_key(soft.as_str()) {
                requires.push(soft);
            }
        }
        deps.insert(desc.id, requires);
    }

    let mut pending: Vec<String> = ids;
    let mut done: Vec<String> = Vec::new();
    while !pending.is_empty() {
        let ready = pending.iter().position(|id| {
            deps.get(id)
                .map(|req| req.iter().all(|r| done.iter().any(|d| d == r)))
                .unwrap_or(true)
        });
        let Some(idx) = ready else {
            return Err(LoadError::Cycle(pending[0].clone()));
        };
        done.push(pending.remove(idx));
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cardforge_api::{EventPriority, Player, PlayerJoinEvent, PluginDescription};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use uuid::Uuid;

    struct Hello;

    impl Plugin for Hello {
        fn description(&self) -> PluginDescription {
            PluginDescription::new("hello", "Hello", "0.1.0")
        }
    }

    #[test]
    fn enable_and_fire_join() {
        let dir = std::env::temp_dir().join(format!("cardforge-test-{}", Uuid::new_v4()));
        let cf = CardForge::new(&dir).unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits2 = Arc::clone(&hits);
        cf.bus()
            .listen::<PlayerJoinEvent, _>("counter", EventPriority::Normal, move |_| {
                hits2.fetch_add(1, Ordering::SeqCst);
            });
        cf.register(Box::new(Hello)).unwrap();
        cf.enable_all().unwrap();
        let mut event = PlayerJoinEvent {
            player: Player::named(Uuid::nil(), "Ada"),
            join_message: Some("Ada joined".into()),
        };
        cf.call(&mut event);
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        cf.disable_all();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn builtins_plugins_and_version() {
        use cardforge_api::CommandSender;
        let dir = std::env::temp_dir().join(format!("cardforge-cmd-{}", Uuid::new_v4()));
        let cf = CardForge::new(&dir).unwrap();
        cf.register(Box::new(Hello)).unwrap();
        cf.enable_all().unwrap();
        let sender = CommandSender::Console;
        let plugins = cf.dispatch_command(&sender, "plugins").unwrap();
        assert!(
            plugins[0].contains("Hello"),
            "got {:?}",
            plugins
        );
        let ver = cf.dispatch_command(&sender, "version").unwrap();
        assert!(ver[0].contains("CardForge"), "got {:?}", ver);
        let cfhelp = cf.dispatch_command(&sender, "cardforge").unwrap();
        assert!(
            cfhelp[0].contains("CardForge") && cfhelp[0].contains("commands"),
            "got {:?}",
            cfhelp
        );
        let list = cf.dispatch_command(&sender, "plugins").unwrap();
        assert!(
            list[0].contains('§') && list[0].contains("Hello"),
            "expected colored plugin list, got {:?}",
            list
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn op_player_passes_leveled_command_and_native_usages_are_public() {
        use cardforge_api::{colorize, Command, CommandSender, Player};
        let dir = std::env::temp_dir().join(format!("cardforge-op-{}", Uuid::new_v4()));
        let cf = CardForge::new(&dir).unwrap();
        cf.permissions().set_default("cardforge.travel.setwarp", false);
        cf.commands().register(
            Command::new(
                "setwarp",
                "setwarp <name:word>",
                "test",
                "travel",
                |_s, _l, _a| vec![colorize("&aok")],
            )
            .level(1)
            .permission("cardforge.travel.setwarp"),
        );
        let mut player = Player::named(Uuid::nil(), "Tequilas");
        player.permission_level = 0;
        let denied = cf
            .dispatch_command(&CommandSender::Player(player.clone()), "setwarp home")
            .unwrap();
        assert!(denied[0].contains("op level"), "got {:?}", denied);

        player.permission_level = 4;
        let ok = cf
            .dispatch_command(&CommandSender::Player(player), "setwarp home")
            .unwrap();
        assert!(ok[0].contains("ok"), "got {:?}", ok);

        for (usage, level) in cf.command_usages() {
            assert_eq!(level, 0, "Horizon must see level 0 for {usage}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
