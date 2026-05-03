---
phase: 04-service-provider-contract
status: complete
created: 2026-05-03
---

# Phase 04 Pattern Map

## Closest Existing Analogs

| Planned Area | Target Files | Closest Existing Analog | Pattern to Reuse |
|---------------|--------------|-------------------------|------------------|
| Backend state snapshot | `crates/core/runtime/scripting/src/backend.rs` | `BackendRuntime.pending_emit`, `take_pending_emit()`, `run_poll()` | Keep backend host data behind `Arc<Mutex<BackendRuntime>>`; snapshot after callbacks; convert through `LuaSerdeExt`. |
| Backend command result routing | `crates/core/runtime/backend/src/lib.rs` | `BackendServiceEvent::Update`, command branch in `spawn_backend_service()` | Add typed runtime events and tests using `mpsc::unbounded_channel()` plus `tokio::time::timeout`. |
| Interface/provider metadata | `crates/core/shell/src/shell/mod.rs` | `BackendLaunchCandidate`, `BackendRuntimeSlot`, `latest_service_events` | Keep canonical interface as the public key and provider id as metadata. |
| Interface contract validation | `crates/core/extension/service/src/contract.rs`, `crates/core/shell/src/shell/mod.rs` | `InterfaceContract.state_fields`, `InterfaceContract.methods`, `scan_plugin_dir()` | Validate by iterating contract data; never branch on service names. |
| Frontend interface proxy | `crates/core/runtime/scripting/src/context.rs` | `require("@mesh/audio")`, `create_service_proxy()` | Extend the existing proxy table/metatable rather than adding a second import path. |
| Service update application | `crates/core/shell/src/shell/service.rs`, `component.rs` | `apply_service_update()`, `apply_service_payload()` | Continue setting raw payloads directly; add `module.state` as a proxy view. |
| Bundled provider migration | `packages/plugins/backend/core/*/src/main.luau` | Current `read_state()`, `emit_state()`, `on_command_*()` helpers | Assign top-level `state`, update it in helpers, and return result tables from command handlers. |

## Concrete Patterns

### Backend Callback Snapshot

Preferred shape:

```rust
ctx.call_init()?;
if let Some(state) = ctx.take_service_state_snapshot()? {
    tx.send(BackendServiceEvent::Update(... state ...))?;
}
```

Keep interval refresh after callbacks, matching Phase 3.

### Backend State In Luau

Preferred provider shape:

```lua
state = {
    available = false,
    percent = 0,
    muted = false,
}

function on_poll()
    state = read_state() or { available = false }
end
```

Avoid provider-authored identity fields:

```lua
source_plugin = "@mesh/pipewire-audio"
```

Provider identity belongs in runtime metadata.

### Frontend Proxy State

Preferred consumer shape:

```lua
local audio = require("@mesh/audio")
local pct = audio.state.percent or 0
local result = audio.set_volume("default", 0.65)
```

Keep compatibility direct field reads during migration:

```lua
local pct = audio.percent or 0
```

### Command Results

Use a small JSON-compatible result table:

```lua
{ ok = true }
{ ok = false, error = "permission denied" }
```

If frontend proxy dispatch is asynchronous, return an immediate dispatch result:

```lua
{ ok = true, queued = true }
```

Backend handler completion results should still be visible through runtime events or diagnostics.

## Integration Warnings

- `require("@mesh/audio")` already resolves through `InterfaceCatalog`; do not bypass it with provider-specific lookup for normal imports.
- `service_name_from_interface()` intentionally converts `mesh.audio` to `audio` for frontend state keys. Preserve canonical `mesh.audio` in shell/runtime metadata.
- Current `PublishedEvent` is event-like and does not carry a returned result. Add a minimal result path without blocking component event handlers.
- Do not remove `audio.percent` compatibility until bundled frontend surfaces are migrated to `audio.state.percent`.
- Grep checks for `source_plugin` in providers should avoid interface TOML comments until contracts are updated.

## Recommended File Ownership

- Plan 01 owns `crates/core/runtime/scripting/src/backend.rs` and `crates/core/runtime/backend/src/lib.rs`.
- Plan 02 owns shell latest-state metadata and contract validation in `crates/core/shell/src/shell/mod.rs`, `types.rs`, and service contract helpers.
- Plan 03 owns frontend proxy shape in `crates/core/runtime/scripting/src/context.rs` and service update application in `crates/core/shell/src/shell/service.rs` / `component.rs`.
- Plan 04 owns bundled provider migration under `packages/plugins/backend/core/**/src/main.luau` and compatibility tests.
