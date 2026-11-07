# Architecture

## Layers

| Layer | Crate | Role |
| --- | --- | --- |
| Game | Horizon `horizon-client` / `horizon-server` | Real 26.3 simulation |
| Bridge | `cardforge-host` (`mod-cardforge`) | Horizon `Hooks` → CardForge events |
| Runtime | `cardforge-core` | Plugin manager, bus, commands, `.so` loader, perms |
| API | `cardforge-api` | What plugins compile against |
| Plugins | `hello-plugin`, `cf-friends`, `cf-ranks`, `cf-travel`, future `.so` | Gameplay |

## Event flow (join)

```
Horizon native_player_logged_in
  -> Hooks::player_logged_in (cardforge-host)
      -> update Server snapshot
      -> EventBus.call(PlayerJoinEvent)
          -> hello-plugin listener (may set join_message)
      -> Host::tell(join_message)
```

## Event flow (chat)

```
Horizon native_player_chat
  -> Hooks::player_chat
      -> EventBus.call(PlayerChatEvent)
          -> plugins may cancel + push replies
      -> Flow::Cancel drops broadcast (signed body cannot be rewritten)
```

## Relation to minenite/cardforge

Java CardForge = Paper API on NeoForge mixins, loads `.jar` plugins.

CardForgeRust = same product shape for Horizon:

- Event priorities (Lowest → Monitor)
- Plugin lifecycle (load → enable → disable)
- Hard / soft depends + topological enable order
- Per-plugin data folder under `plugins/<id>/`
- Bukkit-style permission nodes
- Dynamic `cdylib` plugins via `cardforge_create_plugin`

It does **not** embed a JVM or rewrite CraftBukkit class-by-class.

## Next milestones

1. Host teleport / position for `/warp` apply
2. Example out-of-tree `.so` plugin + packaging docs
3. Port more ServerPlugin surfaces as needed
4. Astris web client MP against this stack ([minenite/Astris](https://github.com/minenite/Astris))
