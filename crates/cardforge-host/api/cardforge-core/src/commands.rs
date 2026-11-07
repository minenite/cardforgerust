//! Command map and built-in CardForge commands.

use cardforge_api::{
    colorize, Command, CommandSender, PermissibleId, PermissionService, PluginDescription,
};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Bukkit-style `&` formatting applied to every command reply line.
fn reply(line: impl Into<String>) -> String {
    colorize(&line.into())
}

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
    /// Also emits a bare `plugin` / `cardforge` root so tab-complete works with no args.
    pub fn native_usages(&self) -> Vec<(String, u8)> {
        let map = self.by_label.read();
        let order = self.order.read();
        let mut out = Vec::new();
        for name in order.iter() {
            let Some(cmd) = map.get(name) else {
                continue;
            };
            out.push((cmd.usage.clone(), cmd.permission_level));
            if cmd.name == "plugin" {
                out.push(("plugin".into(), cmd.permission_level));
            }
            if cmd.name == "cardforge" {
                out.push(("cardforge".into(), cmd.permission_level));
            }
        }
        out
    }

    /// Dispatch `raw` (no leading slash). Returns `Some(replies)` if handled.
    pub fn dispatch(&self, sender: &CommandSender, raw: &str) -> Option<Vec<String>> {
        self.dispatch_with_perms(sender, raw, None)
    }

    /// Like [`Self::dispatch`], also checking Bukkit-style permission nodes when `perms` is set.
    pub fn dispatch_with_perms(
        &self,
        sender: &CommandSender,
        raw: &str,
        perms: Option<&PermissionService>,
    ) -> Option<Vec<String>> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }
        let mut parts = raw.split_whitespace();
        let label = parts.next()?.to_ascii_lowercase();
        let args: Vec<&str> = parts.collect();
        let cmd = self.get(&label)?;
        if sender.permission_level() < cmd.permission_level {
            return Some(vec![reply(format!(
                "&cYou do not have permission to use /{}.",
                cmd.name
            ))]);
        }
        if let (Some(perms), Some(node)) = (perms, cmd.permission.as_deref()) {
            let who = match sender {
                CommandSender::Console => PermissibleId::Console,
                CommandSender::Player(p) => PermissibleId::Player(p.uuid),
            };
            if !perms.has(who, node) {
                return Some(vec![reply(format!(
                    "&cYou lack permission &f{node}&c to use /{}.",
                    cmd.name
                ))]);
            }
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
        .aliases(["pl"])
        .permission("cardforge.command.plugins"),
    );

    let plugins_one = Arc::clone(&plugins);
    map.register(
        Command::new(
            "plugin",
            "plugin <name:word>",
            "List plugins, or show one",
            "cardforge",
            move |_sender, _label, args| {
                if args.is_empty() {
                    return format_plugins(&plugins_one());
                }
                let want = args[0].to_ascii_lowercase();
                match plugins_one().into_iter().find(|p| {
                    p.description.id.eq_ignore_ascii_case(&want)
                        || p.description.name.eq_ignore_ascii_case(&want)
                }) {
                    Some(p) => describe_plugin(&p),
                    None => vec![
                        reply("&cThis server is not running any plugin by that name."),
                        reply("&7Use /plugins to get a list of plugins."),
                    ],
                }
            },
        ),
    );

    let plugins_ver = Arc::clone(&plugins);
    // Not registered as `version` in the Horizon tree (vanilla owns /version);
    // player_command still intercepts /version /ver /about before vanilla.
    map.register(
        Command::new(
            "version",
            "cfversion",
            "Show CardForge and game version",
            "cardforge",
            move |_sender, _label, _args| {
                version_lines(api_version, game_version, &plugins_ver())
            },
        )
        .aliases(["ver", "about", "cfversion"]),
    );

    let plugins_cf = Arc::clone(&plugins);
    map.register(
        Command::new(
            "cardforge",
            "cardforge <action:word>",
            "CardForge control command",
            "cardforge",
            move |_sender, _label, args| {
                let sub = args.first().copied().unwrap_or("help").to_ascii_lowercase();
                match sub.as_str() {
                    "version" | "ver" => version_lines(api_version, game_version, &plugins_cf()),
                    "plugins" | "pl" | "list" => format_plugins(&plugins_cf()),
                    _ => vec![
                        reply("&6CardForge &7commands:"),
                        reply("&e  /cardforge &7<&fhelp&7|&fversion&7|&fplugins&7>"),
                        reply("&e  /cfversion  &8(also /ver /about /version)"),
                        reply("&e  /plugins  &8(alias: /pl)"),
                        reply("&e  /plugin &7<&fname&7>"),
                        reply("&e  /cf  &8(alias of /cardforge)"),
                    ],
                }
            },
        )
        .aliases(["cf"]),
    );
}

fn format_plugins(plugins: &[PluginInfo]) -> Vec<String> {
    if plugins.is_empty() {
        return vec![reply("&fPlugins (0): &7none")];
    }
    // Bukkit: green = enabled, red + * = disabled; comma separators reset to white.
    let names: Vec<String> = plugins
        .iter()
        .map(|p| {
            if p.enabled {
                format!("&a{}", p.description.name)
            } else {
                format!("&c{}*", p.description.name)
            }
        })
        .collect();
    vec![reply(format!(
        "&fPlugins ({}): {}",
        plugins.len(),
        names.join("&f, ")
    ))]
}

fn describe_plugin(p: &PluginInfo) -> Vec<String> {
    let d = &p.description;
    let mut lines = vec![reply(format!(
        "&a{} &fversion &a{}",
        d.name, d.version
    ))];
    let state = if p.enabled {
        "&aenabled"
    } else {
        "&cdisabled"
    };
    lines.push(reply(format!("&7id: &f{} &7({state}&7)", d.id)));
    if !d.description.is_empty() {
        lines.push(reply(format!("&7{}", d.description)));
    }
    if !d.authors.is_empty() {
        let label = if d.authors.len() == 1 {
            "Author"
        } else {
            "Authors"
        };
        let authors = d
            .authors
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let sep = if i == 0 {
                    String::new()
                } else if i + 1 == d.authors.len() {
                    "&f and ".to_string()
                } else {
                    "&f, ".to_string()
                };
                format!("{sep}&a{a}")
            })
            .collect::<String>();
        lines.push(reply(format!("&7{label}: {authors}")));
    }
    lines
}

fn version_lines(api: &str, game: &str, plugins: &[PluginInfo]) -> Vec<String> {
    let enabled = plugins.iter().filter(|p| p.enabled).count();
    vec![
        reply(format!(
            "This server is running &6CardForge&r version {api}&o (Implementing API version {game})"
        )),
        reply(format!("&7Plugins: &f{enabled} &7loaded")),
    ]
}
