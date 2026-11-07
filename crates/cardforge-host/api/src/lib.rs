//! CardForge Horizon trait mod.

use cardforge_api::{
    colorize, Cancellable, CommandSender, GameTickEvent, HostAction, MenuClickAction, MenuItem,
    OnlinePlayer, Player, PlayerChatEvent as CfPlayerChatEvent, PlayerJoinEvent, PlayerQuitEvent,
    PlayerRespawnEvent, GAME_VERSION,
};
use cardforge_core::CardForge;
use horizon_client::mod_api::{
    Flow, Host, Hooks, ItemStack, MenuClickedEvent, NativeCommand, PlayerChatEvent as HzPlayerChatEvent,
    PlayerCommandEvent, PlayerLoggedInEvent, PlayerLoggedOutEvent, PlayerRespawnedEvent,
    RegisterCommandsEvent, ServerAboutToStartEvent, ServerStoppingEvent, ServerTickEvent,
    ServerTickPhase, Tag, Text,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

static RUNTIME: OnceLock<Mutex<Option<CardForge>>> = OnceLock::new();
/// (player_uuid, window_id) → slot → action
static MENUS: OnceLock<Mutex<HashMap<(u128, i32), HashMap<i32, MenuClickAction>>>> = OnceLock::new();
/// uuid → op level from ops.json (refreshed on start / join)
static OPS_FILE: OnceLock<Mutex<HashMap<Uuid, u8>>> = OnceLock::new();

fn runtime() -> &'static Mutex<Option<CardForge>> {
    RUNTIME.get_or_init(|| Mutex::new(None))
}

