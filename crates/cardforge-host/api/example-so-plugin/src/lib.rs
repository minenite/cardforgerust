//! Example dynamic CardForge plugin — build as cdylib and drop into `plugins/`.

use cardforge_api::{
    declare_plugin, colorize, Command, EventPriority, Plugin, PluginContext, PluginDescription,
    PluginResult, PluginServices, Server, Cancellable,
};

/// Greeter that also cancels chat containing `!example`.
#[derive(Default)]
pub struct ExampleSoPlugin;

impl Plugin for ExampleSoPlugin {
    fn description(&self) -> PluginDescription {
        let mut d = PluginDescription::new("example_so", "Example SO", "0.1.0");
        d.authors = vec!["icedmoca".into()];
        d.description = "Smoke-test dynamic .so loader.".into();
        d
    }

    fn on_load(&mut self, services: &dyn PluginServices) -> PluginResult {
        services.listen_player_join(
            "example_so",
            EventPriority::Lowest,
            Box::new(|event| {
                log::info!("example_so: {} joined", event.player.name);
            }),
        );
        services.listen_player_chat(
            "example_so",
            EventPriority::Highest,
            Box::new(|event| {
                if event.message.to_ascii_lowercase().contains("!example") {
                    event.set_cancelled(true);
                    event
                        .replies
                        .push(colorize("&d[example_so] &7blocked &f!example"));
                }
            }),
        );
        services.register_command(
            Command::new(
                "exampleso",
                "exampleso",
                "Ping the example .so plugin",
                "example_so",
                |_s, _l, _a| vec![colorize("&dExample SO plugin is loaded.")],
            )
            .aliases(["exso"]),
        );
        Ok(())
    }

    fn on_enable(&mut self, ctx: &PluginContext, _server: &Server) -> PluginResult {
        log::info!("example_so enabled at {}", ctx.data_folder.display());
        Ok(())
    }
}

declare_plugin!(ExampleSoPlugin);
