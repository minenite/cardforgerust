//! Bukkit-style permission nodes (plus a tiny Vault-shaped lookup).

use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Who is asking — player UUID or the console.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PermissibleId {
    /// Online / offline player key.
    Player(Uuid),
    /// Dedicated console (always allowed).
    Console,
}

/// In-memory permission table: explicit true/false nodes, op flag, and defaults.
#[derive(Default)]
pub struct PermissionService {
    inner: RwLock<PermInner>,
}

#[derive(Default)]
struct PermInner {
    /// uuid → node → allowed
    players: HashMap<Uuid, HashMap<String, bool>>,
    /// Ops implicitly pass every check (like Bukkit op).
    ops: HashSet<Uuid>,
    /// Default grants for everyone (e.g. `cardforge.command.plugins` = true).
    defaults: HashMap<String, bool>,
}

impl PermissionService {
    /// Empty table (console still passes everything).
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark / unmark a player as op for permission checks.
    pub fn set_op(&self, player: Uuid, op: bool) {
        let mut g = self.inner.write();
        if op {
            g.ops.insert(player);
        } else {
            g.ops.remove(&player);
        }
    }

    /// Whether this player is treated as op by CardForge perms.
    pub fn is_op(&self, player: Uuid) -> bool {
        self.inner.read().ops.contains(&player)
    }

    /// Set a default for players with no explicit node.
    pub fn set_default(&self, node: impl Into<String>, value: bool) {
        self.inner.write().defaults.insert(normalize(&node.into()), value);
    }

    /// Grant or deny a node on a player (overrides defaults).
    pub fn set(&self, player: Uuid, node: impl Into<String>, value: bool) {
        let node = normalize(&node.into());
        self.inner
            .write()
            .players
            .entry(player)
            .or_default()
            .insert(node, value);
    }

    /// Remove an explicit node so defaults apply again.
    pub fn unset(&self, player: Uuid, node: &str) {
        let node = normalize(node);
        if let Some(map) = self.inner.write().players.get_mut(&player) {
            map.remove(&node);
        }
    }

    /// Vault-shaped lookup: console always true; ops always true; else walk nodes + `*` parents.
    pub fn has(&self, who: PermissibleId, node: &str) -> bool {
        match who {
            PermissibleId::Console => true,
            PermissibleId::Player(uuid) => self.player_has(uuid, node),
        }
    }

    fn player_has(&self, player: Uuid, node: &str) -> bool {
        let node = normalize(node);
        let g = self.inner.read();
        if g.ops.contains(&player) {
            return true;
        }
        if let Some(map) = g.players.get(&player) {
            if let Some(&v) = map.get(&node) {
                return v;
            }
            // Wildcard parents: `cardforge.*` covers `cardforge.command.plugins`.
            for (k, &v) in map {
                if k == "*" {
                    return v;
                }
                if let Some(stem) = k.strip_suffix(".*") {
                    if node == stem || node.starts_with(&format!("{stem}.")) {
                        return v;
                    }
                }
            }
        }
        if let Some(&v) = g.defaults.get(&node) {
            return v;
        }
        false
    }
}

fn normalize(node: &str) -> String {
    node.trim().to_ascii_lowercase()
}

/// Tiny Vault-like façade plugins can depend on.
pub trait PermissionProvider: Send + Sync {
    /// Whether `player` has `node`.
    fn player_has(&self, player: Uuid, node: &str) -> bool;
}

impl PermissionProvider for PermissionService {
    fn player_has(&self, player: Uuid, node: &str) -> bool {
        self.has(PermissibleId::Player(player), node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_and_ops() {
        let p = PermissionService::new();
        let u = Uuid::nil();
        assert!(!p.has(PermissibleId::Player(u), "foo.bar"));
        p.set(u, "foo.*", true);
        assert!(p.has(PermissibleId::Player(u), "foo.bar"));
        p.set_op(u, true);
        assert!(p.has(PermissibleId::Player(u), "anything"));
        assert!(p.has(PermissibleId::Console, "anything"));
    }
}
