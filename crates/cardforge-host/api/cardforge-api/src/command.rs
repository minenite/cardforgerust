//! Commands plugins (and CardForge itself) can register.

use crate::player::Player;
use std::sync::Arc;

/// Who ran a command.
#[derive(Clone, Debug)]
pub enum CommandSender {
    /// An online player.
    Player(Player),
    /// The dedicated server console (permission level 4).
    Console,
}

impl CommandSender {
    /// Display name for logs and replies.
    pub fn name(&self) -> &str {
        match self {
            Self::Player(p) => p.name.as_str(),
            Self::Console => "CONSOLE",
        }
    }

    /// Operator level 0..=4 (console is 4).
    pub fn permission_level(&self) -> u8 {
        match self {
            Self::Player(p) => p.permission_level,
            Self::Console => 4,
        }
    }

    /// Whether this is the console.
    pub fn is_console(&self) -> bool {
        matches!(self, Self::Console)
    }
}

/// One registered command.
pub struct Command {
    /// Primary label (`plugins`, `version`, …) without a slash.
    pub name: String,
    /// Alternate labels.
    pub aliases: Vec<String>,
    /// Usage string for tab-complete / help (`plugins`, `plugin <name:word>`).
    pub usage: String,
    /// Short description.
    pub description: String,
    /// Minimum operator level (0 = everyone).
    pub permission_level: u8,
    /// Optional Bukkit-style permission node (checked when set).
    pub permission: Option<String>,
    /// Owning plugin id (`cardforge` for built-ins).
    pub plugin: String,
    /// Handler: return reply lines (no slash formatting).
    pub handler: Arc<dyn Fn(&CommandSender, &str, &[&str]) -> Vec<String> + Send + Sync>,
}

impl Command {
    /// Build a command. `handler` receives (sender, label_used, args).
    pub fn new(
        name: impl Into<String>,
        usage: impl Into<String>,
        description: impl Into<String>,
        plugin: impl Into<String>,
        handler: impl Fn(&CommandSender, &str, &[&str]) -> Vec<String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            aliases: Vec::new(),
            usage: usage.into(),
            description: description.into(),
            permission_level: 0,
            permission: None,
            plugin: plugin.into(),
            handler: Arc::new(handler),
        }
    }

    /// Add aliases.
    pub fn aliases(mut self, aliases: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.aliases.extend(aliases.into_iter().map(Into::into));
        self
    }

    /// Require this operator level.
    pub fn level(mut self, level: u8) -> Self {
        self.permission_level = level.min(4);
        self
    }

    /// Require this permission node (in addition to op level).
    pub fn permission(mut self, node: impl Into<String>) -> Self {
        self.permission = Some(node.into());
        self
    }
}
