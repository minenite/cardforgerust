//! Online player view.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A player currently known to the server.
///
/// Fields mirror what Horizon exposes on join (`native_player_logged_in` /
/// `native_server_players`): identity, dimension, and pose. Richer inventory
/// and permission APIs land as the host grows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Player {
    /// Mojang UUID (online mode) or offline UUID.
    pub uuid: Uuid,
    /// Current display / login name.
    pub name: String,
    /// Dimension id, e.g. `minecraft:overworld`.
    pub dimension: String,
    /// Feet position in blocks.
    pub position: [f64; 3],
    /// Yaw (degrees).
    pub yaw: f32,
    /// Pitch (degrees).
    pub pitch: f32,
    /// Health points (vanilla max 20).
    pub health: f32,
    /// Game mode: 0 survival, 1 creative, 2 adventure, 3 spectator.
    pub game_mode: u8,
    /// Operator permission level 0..=4 when known; 0 if unknown.
    pub permission_level: u8,
}

impl Player {
    /// Build a minimal player from uuid + name (join/quit paths).
    pub fn named(uuid: Uuid, name: impl Into<String>) -> Self {
        Self {
            uuid,
            name: name.into(),
            dimension: "minecraft:overworld".to_string(),
            position: [0.0, 0.0, 0.0],
            yaw: 0.0,
            pitch: 0.0,
            health: 20.0,
            game_mode: 0,
            permission_level: 0,
        }
    }
}
