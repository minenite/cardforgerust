//! Friends list + `/friend` commands (ported from ServerPlugin FriendStore).

use cardforge_api::{
    colorize, Command, CommandSender, Plugin, PluginContext, PluginDescription, PluginResult,
    PluginServices, Server,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Friendship {
    uuid: Uuid,
    name: String,
    since: i64,
}

#[derive(Default, Serialize, Deserialize)]
struct FriendsFile {
    players: HashMap<String, HashMap<String, Friendship>>,
}

struct Shared {
    file: Mutex<FriendsFile>,
    path: Mutex<Option<PathBuf>>,
}

impl Shared {
    fn save(&self) {
        let Some(path) = self.path.lock().clone() else {
            return;
        };
        if let Ok(bytes) = serde_json::to_vec_pretty(&*self.file.lock()) {
            let _ = std::fs::write(path, bytes);
        }
    }
}

/// In-memory + JSON friends store.
pub struct FriendsPlugin {
    shared: Arc<Shared>,
}

impl Default for FriendsPlugin {
    fn default() -> Self {
        Self {
            shared: Arc::new(Shared {
                file: Mutex::new(FriendsFile::default()),
                path: Mutex::new(None),
            }),
        }
    }
}

impl Plugin for FriendsPlugin {
    fn description(&self) -> PluginDescription {
        let mut d = PluginDescription::new("friends", "Friends", "0.1.0");
        d.authors = vec!["icedmoca".into()];
        d.description = "Friends list and /friend commands.".into();
        d
    }

    fn on_load(&mut self, services: &dyn PluginServices) -> PluginResult {
        services.permissions().set_default("cardforge.friends.use", true);
        let shared = Arc::clone(&self.shared);
        services.register_command(
            Command::new(
                "friend",
                "friend <action:word>",
                "Manage friends",
                "friends",
                move |sender, _label, args| friend_cmd(&shared, sender, args),
            )
            .aliases(["friends", "f"])
            .permission("cardforge.friends.use"),
        );
        Ok(())
    }

    fn on_enable(&mut self, ctx: &PluginContext, _server: &Server) -> PluginResult {
        std::fs::create_dir_all(&ctx.data_folder)?;
        let path = ctx.data_folder.join("friends.json");
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(file) = serde_json::from_slice::<FriendsFile>(&bytes) {
                *self.shared.file.lock() = file;
            }
        }
        *self.shared.path.lock() = Some(path);
        log::info!(
            "cf-friends: loaded {} player entries",
            self.shared.file.lock().players.len()
        );
        Ok(())
    }
}

fn friend_cmd(shared: &Shared, sender: &CommandSender, args: &[&str]) -> Vec<String> {
    let CommandSender::Player(player) = sender else {
        return vec![colorize("&cPlayers only.")];
    };
    let sub = args.first().copied().unwrap_or("list").to_ascii_lowercase();
    match sub.as_str() {
        "list" | "ls" => {
            let key = player.uuid.to_string();
            let g = shared.file.lock();
            let empty = g
                .players
                .get(&key)
                .map(|m| m.is_empty())
                .unwrap_or(true);
            if empty {
                vec![colorize("&7You have no friends yet.")]
            } else {
                let mut names: Vec<_> = g
                    .players
                    .get(&key)
                    .map(|m| m.values().map(|f| f.name.clone()).collect())
                    .unwrap_or_default();
                names.sort();
                vec![colorize(&format!(
                    "&aFriends &7({}): &f{}",
                    names.len(),
                    names.join("&7, &f")
                ))]
            }
        }
        "add" => {
            let Some(name) = args.get(1).copied() else {
                return vec![colorize("&7Usage: /friend add <online-name>")];
            };
            // Name-only add stores a nil UUID placeholder until presence exists.
            let now = now_ms();
            let mut g = shared.file.lock();
            let ka = player.uuid.to_string();
            let fake = Uuid::new_v4();
            let kb = fake.to_string();
            if g.players
                .get(&ka)
                .is_some_and(|m| m.values().any(|f| f.name.eq_ignore_ascii_case(name)))
            {
                return vec![colorize("&eAlready on your list.")];
            }
            g.players.entry(ka.clone()).or_default().insert(
                kb.clone(),
                Friendship {
                    uuid: fake,
                    name: name.into(),
                    since: now,
                },
            );
            g.players.entry(kb).or_default().insert(
                ka,
                Friendship {
                    uuid: player.uuid,
                    name: player.name.clone(),
                    since: now,
                },
            );
            drop(g);
            shared.save();
            vec![colorize(&format!(
                "&aAdded &f{name}&a. &7(UUID placeholder until network presence lands.)"
            ))]
        }
        "remove" | "rm" => {
            let Some(name) = args.get(1).copied() else {
                return vec![colorize("&7Usage: /friend remove <name>")];
            };
            let mut g = shared.file.lock();
            let ka = player.uuid.to_string();
            let Some(map) = g.players.get_mut(&ka) else {
                return vec![colorize("&cNot on your list.")];
            };
            let victim = map
                .iter()
                .find(|(_, f)| f.name.eq_ignore_ascii_case(name))
                .map(|(k, f)| (k.clone(), f.name.clone()));
            let Some((kb, fname)) = victim else {
                return vec![colorize("&cNot on your list.")];
            };
            map.remove(&kb);
            if let Some(other) = g.players.get_mut(&kb) {
                other.remove(&ka);
            }
            drop(g);
            shared.save();
            vec![colorize(&format!("&7Removed &f{fname}&7."))]
        }
        _ => vec![colorize("&7/friend <list|add|remove>")],
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Construct for in-process registration.
pub fn plugin() -> Box<dyn Plugin> {
    Box::new(FriendsPlugin::default())
}
