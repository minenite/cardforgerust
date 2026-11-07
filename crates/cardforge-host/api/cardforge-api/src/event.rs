//! Server events plugins can listen for.

use crate::player::Player;

/// How early a listener runs relative to other listeners for the same event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum EventPriority {
    /// Runs before most plugins. Use for monitoring that must see the first state.
    Lowest = 0,
    /// Early, before normal gameplay handlers.
    Low = 1,
    /// Default priority for gameplay plugins.
    #[default]
    Normal = 2,
    /// After normal handlers; good for cleaning up.
    High = 3,
    /// Last chance before the host applies the event.
    Highest = 4,
    /// Observe only — do not cancel or mutate (convention; not enforced).
    Monitor = 5,
}

/// Marker for an event type the bus can dispatch.
pub trait Event: Send + Sync {
    /// Stable name for logs (`player_join`, `server_tick`, …).
    fn name(&self) -> &'static str;
}

/// Events that a listener may cancel.
pub trait Cancellable {
    /// Whether a listener has cancelled the event.
    fn is_cancelled(&self) -> bool;
    /// Cancel (or un-cancel) the event.
    fn set_cancelled(&mut self, cancelled: bool);
}

/// Events that may run off the server thread later. Today every dispatch is
/// still synchronous on the Horizon server thread; the flag is reserved.
pub trait AsyncEvent: Event {
    /// Reserved: whether this event may leave the server thread.
    fn allow_async(&self) -> bool {
        false
    }
}

/// A player finished joining and is listed on the server.
#[derive(Clone, Debug)]
pub struct PlayerJoinEvent {
    /// The player who joined.
    pub player: Player,
    /// Join message shown in chat, if any. `None` suppresses the default line.
    pub join_message: Option<String>,
}

impl Event for PlayerJoinEvent {
    fn name(&self) -> &'static str {
        "player_join"
    }
}

/// A player left the server (disconnect, kick, or stop).
#[derive(Clone, Debug)]
pub struct PlayerQuitEvent {
    /// The player who left.
    pub player: Player,
    /// Quit message shown in chat, if any.
    pub quit_message: Option<String>,
}

impl Event for PlayerQuitEvent {
    fn name(&self) -> &'static str {
        "player_quit"
    }
}

/// A player sent chat (before the server broadcasts it).
///
/// Horizon’s native hook can **cancel** the broadcast and reply to the sender.
/// Signed chat cannot rewrite the body, so [`Self::message`] / [`Self::format`]
/// mutations are visible to later listeners but do not change what other players
/// see unless you cancel and handle delivery yourself.
#[derive(Clone, Debug)]
pub struct PlayerChatEvent {
    /// The speaker.
    pub player: Player,
    /// Message text without the player's name prefix.
    pub message: String,
    /// Format string with `{player}` and `{message}` placeholders.
    pub format: String,
    /// Lines sent only to the speaker (legacy `&` codes OK).
    pub replies: Vec<String>,
    cancelled: bool,
}

impl PlayerChatEvent {
    /// Build a chat event with the usual `"<{player}> {message}"` format.
    pub fn new(player: Player, message: impl Into<String>) -> Self {
        Self {
            player,
            message: message.into(),
            format: "<{player}> {message}".to_string(),
            replies: Vec::new(),
            cancelled: false,
        }
    }

    /// Render [`Self::format`] with the current player name and message.
    pub fn rendered(&self) -> String {
        self.format
            .replace("{player}", &self.player.name)
            .replace("{message}", &self.message)
    }
}

impl Event for PlayerChatEvent {
    fn name(&self) -> &'static str {
        "player_chat"
    }
}

impl Cancellable for PlayerChatEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// A player respawned after death (not End exit).
#[derive(Clone, Debug)]
pub struct PlayerRespawnEvent {
    /// The player who respawned.
    pub player: Player,
}

impl Event for PlayerRespawnEvent {
    fn name(&self) -> &'static str {
        "player_respawn"
    }
}

/// One non-paused server tick (start or end of the tick).
///
/// Named `GameTickEvent` so it does not clash with Horizon's
/// `mod_api::TickEvent` / `ServerTickEvent` inside the host crate.
#[derive(Clone, Debug)]
pub struct GameTickEvent {
    /// Tick counter since the world opened.
    pub tick: u64,
    /// `true` at the start of the tick, `false` at the end.
    pub pre: bool,
}

impl Event for GameTickEvent {
    fn name(&self) -> &'static str {
        "server_tick"
    }
}

