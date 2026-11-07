# Architecture

## Layers

| Layer | Crate | Role |
| --- | --- | --- |
| Game | Horizon `horizon-client` / `horizon-server` | Real 26.3 simulation |
| Bridge | `cardforge-host` | Horizon `Hooks` → CardForge events |
| Runtime | `cardforge-core` | Plugin manager, event bus, load order |
| API | `cardforge-api` | What plugins compile against |
| Plugins | e.g. `hello-plugin`, later ServerPlugin-rust | Gameplay / network logic |

## Event flow (join)

```
Horizon native_player_logged_in
  -> Hooks::player_logged_in (cardforge-host)
      -> update Server snapshot
      -> EventBus.call(PlayerJoinEvent)
          -> hello-plugin listener (may set join_message)
      -> Host::tell(join_message)
```

## Relation to minenite/cardforge

Java CardForge = Paper API on NeoForge mixins, loads `.jar` plugins.

CardForgeRust = same product shape for Horizon:

- Event priorities (Lowest → Monitor)
- Plugin lifecycle (load → enable → disable)
- Hard / soft depends + topological enable order
- Per-plugin data folder under `plugins/<id>/`

It does **not** embed a JVM or rewrite CraftBukkit class-by-class.

## Next milestones

1. Wire host through `horizon-launcher native-build` against local Horizon; boot dedicated server; see hello join line.
2. Chat hook (`native_player_command` / chat packet) → `PlayerChatEvent`.
3. Command registrar + `/cardforge` debug command.
4. Port pieces of `serverplugin` (friends, ranks, travel) as Rust plugins.
5. Optional cdylib plugin loader once the in-process API is stable.