fn menus() -> &'static Mutex<HashMap<(u128, i32), HashMap<i32, MenuClickAction>>> {
    MENUS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ops_file() -> &'static Mutex<HashMap<Uuid, u8>> {
    OPS_FILE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn plugins_dir() -> PathBuf {
    if let Ok(path) = std::env::var("CARDFORGE_PLUGINS") {
        return PathBuf::from(path);
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("plugins")
}

fn server_root() -> PathBuf {
    plugins_dir()
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn uuid_of(player: u128) -> Uuid {
    Uuid::from_u128(player)
}

fn reload_ops_json() {
    let path = server_root().join("ops.json");
    let mut map = HashMap::new();
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(list) = serde_json::from_slice::<Vec<serde_json::Value>>(&bytes) {
            for entry in list {
                let level = entry
                    .get("level")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(4)
                    .min(4) as u8;
                if let Some(u) = entry
                    .get("uuid")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                {
                    map.insert(u, level);
                }
            }
        }
    }
    *ops_file().lock().expect("ops") = map;
}

fn op_level_for(uuid: Uuid, host_permission: u8) -> u8 {
    let from_file = ops_file()
        .lock()
        .expect("ops")
        .get(&uuid)
        .copied()
        .unwrap_or(0);
    host_permission.max(from_file)
}

fn apply_player_perms(cf: &CardForge, uuid: Uuid, level: u8) {
    if level >= 1 {
        cf.permissions().set_op(uuid, true);
        // Explicit admin nodes so defaults=false still pass for operators.
        cf.permissions()
            .set(uuid, "cardforge.travel.setwarp", true);
        cf.permissions().set(uuid, "cardforge.ranks.set", true);
    }
}

fn sync_online(host: &dyn Host, cf: &CardForge) {
    let players: Vec<OnlinePlayer> = host
        .server_players()
        .into_iter()
        .map(|np| {
            let uuid = uuid_of(np.uuid);
            let permission_level = op_level_for(uuid, np.permission);
            apply_player_perms(cf, uuid, permission_level);
            OnlinePlayer {
                uuid,
                name: np.name.clone(),
                dimension: np.dimension.clone(),
                position: np.position,
                yaw: np.yaw,
                pitch: np.pitch,
                permission_level,
            }
        })
        .collect();
    let pairs: Vec<(Uuid, String)> = players
        .iter()
        .map(|p| (p.uuid, p.name.clone()))
        .collect();
    cf_friends::set_online(&pairs);
    cf.set_online(players);
}

fn menu_item_to_stack(item: &MenuItem) -> ItemStack {
    use std::sync::Arc;
    let mut stack = ItemStack {
        item: Arc::from(item.item.as_str()),
        count: item.count.max(1),
        damage: 0,
        extra: None,
    };
    if let Some(name) = &item.name {
        // Plain custom name (legacy § codes work in the string component).
        let labeled = colorize(name);
        stack.data_mut().custom_name = Some(Tag::String(labeled.as_str().into()));
    }
    if !item.lore.is_empty() {
        // Lore is best-effort; many clients show custom_name alone which is enough for GUI.
        let _ = &item.lore;
    }
    stack
}

fn apply_actions(host: &mut dyn Host, actions: Vec<HostAction>) {
    for action in actions {
        match action {
            HostAction::Teleport {
                uuid,
                dimension,
                position,
                yaw,
                pitch,
            } => {
                if let Some(np) = host
                    .server_players()
                    .into_iter()
                    .find(|p| p.uuid == uuid.as_u128())
                {
                    let moved = host.teleport_mob(&dimension, np.entity_id as u32, position);
                    // Coordinate tp (not entity-to-entity) so the client gets yaw/pitch.
                    let line = if np.dimension != dimension {
                        format!(
                            "execute in {dimension} run tp {} {} {} {} {} {}",
                            np.name, position[0], position[1], position[2], yaw, pitch
                        )
                    } else {
                        format!(
                            "tp {} {} {} {} {} {}",
                            np.name, position[0], position[1], position[2], yaw, pitch
                        )
                    };
                    if !host.run_command(&line) && !moved {
                        log::warn!(
                            "cardforge: teleport failed for {} -> {} {:?}",
                            np.name,
                            dimension,
                            position
                        );
                    }
                }
            }
            HostAction::Broadcast { line } => {
                host.broadcast(Text::legacy(&colorize(&line)));
            }
            HostAction::OpenMenu {
                uuid,
                rows,
                title,
                slots,
            } => {
                let rows = rows.clamp(1, 6);
                let size = (rows as usize) * 9;
                let mut items: Vec<Option<ItemStack>> = slots
                    .iter()
                    .map(|s| s.item.as_ref().map(menu_item_to_stack))
                    .collect();
                items.resize(size, None);
                let mut bindings = HashMap::new();
                for (i, s) in slots.iter().enumerate().take(size) {
                    if !matches!(s.action, MenuClickAction::None) {
                        bindings.insert(i as i32, s.action.clone());
                    }
                }
                match host.open_menu(
                    uuid.as_u128(),
                    rows,
                    Text::legacy(&colorize(&title)),
                    items,
                ) {
                    Some(window) => {
                        menus()
                            .lock()
                            .expect("menus")
                            .insert((uuid.as_u128(), window), bindings);
                        log::info!("cardforge: opened menu window={window} for {uuid}");
                    }
                    None => {
                        log::warn!("cardforge: open_menu failed for {uuid}");
                        host.tell(
                            uuid.as_u128(),
                            Text::legacy(&colorize(
                                "&cCould not open menu. Try again in a moment.",
                            )),
                        );
                    }
                }
            }
            HostAction::CloseMenu { uuid } => {
                host.close_menu(uuid.as_u128());
                menus()
                    .lock()
                    .expect("menus")
                    .retain(|(u, _), _| *u != uuid.as_u128());
            }
            HostAction::RunCommand { line } => {
                let _ = host.run_command(&line);
            }
        }
    }
}

/// Entrypoint the Horizon launcher calls when linking this trait mod.
pub fn hooks() -> Box<dyn Hooks> {
    Box::new(CardForgeMod)
}

struct CardForgeMod;

impl Hooks for CardForgeMod {
    fn server_about_to_start(&self, _host: &mut dyn Host, event: &mut ServerAboutToStartEvent) {
        reload_ops_json();
        let dir = plugins_dir();
        match CardForge::new(&dir) {
            Ok(cf) => {
                for (name, result) in [
                    ("hello", cf.register(hello_plugin::plugin())),
                    ("friends", cf.register(cf_friends::plugin())),
                    ("ranks", cf.register(cf_ranks::plugin())),
                    ("travel", cf.register(cf_travel::plugin())),
                ] {
                    if let Err(err) = result {
                        log::error!("cardforge: register {name}: {err}");
                    }
                }
                match cf.load_dynamic_plugins() {
                    Ok(n) if n > 0 => log::info!("cardforge: loaded {n} dynamic plugin(s)"),
                    Ok(_) => {}
                    Err(err) => log::error!("cardforge: dynamic load: {err}"),
                }
                if let Err(err) = cf.enable_all() {
                    log::error!("cardforge: enable plugins: {err}");
                }
                log::info!(
                    "cardforge: ready on Horizon {} (dedicated={}, plugins={})",
                    GAME_VERSION,
                    event.dedicated,
                    dir.display()
                );
                *runtime().lock().expect("cardforge runtime") = Some(cf);
            }
            Err(err) => log::error!("cardforge: failed to start at {}: {err}", dir.display()),
        }
    }

    fn server_stopping(&self, _host: &mut dyn Host, _event: &mut ServerStoppingEvent) {
        if let Some(cf) = runtime().lock().expect("cardforge runtime").as_ref() {
            cf.disable_all();
        }
        *runtime().lock().expect("cardforge runtime") = None;
        log::info!("cardforge: stopped");
    }

    fn register_commands(&self, _host: &mut dyn Host, event: &mut RegisterCommandsEvent<'_>) {
        let guard = runtime().lock().expect("cardforge runtime");
        let Some(cf) = guard.as_ref() else {
            return;
        };
        // Level is always 0 from native_usages; CardForge enforces real levels.
        for (usage, _level) in cf.command_usages() {
            event.commands.push(NativeCommand::new(&usage));
        }
    }

    fn player_command(&self, host: &mut dyn Host, event: &mut PlayerCommandEvent<'_>) -> Flow {
        let actions = {
            let guard = runtime().lock().expect("cardforge runtime");
            let Some(cf) = guard.as_ref() else {
                return Flow::Continue;
            };
            sync_online(host, cf);
            let uuid = uuid_of(event.player);
            let mut player = Player::named(uuid, event.name);
            if let Some(np) = host
                .server_players()
                .into_iter()
                .find(|p| p.uuid == event.player)
            {
                player.permission_level = op_level_for(uuid, np.permission);
                player.dimension = np.dimension;
                player.position = np.position;
                player.yaw = np.yaw;
                player.pitch = np.pitch;
                player.health = np.health;
                player.game_mode = np.game_mode.clamp(0, 255) as u8;
            } else {
                player.permission_level = op_level_for(uuid, 0);
            }
            apply_player_perms(cf, uuid, player.permission_level);
            let sender = CommandSender::Player(player);
            let Some(replies) = cf.dispatch_command(&sender, event.command) else {
                return Flow::Continue;
            };
            for line in replies {
                event.replies.push(Text::legacy(&line));
            }
            cf.take_actions()
        };
        apply_actions(host, actions);
        Flow::Cancel
    }

    fn player_chat(&self, event: &mut HzPlayerChatEvent<'_>) -> Flow {
        let player = Player::named(uuid_of(event.player), event.name);
        let mut chat = CfPlayerChatEvent::new(player, event.message);
        let actions = {
            let guard = runtime().lock().expect("cardforge runtime");
            if let Some(cf) = guard.as_ref() {
                cf.call(&mut chat);
                cf.take_actions()
            } else {
                Vec::new()
            }
        };
        for line in &chat.replies {
            event.replies.push(Text::legacy(&colorize(line)));
        }
        PENDING.lock().expect("pending").extend(actions);
        if chat.is_cancelled() {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn menu_clicked(&self, host: &mut dyn Host, event: &mut MenuClickedEvent) {
        let key = (event.uuid, event.window);
        let action = menus()
            .lock()
            .expect("menus")
            .get(&key)
            .and_then(|m| m.get(&event.slot))
            .cloned();
        let Some(action) = action else {
            return;
        };
        let name = host
            .server_players()
            .into_iter()
            .find(|p| p.uuid == event.uuid)
            .map(|p| p.name)
            .unwrap_or_else(|| "player".into());
        cf_friends::handle_menu_click(uuid_of(event.uuid), &name, &action);
        let actions = {
            let guard = runtime().lock().expect("cardforge runtime");
            guard
                .as_ref()
                .map(|cf| cf.take_actions())
                .unwrap_or_default()
        };
        apply_actions(host, actions);
    }

    fn player_logged_in(&self, host: &mut dyn Host, event: &mut PlayerLoggedInEvent<'_>) {
        reload_ops_json();
        let uuid = uuid_of(event.player);
        let mut player = Player::named(uuid, event.name);
        if let Some(np) = host
            .server_players()
            .into_iter()
            .find(|p| p.uuid == event.player)
        {
            player.permission_level = op_level_for(uuid, np.permission);
            player.dimension = np.dimension;
            player.position = np.position;
        } else {
            player.permission_level = op_level_for(uuid, 0);
        }
        let mut join = PlayerJoinEvent {
            player: player.clone(),
            join_message: Some(format!("{} joined the game", player.name)),
        };
        let actions = {
            let guard = runtime().lock().expect("cardforge runtime");
            if let Some(cf) = guard.as_ref() {
                apply_player_perms(cf, uuid, player.permission_level);
                sync_online(host, cf);
                let mut snap = cf.server().snapshot();
                if !snap.players.iter().any(|p| p.uuid == player.uuid) {
                    snap.players.push(player.clone());
                }
                snap.game_version = GAME_VERSION.to_string();
                cf.set_snapshot(snap);
                cf.call(&mut join);
                cf.take_actions()
            } else {
                Vec::new()
            }
        };
        if let Some(msg) = join.join_message {
            host.tell(event.player, Text::literal(msg));
        }
        // Confirm op level so admins know CardForge sees them.
        if player.permission_level >= 1 {
            host.tell(
                event.player,
                Text::legacy(&colorize(&format!(
                    "&7CardForge: op level &f{}&7 (setwarp/ranks unlocked)",
                    player.permission_level
                ))),
            );
        }
        apply_actions(host, actions);
    }

    fn player_logged_out(&self, host: &mut dyn Host, event: &mut PlayerLoggedOutEvent<'_>) {
        let player = Player::named(uuid_of(event.player), event.name);
        let mut quit = PlayerQuitEvent {
            player: player.clone(),
            quit_message: Some(format!("{} left the game", player.name)),
        };
        menus()
            .lock()
            .expect("menus")
            .retain(|(u, _), _| *u != event.player);
        let actions = {
            let guard = runtime().lock().expect("cardforge runtime");
            if let Some(cf) = guard.as_ref() {
                let mut snap = cf.server().snapshot();
                snap.players.retain(|p| p.uuid != player.uuid);
                cf.set_snapshot(snap);
                cf.call(&mut quit);
                sync_online(host, cf);
                cf.take_actions()
            } else {
                Vec::new()
            }
        };
        apply_actions(host, actions);
    }

    fn player_respawned(&self, host: &mut dyn Host, event: &mut PlayerRespawnedEvent<'_>) {
        let mut respawn = PlayerRespawnEvent {
            player: Player::named(uuid_of(event.player), event.name),
        };
        let actions = {
            let guard = runtime().lock().expect("cardforge runtime");
            if let Some(cf) = guard.as_ref() {
                cf.call(&mut respawn);
                cf.take_actions()
            } else {
                Vec::new()
            }
        };
        apply_actions(host, actions);
    }

    fn server_tick(&self, host: &mut dyn Host, event: &mut ServerTickEvent<'_>) {
        let pre = matches!(event.phase, ServerTickPhase::ServerTickStart { .. });
        let mut tick = GameTickEvent {
            tick: event.context.tick.get(),
            pre,
        };
        let mut actions = PENDING.lock().expect("pending").drain(..).collect::<Vec<_>>();
        {
            let guard = runtime().lock().expect("cardforge runtime");
            if let Some(cf) = guard.as_ref() {
                if pre {
                    sync_online(host, cf);
                }
                cf.call(&mut tick);
                actions.extend(cf.take_actions());
            }
        }
        apply_actions(host, actions);
    }
}

static PENDING: Mutex<Vec<HostAction>> = Mutex::new(Vec::new());
