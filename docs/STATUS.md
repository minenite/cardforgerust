# Status

Started: 2026-11-07 by icedmoca.

## Done

- [x] Repo scaffold under `minenite/cardforgerust`
- [x] `cardforge-api` events + `Plugin` + `Server`
- [x] `cardforge-core` bus + manager + tests
- [x] `hello-plugin` example
- [x] Draft `cardforge-host` trait mod for Horizon 26.3

## In progress

- [ ] First successful `native-build` + dedicated server smoke test

## Blocked / needs machine time

- Horizon trait mod compile (pulls full `horizon-client`)
- Confirm `Host::tell` join message behaviour on dedicated vs integrated

## Explicit non-goals (for now)

- Loading Java Bukkit/Paper jars
- Full Paper API surface parity
- Valkyrien Skies / MineniteVS on Horizon
