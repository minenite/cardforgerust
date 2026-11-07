//! Warps with real teleport via host actions.

use cardforge_api::{
    colorize, Command, CommandSender, HostAction, Plugin, PluginContext, PluginDescription,
    PluginResult, PluginServices, Server,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Warp {
    world: String,
    x: f64,
    y: f64,
    z: f64,
    yaw: f32,
    pitch: f32,
}

#[derive(Default, Serialize, Deserialize)]
struct WarpsFile {
    warps: HashMap<String, Warp>,
}

struct Shared {
    file: Mutex<WarpsFile>,
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
}

/// Warp registry plugin.
pub struct TravelPlugin {
    shared: Arc<Shared>,
}

impl Default for TravelPlugin {
    fn default() -> Self {
        Self {
            shared: Arc::new(Shared {
                file: Mutex::new(WarpsFile::default()),
                path: Mutex::new(None),
            }),
        }
    }
}

impl Plugin for TravelPlugin {
    fn description(&self) -> PluginDescription {
        let mut d = PluginDescription::new("travel", "Travel", "0.1.0");
        d.authors = vec!["icedmoca".into()];
        d.description = "Named warps with teleport (/warp /setwarp /delwarp).".into();
        d
    }

    fn on_load(&mut self, services: &dyn PluginServices) -> PluginResult {
        services.permissions().set_default("cardforge.travel.warp", true);
        let sink = services.action_sink();
        let shared = Arc::clone(&self.shared);

        let s1 = Arc::clone(&shared);
        let sink1 = Arc::clone(&sink);
        services.register_command(
            Command::new(
                "warp",
                "warp <name:word>",
                "Teleport to a warp",
                "travel",
                move |sender, _label, args| warp_cmd(&s1, sink1.as_ref(), sender, args),
            )
            .aliases(["warps"])
            .permission("cardforge.travel.warp"),
        );

        let s2 = Arc::clone(&shared);
        services.register_command(
            Command::new(
                "setwarp",
                "setwarp <name:word>",
                "Create a warp at your position",
                "travel",
                move |sender, _label, args| setwarp_cmd(&s2, sender, args),
            )
            .level(3)
            .permission("cardforge.travel.setwarp"),
        );

        let s3 = Arc::clone(&shared);
        services.register_command(
            Command::new(
                "delwarp",
                "delwarp <name:word>",
                "Delete a warp",
                "travel",
                move |sender, _label, args| delwarp_cmd(&s3, sender, args),
            )
            .level(3)
            .permission("cardforge.travel.setwarp"),
        );
        Ok(())
    }

    fn on_enable(&mut self, ctx: &PluginContext, _server: &Server) -> PluginResult {
        std::fs::create_dir_all(&ctx.data_folder)?;
        let path = ctx.data_folder.join("warps.json");
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(file) = serde_json::from_slice::<WarpsFile>(&bytes) {
                *self.shared.file.lock() = file;
            }
        }
        *self.shared.path.lock() = Some(path);
        Ok(())
    }
}

fn warp_cmd(
    shared: &Shared,
    sink: &(dyn Fn(HostAction) + Send + Sync),
    sender: &CommandSender,
    args: &[&str],
) -> Vec<String> {
    if args.is_empty() {
        let g = shared.file.lock();
        if g.warps.is_empty() {
            return vec![colorize("&7No warps. &8(/setwarp <name>)")];
        }
        let mut names: Vec<_> = g.warps.keys().cloned().collect();
        names.sort();
        return vec![colorize(&format!(
            "&aWarps &7({}): &f{}",
            names.len(),
            names.join("&7, &f")
        ))];
    }
    let CommandSender::Player(player) = sender else {
        return vec![colorize("&cPlayers only.")];
    };
    let name = args[0].to_ascii_lowercase();
    let g = shared.file.lock();
    let Some(w) = g.warps.get(&name).cloned() else {
        return vec![colorize(&format!("&cNo warp called &f{name}&c."))];
    };
    drop(g);
    sink(HostAction::Teleport {
        uuid: player.uuid,
        dimension: w.world.clone(),
        position: [w.x, w.y, w.z],
        yaw: w.yaw,
        pitch: w.pitch,
    });
    vec![colorize(&format!(
        "&aWarping to &f{name}&a (&7{} {:.1} {:.1} {:.1}&a)",
        w.world, w.x, w.y, w.z
    ))]
}

fn setwarp_cmd(shared: &Shared, sender: &CommandSender, args: &[&str]) -> Vec<String> {
    let CommandSender::Player(player) = sender else {
        return vec![colorize("&cPlayers only.")];
    };
    let Some(name) = args.first().copied() else {
        return vec![colorize("&7Usage: /setwarp <name>")];
    };
    let name = name.to_ascii_lowercase();
    let warp = Warp {
        world: player.dimension.clone(),
        x: player.position[0],
        y: player.position[1],
        z: player.position[2],
        yaw: player.yaw,
        pitch: player.pitch,
    };
    shared.file.lock().warps.insert(name.clone(), warp.clone());
    shared.save();
    vec![colorize(&format!(
        "&aSet warp &f{name}&a at &7{} {:.1} {:.1} {:.1}",
        warp.world, warp.x, warp.y, warp.z
    ))]
}

fn delwarp_cmd(shared: &Shared, _sender: &CommandSender, args: &[&str]) -> Vec<String> {
    let Some(name) = args.first().copied() else {
        return vec![colorize("&7Usage: /delwarp <name>")];
    };
    let name = name.to_ascii_lowercase();
    let removed = shared.file.lock().warps.remove(&name).is_some();
    if removed {
        shared.save();
        vec![colorize(&format!("&7Deleted warp &f{name}&7."))]
    } else {
        vec![colorize(&format!("&cNo warp called &f{name}&c."))]
    }
}

/// Construct for in-process registration.
pub fn plugin() -> Box<dyn Plugin> {
    Box::new(TravelPlugin::default())
}
