//! Command map and built-in CardForge commands.

use cardforge_api::{Command, CommandSender, PluginDescription};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Registry of labels → commands.
#[derive(Default)]
pub struct CommandMap {
    by_label: RwLock<HashMap<String, Arc<Command>>>,
    /// Primary names in registration order (for /cardforge help).
    order: RwLock<Vec<String>>,
}

impl CommandMap {
    /// Empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a command and its aliases. Later registrations win on conflict.
    pub fn register(&self, command: Command) {
        let command = Arc::new(command);
        let mut map = self.by_label.write();
        let mut order = self.order.write();
        let primary = command.name.to_ascii_lowercase();
        if !order.iter().any(|n| n == &primary) {
            order.push(primary.clone());
        }
        map.insert(primary, Arc::clone(&command));
        for alias in &command.aliases {
            map.insert(alias.to_ascii_lowercase(), Arc::clone(&command));
        }
    }

    /// Look up by label (case-insensitive).
    pub fn get(&self, label: &str) -> Option<Arc<Command>> {
        self.by_label.read().get(&label.to_ascii_lowercase()).cloned()
    }

    /// All primary command names in order.
    pub fn primary_names(&self) -> Vec<String> {
        self.order.read().clone()
    }

    /// Usages for Horizon `register_commands` (one entry per command, not per alias).
    pub fn native_usages(&self) -> Vec<(String, u8)> {
        let map = self.by_label.read();
        let order = self.order.read();
        order
            .iter()
            .filter_map(|name| {
                let cmd = map.get(name)?;
                Some((cmd.usage.clone(), cmd.permission_level))
            })
            .collect()
    }

    /// Dispatch `raw` (no leading slash). Returns `Some(replies)` if handled.
    pub fn dispatch(&self, sender: &CommandSender, raw: &str) -> Option<Vec<String>> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }
        let mut parts = raw.split_whitespace();
        let label = parts.next()?.to_ascii_lowercase();
        let args: Vec<&str> = parts.collect();
        let cmd = self.get(&label)?;
        if sender.permission_level() < cmd.permission_level {
            return Some(vec![format!(
                "You do not have permission to use /{}.",
                cmd.name
            )]);
        }
        Some((cmd.handler)(sender, &label, &args))
    }
}

/// Snapshot of a loaded plugin for /plugins and /plugin.
#[derive(Clone, Debug)]
pub struct PluginInfo {
    /// Manifest.
    pub description: PluginDescription,
    /// Whether currently enabled.
    pub enabled: bool,
}

/// Install Bukkit-style built-ins. `plugins` is consulted live via the closure.
pub fn register_builtins(
    map: &CommandMap,
    plugins: Arc<dyn Fn() -> Vec<PluginInfo> + Send + Sync>,
    api_version: &'static str,
    game_version: &'static str,
) {
    let plugins_list = Arc::clone(&plugins);
    map.register(
        Command::new(
            "plugins",
            "plugins",
            "List loaded CardForge plugins",
            "cardforge",
            move |_sender, _label, _args| format_plugins(&plugins_list()),
        )
        .aliases(["pl"]),
    );

    let plugins_one = Arc::clone(&plugins);
    map.register(
        Command::new(
            "plugin",
            "plugin [<name:word>]",
            "List plugins, or show one",
            "cardforge",
            move |_sender, _label, args| {
                if args.is_empty() {
                    return format_plugins(&plugins_one());
                }
                let want = args[0].to_ascii_lowercase();
                match plugins_one()
                    .into_iter()
                    .find(|p| p.description.id.eq_ignore_ascii_case(&want) || p.description.name.eq_ignore_ascii_case(&want))
                {
                    Some(p) => {
                        let d = &p.description;
                        let state = if p.enabled { "enabled" } else { "disabled" };
                        vec![
                            format!("{} v{} ({state})", d.name, d.version),
                            format!("id: {}", d.id),
                            if d.description.is_empty() {
                                String::new()
                            } else {
                                d.description.clone()
                            },
                            if d.authors.is_empty() {
                                String::new()
                            } else {
                                format!("authors: {}", d.authors.join(", "))
                            },
                        ]
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .collect()
                    }
                    None => vec![format!("Unknown plugin `{want}`.")],
                }
            },
        ),
    );

    let plugins_ver = Arc::clone(&plugins);
    map.register(
        Command::new(
            "version",
            "version",
            "Show CardForge and game version",
            "cardforge",
            move |_sender, _label, _args| {
                version_lines(api_version, game_version, &plugins_ver())
            },
        )
        .aliases(["ver", "about"]),
    );

    let plugins_cf = Arc::clone(&plugins);
    let map_help = map.primary_names(); // snapshot empty now — help reads live via map below
    let _ = map_help;
    // Capture CommandMap pointer for help — register after we have map methods via Arc
    // We'll pass help through a second register that lists known built-in text.
    map.register(
        Command::new(
            "cardforge",
            "cardforge [help|version|plugins]",
            "CardForge control command",
            "cardforge",
            move |_sender, _label, args| {
                let sub = args.first().copied().unwrap_or("help").to_ascii_lowercase();
                match sub.as_str() {
                    "version" | "ver" => version_lines(api_version, game_version, &plugins_cf()),
                    "plugins" | "pl" | "list" => format_plugins(&plugins_cf()),
                    "help" | _ => vec![
                        "CardForge commands:".into(),
                        "  /cardforge [help|version|plugins]".into(),
                        "  /version  (aliases: /ver /about)".into(),
                        "  /plugins  (alias: /pl)".into(),
                        "  /plugin [name]".into(),
                        "  /cf  (alias of /cardforge)".into(),
                    ],
                }
            },
        )
        .aliases(["cf"]),
    );
}

fn format_plugins(plugins: &[PluginInfo]) -> Vec<String> {
    if plugins.is_empty() {
        return vec!["Plugins (0): none".into()];
    }
    let names: Vec<String> = plugins
        .iter()
        .map(|p| {
            if p.enabled {
                p.description.name.clone()
            } else {
                format!("{}*", p.description.name)
            }
        })
        .collect();
    vec![format!(
        "Plugins ({}): {}",
        plugins.len(),
        names.join(", ")
    )]
}

fn version_lines(api: &str, game: &str, plugins: &[PluginInfo]) -> Vec<String> {
    vec![
        format!("This server is running CardForge {api} for Horizon / Minecraft {game}"),
        format!(
            "Plugins: {} loaded",
            plugins.iter().filter(|p| p.enabled).count()
        ),
    ]
}
