//! CardForge Horizon trait mod.
//!
//! Linked into `horizon-client` by the launcher. Maps Horizon native hooks onto
//! [`cardforge_core::CardForge`] so Rust plugins see Bukkit-style events.

use cardforge_api::{
    colorize, Cancellable, CommandSender, GameTickEvent, Player,
    PlayerChatEvent as CfPlayerChatEvent, PlayerJoinEvent, PlayerQuitEvent, PlayerRespawnEvent,
    GAME_VERSION,
};
use cardforge_core::CardForge;
use horizon_client::mod_api::{
    Flow, Host, Hooks, NativeCommand, PlayerChatEvent as HzPlayerChatEvent, PlayerCommandEvent,
    PlayerLoggedInEvent, PlayerLoggedOutEvent, PlayerRespawnedEvent, RegisterCommandsEvent,
    ServerAboutToStartEvent, ServerStoppingEvent, ServerTickEvent, ServerTickPhase, Text,
};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

static RUNTIME: OnceLock<Mutex<Option<CardForge>>> = OnceLock::new();

fn runtime() -> &'static Mutex<Option<CardForge>> {
    RUNTIME.get_or_init(|| Mutex::new(None))
}

fn plugins_dir() -> PathBuf {
    if let Ok(path) = std::env::var("CARDFORGE_PLUGINS") {
        return PathBuf::from(path);
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("plugins")
}

fn uuid_of(player: u128) -> Uuid {
    Uuid::from_u128(player)
}

/// Entrypoint the Horizon launcher calls when linking this trait mod.
pub fn hooks() -> Box<dyn Hooks> {
    Box::new(CardForgeMod)
}

struct CardForgeMod;

impl Hooks for CardForgeMod {
    fn server_about_to_start(&self, _host: &mut dyn Host, event: &mut ServerAboutToStartEvent) {
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
        for (usage, level) in cf.command_usages() {
            event.commands.push(if level == 0 {
                NativeCommand::new(&usage)
            } else {
                NativeCommand::op(&usage, level)
            });
        }
    }

    fn player_command(&self, host: &mut dyn Host, event: &mut PlayerCommandEvent<'_>) -> Flow {
        let guard = runtime().lock().expect("cardforge runtime");
        let Some(cf) = guard.as_ref() else {
            return Flow::Continue;
        };
        let mut player = Player::named(uuid_of(event.player), event.name);
        // Horizon already knows their ops.json level — CardForge used to leave this at 0,
        // so /setwarp (level 3) always failed even for operators.
        if let Some(np) = host
            .server_players()
            .into_iter()
            .find(|p| p.uuid == event.player)
        {
            player.permission_level = np.permission;
            player.dimension = np.dimension;
            player.position = np.position;
            player.yaw = np.yaw;
            player.pitch = np.pitch;
            player.health = np.health;
            player.game_mode = np.game_mode.clamp(0, 255) as u8;
        }
        if player.permission_level >= 1 {
            cf.permissions().set_op(player.uuid, true);
        }
        let sender = CommandSender::Player(player);
        let Some(replies) = cf.dispatch_command(&sender, event.command) else {
            return Flow::Continue;
        };
        for line in replies {
            event.replies.push(Text::legacy(&line));
        }
        Flow::Cancel
    }

    fn player_chat(&self, event: &mut HzPlayerChatEvent<'_>) -> Flow {
        let player = Player::named(uuid_of(event.player), event.name);
        let mut chat = CfPlayerChatEvent::new(player, event.message);
        {
            let guard = runtime().lock().expect("cardforge runtime");
            if let Some(cf) = guard.as_ref() {
                cf.call(&mut chat);
            }
        }
        let cancelled = chat.is_cancelled();
        for line in &chat.replies {
            event.replies.push(Text::legacy(&colorize(line)));
        }
        // Horizon cannot rewrite signed chat bodies — cancel only.
        if cancelled {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn player_logged_in(&self, host: &mut dyn Host, event: &mut PlayerLoggedInEvent<'_>) {
        let player = Player::named(uuid_of(event.player), event.name);
        let mut join = PlayerJoinEvent {
            player: player.clone(),
            join_message: Some(format!("{} joined the game", player.name)),
        };
        {
            let guard = runtime().lock().expect("cardforge runtime");
            if let Some(cf) = guard.as_ref() {
                let mut snap = cf.server().snapshot();
                if !snap.players.iter().any(|p| p.uuid == player.uuid) {
                    snap.players.push(player.clone());
                }
                snap.game_version = GAME_VERSION.to_string();
                cf.set_snapshot(snap);
                cf.call(&mut join);
            }
        }
        if let Some(msg) = join.join_message {
            host.tell(event.player, Text::literal(msg));
        }
    }

    fn player_logged_out(&self, _host: &mut dyn Host, event: &mut PlayerLoggedOutEvent<'_>) {
        let player = Player::named(uuid_of(event.player), event.name);
        let mut quit = PlayerQuitEvent {
            player: player.clone(),
            quit_message: Some(format!("{} left the game", player.name)),
        };
        let guard = runtime().lock().expect("cardforge runtime");
        if let Some(cf) = guard.as_ref() {
            let mut snap = cf.server().snapshot();
            snap.players.retain(|p| p.uuid != player.uuid);
            cf.set_snapshot(snap);
            cf.call(&mut quit);
        }
    }

    fn player_respawned(&self, _host: &mut dyn Host, event: &mut PlayerRespawnedEvent<'_>) {
        let mut respawn = PlayerRespawnEvent {
            player: Player::named(uuid_of(event.player), event.name),
        };
        let guard = runtime().lock().expect("cardforge runtime");
        if let Some(cf) = guard.as_ref() {
            cf.call(&mut respawn);
        }
    }

    fn server_tick(&self, _host: &mut dyn Host, event: &mut ServerTickEvent<'_>) {
        let pre = matches!(event.phase, ServerTickPhase::ServerTickStart { .. });
        let mut tick = GameTickEvent {
            tick: event.context.tick.get(),
            pre,
        };
        let guard = runtime().lock().expect("cardforge runtime");
        if let Some(cf) = guard.as_ref() {
            cf.call(&mut tick);
        }
    }
}
