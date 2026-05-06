# `@mesh/reference-media`

Deterministic in-memory media backend used as the Phase 5 MVP authoring reference.

- **Type:** `backend`
- **Manifest ID:** `@mesh/reference-media`
- **Implements:** `mesh.media`
- **Base plugin:** `@mesh/media-interface`
- **Backend name:** `Reference`
- **Priority:** `10`
- **Entrypoint:** `packages/plugins/backend/core/reference-media/src/main.luau`

## Why this plugin is the reference

`@mesh/reference-media` is intentionally simple and deterministic:

- it does not call external binaries
- it seeds its playlist from `mesh.config()`
- it exports top-level `state = { ... }`
- it uses `init()` plus `mesh.service.set_poll_interval(5000)`
- it handles commands with `on_command_play`, `on_command_pause`, `on_command_next`, and `on_command_previous`

That makes it the shortest proven path for backend plugin authors who want to copy the MVP contract without inheriting placeholder or platform-specific behavior.

## Manifest contract

The provider manifest lives at `packages/plugins/backend/core/reference-media/plugin.json` and declares:

- `id = "@mesh/reference-media"`
- `type = "backend"`
- `entrypoints.main = "src/main.luau"`
- `provides[0].interface = "mesh.media"`
- `provides[0].base_plugin = "@mesh/media-interface"`
- required capabilities `service.media.read` and `service.media.control`

## Runtime shape

The implementation lives in `packages/plugins/backend/core/reference-media/src/main.luau`.

It reads config once at startup:

```lua
local cfg = mesh.config()
```

It then seeds an in-memory playlist and exports public provider state through a top-level global:

```lua
state = {
    available = true,
    title = playlist[current_index].title,
    artist = playlist[current_index].artist,
    album = playlist[current_index].album,
    state = playback_state,
}
```

This is the MVP author path from Phase 4 onward: backend providers export `state = ...`, and the runtime snapshots that state after `init()`, `on_poll()`, and command handlers return.

## `init()` and polling

`init()` proves three core backend APIs in one place:

- `mesh.log.info(...)` for provider-scoped logs
- `mesh.service.set_poll_interval(5000)` for poll cadence
- `sync_state()` to publish the first reactive snapshot through exported state

The poll handler is intentionally a no-op refresh:

- `on_poll()` calls `sync_state()`
- no external process or daemon is required
- authors can see the runtime pattern without extra integration noise

## Command handlers

The reference provider exposes four command handlers:

- `on_command_play()` logs the request, switches `state.state` to `"playing"`, and returns `{ ok = true }`
- `on_command_pause()` logs the request and returns `{ ok = false, error = "not currently playing" }` if the player is not in the `"playing"` state; otherwise it switches to `"paused"` and returns `{ ok = true }`
- `on_command_next()` advances the in-memory playlist, updates state, and returns `{ ok = true }`
- `on_command_previous()` moves backward in the playlist, updates state, and returns `{ ok = true }`

Each handler reads the current payload with `mesh.service.payload()` so the command path stays generic and interface-driven.

## Proven author pattern

For a new backend MVP provider, copy this structure:

1. Declare the provider in `package.json` with `mesh.implements[].interface` and `basePlugin`.
2. Read plugin settings from `mesh.config()`.
3. Export top-level `state = { ... }` with the interface's required fields.
4. Use `init()` to log startup, set the poll interval, and prepare the first state snapshot.
5. Keep `on_poll()` deterministic and focused on refreshing exported state.
6. Implement `on_command_<name>()` handlers that mutate state and return small result tables.

## Verified commands

These tests prove the reference contract:

- `nix develop -c cargo test -p mesh-core-scripting reference_media`
- `nix develop -c cargo test -p mesh-core-backend reference_media`

Those tests cover config seeding, initial state export, polling, command dispatch, result tables, and failure-path attribution for `@mesh/reference-media`.
