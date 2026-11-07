//! CardForge Horizon trait mod.
//!
//! Linked into `horizon-client` by the launcher. Maps Horizon native hooks onto
//! [`cardforge_core::CardForge`] so Rust plugins see Bukkit-style events.

use cardforge_api::{
    CommandSender, GameTickEvent, Player, PlayerJoinEvent, PlayerQuitEvent, PlayerRespawnEvent,
    GAME_VERSION,
};
use cardforge_core::CardForge;
use horizon_client::mod_api::{
    Flow, Host, Hooks, NativeCommand, PlayerCommandEvent, PlayerLoggedInEvent,
    PlayerLoggedOutEvent, PlayerRespawnedEvent, RegisterCommandsEvent, ServerAboutToStartEvent,
    ServerStoppingEvent, ServerTickEvent, ServerTickPhase, Text,
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
                if let Err(err) = cf.register(hello_plugin::plugin()) {
                    log::error!("cardforge: register hello: {err}");
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
        let Some(cf) = runtime().lock().expect("cardforge runtime").as_ref() else {
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

    fn player_command(&self, _host: &mut dyn Host, event: &mut PlayerCommandEvent<'_>) -> Flow {
        let Some(cf) = runtime().lock().expect("cardforge runtime").as_ref() else {
            return Flow::Continue;
        };
        let sender = CommandSender::Player(Player::named(uuid_of(event.player), event.name));
        let Some(replies) = cf.dispatch_command(&sender, event.command) else {
            return Flow::Continue;
        };
        for line in replies {
            event.replies.push(Text::literal(line));
        }
        Flow::Cancel
    }

    fn player_logged_in(&self, host: &mut dyn Host, event: &mut PlayerLoggedInEvent<'_>) {
        let player = Player::named(uuid_of(event.player), event.name);
        let mut join = PlayerJoinEvent {
            player: player.clone(),
            join_message: Some(format!("{} joined the game", player.name)),
        };
        if let Some(cf) = runtime().lock().expect("cardforge runtime").as_ref() {
            let mut snap = cf.server().snapshot();
            if !snap.players.iter().any(|p| p.uuid == player.uuid) {
                snap.players.push(player.clone());
            }
            snap.game_version = GAME_VERSION.to_string();
            cf.set_snapshot(snap);
            cf.call(&mut join);
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
        if let Some(cf) = runtime().lock().expect("cardforge runtime").as_ref() {
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
        if let Some(cf) = runtime().lock().expect("cardforge runtime").as_ref() {
            cf.call(&mut respawn);
        }
    }

    fn server_tick(&self, _host: &mut dyn Host, event: &mut ServerTickEvent<'_>) {
        let pre = matches!(event.phase, ServerTickPhase::ServerTickStart { .. });
        let mut tick = GameTickEvent {
            tick: event.context.tick.get(),
            pre,
        };
        if let Some(cf) = runtime().lock().expect("cardforge runtime").as_ref() {
            cf.call(&mut tick);
        }
    }
}
