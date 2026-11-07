# CardForge (Rust) for Horizon 26.3

<p align="center">
  <strong>Bukkit-inspired plugin host for <a href="https://github.com/horizon-rust/horizon">Horizon</a> — not a Java jar loader.</strong>
</p>

```
Horizon 26.3 (horizon-server / horizon-client)
    -> cardforge-host (native trait mod)
        -> cardforge-core (plugin manager + event bus + .so loader)
            -> hello / friends / ranks / travel (+ drop-in *.so)
```

This is the Horizon successor to [minenite/cardforge](https://github.com/minenite/cardforge)
(NeoForge + Paper API). Same *job* — let network plugins sit on a real game
server — different *machine*: Horizon has no JVM, so plugins are Rust crates
that implement `cardforge_api::Plugin`.

Existing Bukkit/Paper `.jar` plugins **do not** load here. Port them (or rewrite
ServerPlugin-style logic) against `cardforge-api`.

Web client (separate): [minenite/Astris](https://github.com/minenite/Astris).

## Status

| Piece | State |
| --- | --- |
| `cardforge-api` | Working — events, Plugin, Server, colorize, permissions |
| `cardforge-core` | Working — bus, commands, `.so` loader, unit tests |
| `hello-plugin` | Working — join greeting + `!cancel` chat demo |
| `cf-friends` / `cf-ranks` / `cf-travel` | Working — ServerPlugin subset ports |
| `cardforge-host` | Working — native-build against Horizon nightly; dedicated smoke |
| Dynamic `.so` plugins | Working — `declare_plugin!` + `plugins/*.so` |
| Chat cancel | Working — Horizon signed chat cannot rewrite body |
| Perms | Working — nodes + wildcards + op; Vault-shaped provider |

Pinned Horizon revision for the host: see `crates/cardforge-host/horizon-mixin.toml`
(currently nightly `66f3981…` via `horizon-client` git dep).

## Workspace

```
crates/cardforge-host/
  horizon-mixin.toml
  api/                 # Horizon trait mod (native-build stages this whole folder)
    cardforge-api/
    cardforge-core/
    hello-plugin/
    cf-friends/
    cf-ranks/
    cf-travel/
    src/lib.rs         # Hooks → CardForge
```

```sh
cargo test           # api + core + plugins
```

## Dynamic plugins

`declare_plugin!` exports a **C ABI** (`cardforge_plugin_abi`, `cardforge_create_plugin`,
`cardforge_plugin_id` / `_name` / `_version`). The host builds a shim `Plugin` on its
own heap — never pass `Box<dyn Plugin>` across the `.so` boundary (Horizon's mimalloc
vs the plugin allocator will SIGSEGV).

```rust
use cardforge_api::declare_plugin;
#[derive(Default)]
struct MyPlugin;
impl cardforge_api::Plugin for MyPlugin { /* description() id/name/version */ }
declare_plugin!(MyPlugin);
```

```sh
cargo build -p example-so-plugin --release
cp target/release/libexample_so_plugin.so /path/to/server/plugins/
```

The host loads every `plugins/*.so` at `server_about_to_start` and registers
`/<id>` (plus `/exampleso` for the shipped example).

## Building the Horizon host

```sh
horizon-launcher native-build crates/cardforge-host \
  --server-output /path/to/engine-dir \
  --version nightly-2026-11-05-66f3981 \
  --trust-native-code \
  --profile performance
```

Point `server.toml` `[engine].path` at the resulting `horizon-engine`.
Optional: `CARDFORGE_PLUGINS=/path/to/plugins`.

## License

MIT — see [LICENSE](LICENSE).
