//! Server handle plugins receive.

use crate::player::Player;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Immutable snapshot of who is online and basic server identity.
#[derive(Clone, Debug, Default)]
pub struct ServerSnapshot {
    /// MOTD / list name.
    pub motd: String,
    /// Max players from server config.
    pub max_players: u32,
    /// Players currently online.
    pub players: Vec<Player>,
    /// Horizon / CardForge game version string (`26.3`).
    pub game_version: String,
}

/// Shared server handle. Cheap to clone; points at live host state.
#[derive(Clone, Debug)]
pub struct Server {
    inner: Arc<RwLock<ServerSnapshot>>,
}

impl Server {
    /// Create a server handle wrapping a snapshot store.
    pub fn new(snapshot: ServerSnapshot) -> Self {
        Self {
            inner: Arc::new(RwLock::new(snapshot)),
        }
    }

    /// Replace the online-player snapshot (host calls this each join/quit/tick).
    pub fn replace_snapshot(&self, snapshot: ServerSnapshot) {
        *self.inner.write().expect("server snapshot lock") = snapshot;
    }

    /// Read a clone of the current snapshot.
    pub fn snapshot(&self) -> ServerSnapshot {
        self.inner.read().expect("server snapshot lock").clone()
    }

    /// Look up an online player by UUID.
    pub fn player(&self, uuid: Uuid) -> Option<Player> {
        self.inner
            .read()
            .expect("server snapshot lock")
            .players
            .iter()
            .find(|p| p.uuid == uuid)
            .cloned()
    }

    /// Look up an online player by exact name (case-sensitive for now).
    pub fn player_by_name(&self, name: &str) -> Option<Player> {
        self.inner
            .read()
            .expect("server snapshot lock")
            .players
            .iter()
            .find(|p| p.name == name)
            .cloned()
    }

    /// How many players are online.
    pub fn online_count(&self) -> usize {
        self.inner
            .read()
            .expect("server snapshot lock")
            .players
            .len()
    }
}
