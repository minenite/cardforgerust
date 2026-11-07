# CardForge (Rust) for Horizon 26.3

<p align="center">
  <strong>Bukkit-inspired plugin host for <a href="https://github.com/horizon-rust/horizon">Horizon</a> — not a Java jar loader.</strong>
</p>

```
Horizon 26.3 (horizon-server / horizon-client)
    -> cardforge-host (native trait mod)
        -> cardforge-core (plugin manager + event bus)
            -> your Rust plugins
```

This is the Horizon successor to [minenite/cardforge](https://github.com/minenite/cardforge)
(NeoForge + Paper API). Same *job* — let network plugins sit on a real game
server — different *machine*: Horizon has no JVM, so plugins are Rust crates
that implement `cardforge_api::Plugin`.

Existing Bukkit/Paper `.jar` plugins **do not** load here. Port them (or rewrite
ServerPlugin-style logic) against `cardforge-api`.

## Status

| Piece | State |
| --- | --- |
| `cardforge-api` | Working — events, `Plugin`, `Server`, `Player` |
| `cardforge-core` | Working — bus + load order + unit tests |
| `hello-plugin` | Working example (join greeting) |
| `cardforge-host` | Draft Horizon trait mod — needs `horizon-launcher native-build` against a Horizon tree |
| Dynamic `.so` plugins | Not yet — plugins are linked / registered in-process |
| Chat / commands / perms | Stubbed next |

Pinned Horizon revision for the host: `00f6358e7a7d4f168450fe6d74cbe82100dcc080` (26.3).

## Workspace

```
crates/cardforge-host/
  horizon-mixin.toml
  api/                 # Horizon trait mod (native-build stages this whole folder)
    cardforge-api/     # plugin-facing API
    cardforge-core/    # event bus + plugin manager
    hello-plugin/      # example join greeter
    src/lib.rs         # Hooks → CardForge
```

Nested under `api/` on purpose: `horizon-launcher native-build` only stages the `[api]` package tree.

```sh
cargo test           # api + core + example
cargo build          # same, without the Horizon host
```

## Writing a plugin

```rust
use cardforge_api::{
    EventPriority, Plugin, PluginDescription, PluginResult, PluginServices,
};

pub struct MyPlugin;

impl Plugin for MyPlugin {
    fn description(&self) -> PluginDescription {
        PluginDescription::new("my_plugin", "My Plugin", "0.1.0")
    }

    fn on_load(&mut self, services: &dyn PluginServices) -> PluginResult {
        services.listen_player_join(
            "my_plugin",
            EventPriority::Normal,
            Box::new(|event| {
                log::info!("{} joined", event.player.name);
            }),
        );
        Ok(())
    }
}
```

Register it from the host (today) the same way `hello-plugin` is registered in
`crates/cardforge-host/api/src/lib.rs`.

## Building the Horizon host

1. Have a Horizon checkout that matches (or update) `horizon_revision` in
   `crates/cardforge-host/horizon-mixin.toml`.
2. Point the host's `horizon-client` dependency at that checkout (path override
   or let `horizon-launcher native-build` rewrite it).
3. Build:

```sh
horizon-launcher native-build crates/cardforge-host --trust-native-code
```

4. Install the resulting `.hnmod` into the server/client instance and enable it.
5. Optional: `CARDFORGE_PLUGINS=/path/to/plugins` overrides the plugins data dir
   (default: `./plugins` under the process cwd).

## Why not “copy CardForge Java”?

CardForge on NeoForge is ~1.7k Java files implementing Paper’s API on Mixin.
Horizon’s mod surface is Rust trait hooks (`docs/native-mods.md`). Re‑hosting
the JVM would fight Horizon’s design. CardForgeRust re‑implements the *plugin
product* on that surface instead.

## License

MIT — see [LICENSE](LICENSE).
