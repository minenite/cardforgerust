//! Example CardForge plugin: greets each player on join.

use cardforge_api::{
    EventPriority, Plugin, PluginContext, PluginDescription, PluginResult, PluginServices, Server,
};

/// Greeter plugin.
pub struct HelloPlugin;

impl Plugin for HelloPlugin {
    fn description(&self) -> PluginDescription {
        let mut d = PluginDescription::new("hello", "Hello", "0.1.0");
        d.authors = vec!["icedmoca".into()];
        d.description = "Greets players when they join a CardForge Horizon server.".into();
        d
    }

    fn on_load(&mut self, services: &dyn PluginServices) -> PluginResult {
        services.listen_player_join(
            "hello",
            EventPriority::Normal,
            Box::new(|event| {
                log::info!(
                    "hello-plugin: {} joined (uuid {})",
                    event.player.name,
                    event.player.uuid
                );
                event.join_message =
                    Some(format!("Welcome to CardForge, {}!", event.player.name));
            }),
        );
        Ok(())
    }

    fn on_enable(&mut self, ctx: &PluginContext, _server: &Server) -> PluginResult {
        log::info!(
            "hello-plugin enabled; data folder {}",
            ctx.data_folder.display()
        );
        Ok(())
    }
}

/// Construct the plugin for hosts that register built-ins by hand.
pub fn plugin() -> Box<dyn Plugin> {
    Box::new(HelloPlugin)
}
