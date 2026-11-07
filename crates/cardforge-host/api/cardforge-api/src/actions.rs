//! Requests plugins enqueue for the Horizon host to perform on the next tick.

use uuid::Uuid;

/// A simple menu item description (converted to Horizon `ItemStack` by the host).
#[derive(Clone, Debug)]
pub struct MenuItem {
    /// Item id, e.g. `minecraft:player_head`.
    pub item: String,
    /// Stack count.
    pub count: u32,
    /// Optional custom name (`&` color codes OK).
    pub name: Option<String>,
    /// Lore lines (`&` color codes OK).
    pub lore: Vec<String>,
}

impl MenuItem {
    /// Named stack of one.
    pub fn named(item: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            item: item.into(),
            count: 1,
            name: Some(name.into()),
            lore: Vec::new(),
        }
    }

    /// Add lore lines.
    pub fn lore(mut self, lines: impl IntoIterator<Item = String>) -> Self {
        self.lore.extend(lines);
        self
    }
}

/// What a menu click should do (handled by the host / friends plugin bridge).
#[derive(Clone, Debug)]
pub enum MenuClickAction {
    /// No-op / filler.
    None,
    /// Re-open the friends list.
    FriendsList,
    /// Open detail for this friend uuid.
    FriendDetail(Uuid),
    /// Remove friendship both ways.
    FriendRemove(Uuid),
    /// Accept an incoming request.
    FriendAccept(Uuid),
    /// Deny an incoming request.
    FriendDeny(Uuid),
    /// Open the requests screen.
    FriendRequests,
    /// Send a friend request to this online player (by uuid).
    FriendAdd(Uuid),
    /// Open the “add online” picker.
    FriendAddMenu,
    /// Close the open menu.
    Close,
}

/// One menu slot: item + optional click action.
#[derive(Clone, Debug)]
pub struct MenuSlot {
    /// Display item.
    pub item: Option<MenuItem>,
    /// Click binding.
    pub action: MenuClickAction,
}

/// Work for the Horizon `Host` to do (teleport, chat, menus, console).
#[derive(Clone, Debug)]
pub enum HostAction {
    /// Teleport a player (dimension + pose).
    Teleport {
        /// Player uuid.
        uuid: Uuid,
        /// Dimension id.
        dimension: String,
        /// Feet position.
        position: [f64; 3],
        /// Yaw degrees.
        yaw: f32,
        /// Pitch degrees.
        pitch: f32,
    },
    /// Broadcast a colored chat/system line to everyone.
    Broadcast {
        /// Legacy `&` / plain line.
        line: String,
    },
    /// Open a chest-style menu with click bindings.
    OpenMenu {
        /// Viewer.
        uuid: Uuid,
        /// Rows 1..=6.
        rows: u8,
        /// Title (`&` OK).
        title: String,
        /// Slots in order (length may be < rows*9).
        slots: Vec<MenuSlot>,
    },
    /// Close the player's native menu.
    CloseMenu {
        /// Player.
        uuid: Uuid,
    },
    /// Run a console command (no leading slash).
    RunCommand {
        /// Command line.
        line: String,
    },
}

/// Online player snapshot for plugins (name resolve, GUIs).
#[derive(Clone, Debug)]
pub struct OnlinePlayer {
    /// Uuid.
    pub uuid: Uuid,
    /// Name.
    pub name: String,
    /// Dimension.
    pub dimension: String,
    /// Position.
    pub position: [f64; 3],
    /// Yaw.
    pub yaw: f32,
    /// Pitch.
    pub pitch: f32,
    /// Op level 0..=4.
    pub permission_level: u8,
}
