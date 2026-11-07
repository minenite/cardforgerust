# Status

Started: 2026-11-07 by icedmoca.

## Done

- [x] Repo scaffold under `minenite/cardforgerust`
- [x] `cardforge-api` events + `Plugin` + `Server` + `colorize` + permissions
- [x] `cardforge-core` bus + manager + commands + dynamic `.so` loader
- [x] `hello-plugin` (join greeting + `!cancel` chat demo)
- [x] `cardforge-host` Horizon trait mod — native-build + dedicated smoke
- [x] Built-in commands `/plugins` `/plugin` `/version` `/cardforge` (colored)
- [x] `Hooks::player_chat` → `PlayerChatEvent` (cancel + sender replies)
- [x] Bukkit-style permission nodes + Vault-shaped `PermissionProvider`
- [x] Dynamic plugins: `cardforge_create_plugin` / `declare_plugin!`
- [x] Gameplay ports: `cf-friends`, `cf-ranks`, `cf-travel`

## In progress / follow-ups

- [ ] Warp teleport apply (needs Horizon Host position API)
- [ ] Friend add by live online UUID resolve
- [ ] Rank chat prefix on the wire (Horizon signed chat cannot rewrite body)
- [ ] Example cdylib plugin crate + install docs

## Explicit non-goals (for now)

- Loading Java Bukkit/Paper jars
- Full Paper API surface parity
- Valkyrien Skies / MineniteVS on Horizon
