//! Network ranks + chat prefix (ported from ServerPlugin Rank / RankStore).

use cardforge_api::{
    colorize, Command, CommandSender, EventPriority, Plugin, PluginContext, PluginDescription,
    PluginResult, PluginServices, Server,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

/// Hierarchy order matches ServerPlugin (lower index = higher authority).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
enum Rank {
    Owner,
    Admin,
    Dev,
    Smod,
    Mod,
    Tmod,
    Builder,
    Elite,
    Mvp,
    Vip,
    Default,
}

impl Rank {
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "OWNER" => Some(Self::Owner),
            "ADMIN" => Some(Self::Admin),
            "DEV" => Some(Self::Dev),
            "SMOD" => Some(Self::Smod),
            "MOD" => Some(Self::Mod),
            "TMOD" => Some(Self::Tmod),
            "BUILDER" => Some(Self::Builder),
            "ELITE" => Some(Self::Elite),
            "MVP" => Some(Self::Mvp),
            "VIP" => Some(Self::Vip),
            "DEFAULT" | "PLAYER" | "" => Some(Self::Default),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Owner => "OWNER",
            Self::Admin => "ADMIN",
            Self::Dev => "DEV",
            Self::Smod => "SMOD",
            Self::Mod => "MOD",
            Self::Tmod => "TMOD",
            Self::Builder => "BUILDER",
            Self::Elite => "ELITE",
            Self::Mvp => "MVP",
            Self::Vip => "VIP",
            Self::Default => "",
        }
    }

    fn prefix(self, name: &str) -> String {
        match self {
            Self::Default => format!("&7{name}"),
            Self::Owner => format!("&fOWNER &4{name}"),
            Self::Admin => format!("&fADMIN &c{name}"),
            Self::Dev => format!("&fDEV &2{name}"),
            Self::Smod => format!("&fSMOD &6{name}"),
            Self::Mod => format!("&fMOD &5{name}"),
            Self::Tmod => format!("&fTMOD &d{name}"),
            Self::Builder => format!("&fBUILDER &3{name}"),
            Self::Elite => format!("&aELITE &7{name}"),
            Self::Mvp => format!("&9MVP &7{name}"),
            Self::Vip => format!("&bVIP &7{name}"),
        }
    }

    fn at_least(self, other: Self) -> bool {
        (self as u8) <= (other as u8)
    }
}

#[derive(Default, Serialize, Deserialize)]
struct RanksFile {
    ranks: HashMap<String, Rank>,
}

struct Shared {
    file: Mutex<RanksFile>,
    path: Mutex<Option<PathBuf>>,
}

impl Shared {
    fn rank_of(&self, uuid: Uuid) -> Rank {
        self.file
            .lock()
            .ranks
            .get(&uuid.to_string())
            .copied()
            .unwrap_or(Rank::Default)
    }

    fn set_rank(&self, uuid: Uuid, rank: Rank) {
        self.file
            .lock()
            .ranks
            .insert(uuid.to_string(), rank);
        self.save();
    }

    fn save(&self) {
        let Some(path) = self.path.lock().clone() else {
            return;
        };
        if let Ok(bytes) = serde_json::to_vec_pretty(&*self.file.lock()) {
            let _ = std::fs::write(path, bytes);
        }
    }
}

/// Ranks plugin: `/rank`, chat name colouring via format rewrite.
pub struct RanksPlugin {
    shared: Arc<Shared>,
}

impl Default for RanksPlugin {
    fn default() -> Self {
        Self {
            shared: Arc::new(Shared {
                file: Mutex::new(RanksFile::default()),
                path: Mutex::new(None),
            }),
        }
    }
}

impl Plugin for RanksPlugin {
    fn description(&self) -> PluginDescription {
        let mut d = PluginDescription::new("ranks", "Ranks", "0.1.0");
        d.authors = vec!["icedmoca".into()];
        d.description = "Network ranks and chat prefixes.".into();
        d
    }

    fn on_load(&mut self, services: &dyn PluginServices) -> PluginResult {
        services.permissions().set_default("cardforge.ranks.view", true);
        let shared_chat = Arc::clone(&self.shared);
        services.listen_player_chat(
            "ranks",
            EventPriority::Low,
            Box::new(move |event| {
                let rank = shared_chat.rank_of(event.player.uuid);
                let tagged = rank.prefix(&event.player.name);
                // Format is for plugins / future unsigned chat; Horizon signed chat ignores it.
                event.format = format!("{tagged}&r: &f{{message}}");
            }),
        );

        let shared_cmd = Arc::clone(&self.shared);
        services.register_command(
            Command::new(
                "rank",
                "rank <action:word>",
                "View or set ranks",
                "ranks",
                move |sender, _label, args| rank_cmd(&shared_cmd, sender, args),
            )
            .aliases(["ranks"])
            .permission("cardforge.ranks.view"),
        );
        Ok(())
    }

    fn on_enable(&mut self, ctx: &PluginContext, _server: &Server) -> PluginResult {
        std::fs::create_dir_all(&ctx.data_folder)?;
        let path = ctx.data_folder.join("ranks.json");
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(file) = serde_json::from_slice::<RanksFile>(&bytes) {
                *self.shared.file.lock() = file;
            }
        }
        *self.shared.path.lock() = Some(path);
        Ok(())
    }
}

fn rank_cmd(shared: &Shared, sender: &CommandSender, args: &[&str]) -> Vec<String> {
    let sub = args.first().copied().unwrap_or("me").to_ascii_lowercase();
    match sub.as_str() {
        "me" | "get" => {
            let CommandSender::Player(p) = sender else {
                return vec![colorize("&7Console has no rank.")];
            };
            let r = shared.rank_of(p.uuid);
            vec![colorize(&format!(
                "&7Your rank: {}",
                r.prefix(&p.name)
            ))]
        }
        "set" => {
            if sender.permission_level() < 3 {
                return vec![colorize("&cOp level 3+ required to set ranks.")];
            }
            let (Some(name), Some(rank_s)) = (args.get(1).copied(), args.get(2).copied()) else {
                return vec![colorize("&7Usage: /rank set <player> <rank>")];
            };
            let Some(rank) = Rank::parse(rank_s) else {
                return vec![colorize("&cUnknown rank.")];
            };
            // Name-keyed offline id until a player directory exists.
            let uuid = name_uuid(name);
            shared.set_rank(uuid, rank);
            // Also set if sender targets self by name.
            if let CommandSender::Player(p) = sender {
                if p.name.eq_ignore_ascii_case(name) {
                    shared.set_rank(p.uuid, rank);
                }
            }
            let _ = rank.at_least(Rank::Default);
            vec![colorize(&format!(
                "&aSet &f{name}&a to &f{}",
                rank_label(rank)
            ))]
        }
        "list" => vec![colorize(
            "&7Ranks: &fOWNER ADMIN DEV SMOD MOD TMOD BUILDER ELITE MVP VIP DEFAULT",
        )],
        _ => vec![colorize("&7/rank <me|set|list>")],
    }
}

fn rank_label(rank: Rank) -> String {
    let l = rank.label();
    if l.is_empty() {
        "DEFAULT".into()
    } else {
        l.into()
    }
}

fn name_uuid(name: &str) -> Uuid {
    let mut bytes = [0u8; 16];
    for (i, b) in name.to_ascii_lowercase().bytes().enumerate() {
        bytes[i % 16] ^= b.wrapping_add(i as u8);
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4-ish
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

/// Construct for in-process registration.
pub fn plugin() -> Box<dyn Plugin> {
    Box::new(RanksPlugin::default())
}
