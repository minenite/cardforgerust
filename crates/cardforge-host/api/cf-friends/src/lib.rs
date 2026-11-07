//! Friends list, requests, live UUID add, and chest GUI (ServerPlugin-style).

use cardforge_api::{
    colorize, Command, CommandSender, HostAction, MenuClickAction, MenuItem, MenuSlot, Plugin,
    PluginContext, PluginDescription, PluginResult, PluginServices, Server,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Request {
    uuid: Uuid,
    name: String,
    sent: i64,
}

#[derive(Default, Serialize, Deserialize)]
struct FriendsFile {
    players: HashMap<String, HashMap<String, Friendship>>,
    /// target uuid → incoming requests
    requests: HashMap<String, Vec<Request>>,
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

    fn friends_of(&self, owner: Uuid) -> Vec<Friendship> {
        let key = owner.to_string();
        let g = self.file.lock();
        let mut v: Vec<_> = g
            .players
            .get(&key)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default();
        v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        v
    }

    fn requests_for(&self, target: Uuid) -> Vec<Request> {
        self.file
            .lock()
            .requests
            .get(&target.to_string())
            .cloned()
            .unwrap_or_default()
    }

    fn link(&self, a: Uuid, name_a: &str, b: Uuid, name_b: &str) -> bool {
        let mut g = self.file.lock();
        let ka = a.to_string();
        let kb = b.to_string();
        if g.players.get(&ka).and_then(|m| m.get(&kb)).is_some() {
            return false;
        }
        let now = now_ms();
        g.players.entry(ka.clone()).or_default().insert(
            kb.clone(),
            Friendship {
                uuid: b,
                name: name_b.into(),
                since: now,
            },
        );
        g.players.entry(kb).or_default().insert(
            ka,
            Friendship {
                uuid: a,
                name: name_a.into(),
                since: now,
            },
        );
        drop(g);
        self.save();
        true
    }

    fn unlink(&self, a: Uuid, b: Uuid) -> bool {
        let mut g = self.file.lock();
        let ka = a.to_string();
        let kb = b.to_string();
        let mut changed = false;
        if let Some(m) = g.players.get_mut(&ka) {
            changed |= m.remove(&kb).is_some();
        }
        if let Some(m) = g.players.get_mut(&kb) {
            changed |= m.remove(&ka).is_some();
        }
        drop(g);
        if changed {
            self.save();
        }
        changed
    }

    fn add_request(&self, from: Uuid, from_name: &str, to: Uuid) -> bool {
        let mut g = self.file.lock();
        let key = to.to_string();
        let list = g.requests.entry(key).or_default();
        if list.iter().any(|r| r.uuid == from) {
            return false;
        }
        list.push(Request {
            uuid: from,
            name: from_name.into(),
            sent: now_ms(),
        });
        drop(g);
        self.save();
        true
    }

    fn take_request(&self, to: Uuid, from: Uuid) -> Option<Request> {
        let mut g = self.file.lock();
        let key = to.to_string();
        let list = g.requests.get_mut(&key)?;
        let idx = list.iter().position(|r| r.uuid == from)?;
        let req = list.remove(idx);
        drop(g);
        self.save();
        Some(req)
    }
}

/// Friends plugin.
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
        d.description = "Friends list, requests, and /friend GUI.".into();
        d
    }

    fn on_load(&mut self, services: &dyn PluginServices) -> PluginResult {
        services.permissions().set_default("cardforge.friends.use", true);
        let sink = services.action_sink();
        let shared = Arc::clone(&self.shared);

        // Expose shared for host menu-click bridge.
        *FRIENDS_SHARED.lock() = Some(Arc::clone(&shared));
        *FRIENDS_SINK.lock() = Some(Arc::clone(&sink));

        let s_cmd = Arc::clone(&shared);
        let sink_cmd = Arc::clone(&sink);
        services.register_command(
            Command::new(
                "friend",
                "friend [action:word] [name:word]",
                "Friends list / add / GUI",
                "friends",
                move |sender, _label, args| {
                    friend_cmd(&s_cmd, sink_cmd.as_ref(), sender, args)
                },
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
        Ok(())
    }
}

static FRIENDS_SHARED: Mutex<Option<Arc<Shared>>> = Mutex::new(None);
static FRIENDS_SINK: Mutex<Option<Arc<dyn Fn(HostAction) + Send + Sync>>> = Mutex::new(None);

/// Host calls this for friends menu clicks.
pub fn handle_menu_click(viewer: Uuid, viewer_name: &str, action: &MenuClickAction) {
    let Some(shared) = FRIENDS_SHARED.lock().clone() else {
        return;
    };
    let Some(sink) = FRIENDS_SINK.lock().clone() else {
        return;
    };
    match action {
        MenuClickAction::FriendsList => open_list(&shared, sink.as_ref(), viewer),
        MenuClickAction::FriendRequests => open_requests(&shared, sink.as_ref(), viewer),
        MenuClickAction::FriendDetail(id) => {
            if let Some(f) = shared.friends_of(viewer).into_iter().find(|f| f.uuid == *id) {
                open_detail(&shared, sink.as_ref(), viewer, &f);
            }
        }
        MenuClickAction::FriendRemove(id) => {
            shared.unlink(viewer, *id);
            open_list(&shared, sink.as_ref(), viewer);
            sink(HostAction::Broadcast {
                line: format!("&7{viewer_name} updated their friends list."),
            });
        }
        MenuClickAction::FriendAccept(id) => {
            if let Some(req) = shared.take_request(viewer, *id) {
                shared.link(viewer, viewer_name, req.uuid, &req.name);
            }
            open_requests(&shared, sink.as_ref(), viewer);
        }
        MenuClickAction::FriendDeny(id) => {
            let _ = shared.take_request(viewer, *id);
            open_requests(&shared, sink.as_ref(), viewer);
        }
        MenuClickAction::FriendAddMenu => open_add(&shared, sink.as_ref(), viewer),
        MenuClickAction::FriendAdd(id) => {
            if *id != viewer {
                if let Some(target) = FRIENDS_ONLINE
                    .lock()
                    .iter()
                    .find(|p| p.uuid == *id)
                    .cloned()
                {
                    let _ = shared.add_request(viewer, viewer_name, target.uuid);
                }
            }
            open_add(&shared, sink.as_ref(), viewer);
        }
        MenuClickAction::Close => {
            sink(HostAction::CloseMenu { uuid: viewer });
        }
        MenuClickAction::None => {}
    }
}

fn friend_cmd(
    shared: &Shared,
    sink: &(dyn Fn(HostAction) + Send + Sync),
    sender: &CommandSender,
    args: &[&str],
) -> Vec<String> {
    let CommandSender::Player(player) = sender else {
        return vec![colorize("&cPlayers only.")];
    };
    let sub = args.first().copied().unwrap_or("gui").to_ascii_lowercase();
    match sub.as_str() {
        "gui" | "menu" => {
            open_list(shared, sink, player.uuid);
            vec![colorize("&aOpening friends menu…")]
        }
        "list" | "ls" => {
            let friends = shared.friends_of(player.uuid);
            if friends.is_empty() {
                vec![colorize("&7You have no friends yet. &8(/friend add <name>)")]
            } else {
                let names: Vec<_> = friends.into_iter().map(|f| f.name).collect();
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
            // Resolve via online snapshot: command dispatch happens after host set_online.
            // We read GLOBAL online from a side channel — CardForge online is on services
            // only during on_load. Host sets online before dispatch; friends needs it in
            // Shared. Host also copies online into FRIENDS_ONLINE before dispatch.
            let Some(target) = FRIENDS_ONLINE
                .lock()
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(name))
                .cloned()
            else {
                return vec![colorize(&format!("&cNo online player named &f{name}&c."))];
            };
            if target.uuid == player.uuid {
                return vec![colorize("&cYou cannot add yourself.")];
            }
            if shared
                .friends_of(player.uuid)
                .iter()
                .any(|f| f.uuid == target.uuid)
            {
                return vec![colorize("&eAlready friends.")];
            }
            if shared.add_request(player.uuid, &player.name, target.uuid) {
                vec![colorize(&format!(
                    "&aFriend request sent to &f{}&a.",
                    target.name
                ))]
            } else {
                vec![colorize("&eRequest already pending.")]
            }
        }
        "accept" => {
            let Some(name) = args.get(1).copied() else {
                return vec![colorize("&7Usage: /friend accept <name>")];
            };
            let reqs = shared.requests_for(player.uuid);
            let Some(req) = reqs.into_iter().find(|r| r.name.eq_ignore_ascii_case(name)) else {
                return vec![colorize("&cNo such request.")];
            };
            let _ = shared.take_request(player.uuid, req.uuid);
            shared.link(player.uuid, &player.name, req.uuid, &req.name);
            vec![colorize(&format!("&aYou are now friends with &f{}&a.", req.name))]
        }
        "remove" | "rm" => {
            let Some(name) = args.get(1).copied() else {
                return vec![colorize("&7Usage: /friend remove <name>")];
            };
            let friends = shared.friends_of(player.uuid);
            let Some(f) = friends.into_iter().find(|f| f.name.eq_ignore_ascii_case(name)) else {
                return vec![colorize("&cNot on your list.")];
            };
            shared.unlink(player.uuid, f.uuid);
            vec![colorize(&format!("&7Removed &f{}&7.", f.name))]
        }
        "requests" => {
            open_requests(shared, sink, player.uuid);
            vec![colorize("&aOpening friend requests…")]
        }
        _ => {
            open_list(shared, sink, player.uuid);
            vec![colorize(
                "&7/friend <gui|list|add|accept|remove|requests>",
            )]
        }
    }
}

#[derive(Clone)]
struct OnlineName {
    uuid: Uuid,
    name: String,
}

static FRIENDS_ONLINE: Mutex<Vec<OnlineName>> = Mutex::new(Vec::new());

/// Host refreshes the name→uuid table before command dispatch.
pub fn set_online(players: &[(Uuid, String)]) {
    *FRIENDS_ONLINE.lock() = players
        .iter()
        .map(|(u, n)| OnlineName {
            uuid: *u,
            name: n.clone(),
        })
        .collect();
}

fn empty_slot() -> MenuSlot {
    MenuSlot {
        item: None,
        action: MenuClickAction::None,
    }
}

fn open_list(shared: &Shared, sink: &(dyn Fn(HostAction) + Send + Sync), viewer: Uuid) {
    // 6-row chest: rows 0–4 friends (45), row 5 toolbar (add / requests / close).
    let friends = shared.friends_of(viewer);
    let mut slots = vec![empty_slot(); 54];
    for (i, f) in friends.iter().take(45).enumerate() {
        slots[i] = MenuSlot {
            item: Some(
                MenuItem::named("minecraft:player_head", format!("&a{}", f.name)).lore([
                    format!("&7UUID &f{}", f.uuid),
                    "&eClick for details".into(),
                ]),
            ),
            action: MenuClickAction::FriendDetail(f.uuid),
        };
    }
    if friends.is_empty() {
        slots[22] = MenuSlot {
            item: Some(
                MenuItem::named("minecraft:paper", "&7No friends yet")
                    .lore(["&8Click emerald to add online players".into()]),
            ),
            action: MenuClickAction::FriendAddMenu,
        };
    }
    slots[45] = MenuSlot {
        item: Some(
            MenuItem::named("minecraft:emerald", "&aAdd friend")
                .lore(["&7Pick an online player".into()]),
        ),
        action: MenuClickAction::FriendAddMenu,
    };
    let pending = shared.requests_for(viewer).len();
    slots[49] = MenuSlot {
        item: Some(
            MenuItem::named(
                if pending > 0 {
                    "minecraft:writable_book"
                } else {
                    "minecraft:book"
                },
                format!(
                    "{}Requests: {pending}",
                    if pending > 0 { "&e" } else { "&7" }
                ),
            )
            .lore([if pending > 0 {
                "&7Click to answer".into()
            } else {
                "&8Nobody waiting".into()
            }]),
        ),
        action: MenuClickAction::FriendRequests,
    };
    slots[53] = MenuSlot {
        item: Some(MenuItem::named("minecraft:barrier", "&cClose")),
        action: MenuClickAction::Close,
    };
    sink(HostAction::OpenMenu {
        uuid: viewer,
        rows: 6,
        title: "&8Friends".into(),
        slots,
    });
}

fn open_add(shared: &Shared, sink: &(dyn Fn(HostAction) + Send + Sync), viewer: Uuid) {
    let friends = shared.friends_of(viewer);
    let online = FRIENDS_ONLINE.lock().clone();
    let mut slots = vec![empty_slot(); 54];
    let mut slot = 0usize;
    for p in online.iter().filter(|p| p.uuid != viewer) {
        if slot >= 45 {
            break;
        }
        let already = friends.iter().any(|f| f.uuid == p.uuid);
        let pending = shared
            .requests_for(p.uuid)
            .iter()
            .any(|r| r.uuid == viewer);
        slots[slot] = MenuSlot {
            item: Some(
                MenuItem::named(
                    "minecraft:player_head",
                    if already {
                        format!("&a{} &7(friend)", p.name)
                    } else if pending {
                        format!("&e{} &7(pending)", p.name)
                    } else {
                        format!("&f{}", p.name)
                    },
                )
                .lore([
                    format!("&7UUID &f{}", p.uuid),
                    if already || pending {
                        "&8Already handled".into()
                    } else {
                        "&aClick to send request".into()
                    },
                ]),
            ),
            action: if already || pending {
                MenuClickAction::None
            } else {
                MenuClickAction::FriendAdd(p.uuid)
            },
        };
        slot += 1;
    }
    if slot == 0 {
        slots[22] = MenuSlot {
            item: Some(
                MenuItem::named("minecraft:paper", "&7No other players online")
                    .lore(["&8/friend add <name> when they join".into()]),
            ),
            action: MenuClickAction::None,
        };
    }
    slots[49] = MenuSlot {
        item: Some(MenuItem::named("minecraft:arrow", "&7Back")),
        action: MenuClickAction::FriendsList,
    };
    sink(HostAction::OpenMenu {
        uuid: viewer,
        rows: 6,
        title: "&8Add friend".into(),
        slots,
    });
}

fn open_detail(
    shared: &Shared,
    sink: &(dyn Fn(HostAction) + Send + Sync),
    viewer: Uuid,
    friend: &Friendship,
) {
    let _ = (shared, viewer);
    let mut slots = vec![MenuSlot {
        item: None,
        action: MenuClickAction::None,
    }; 27];
    slots[4] = MenuSlot {
        item: Some(
            MenuItem::named("minecraft:player_head", format!("&a{}", friend.name))
                .lore([format!("&7UUID &f{}", friend.uuid)]),
        ),
        action: MenuClickAction::None,
    };
    slots[15] = MenuSlot {
        item: Some(
            MenuItem::named("minecraft:redstone_block", "&cRemove friend")
                .lore(["&7Removes them for both of you".to_string()]),
        ),
        action: MenuClickAction::FriendRemove(friend.uuid),
    };
    slots[22] = MenuSlot {
        item: Some(MenuItem::named("minecraft:arrow", "&7Back")),
        action: MenuClickAction::FriendsList,
    };
    sink(HostAction::OpenMenu {
        uuid: viewer,
        rows: 3,
        title: format!("&8Friend: {}", friend.name),
        slots,
    });
}

fn open_requests(shared: &Shared, sink: &(dyn Fn(HostAction) + Send + Sync), viewer: Uuid) {
    let reqs = shared.requests_for(viewer);
    let mut slots = vec![MenuSlot {
        item: None,
        action: MenuClickAction::None,
    }; 27];
    for (i, r) in reqs.iter().take(18).enumerate() {
        slots[i] = MenuSlot {
            item: Some(
                MenuItem::named("minecraft:player_head", format!("&e{}", r.name)).lore([
                    "&aLeft-click: accept (use /friend accept)".to_string(),
                    "&cOr click: accept now".to_string(),
                ]),
            ),
            action: MenuClickAction::FriendAccept(r.uuid),
        };
    }
    if reqs.is_empty() {
        slots[13] = MenuSlot {
            item: Some(MenuItem::named("minecraft:paper", "&7No requests waiting")),
            action: MenuClickAction::None,
        };
    }
    slots[22] = MenuSlot {
        item: Some(MenuItem::named("minecraft:arrow", "&7Back")),
        action: MenuClickAction::FriendsList,
    };
    sink(HostAction::OpenMenu {
        uuid: viewer,
        rows: 3,
        title: "&8Friend requests".into(),
        slots,
    });
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
