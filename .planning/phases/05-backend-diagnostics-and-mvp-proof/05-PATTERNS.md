---
phase: 05-backend-diagnostics-and-mvp-proof
status: complete
created: 2026-05-04
---

# Phase 05 Pattern Map

## Closest Existing Analogs

| Planned Area | Target Files | Closest Existing Analog | Pattern to Reuse |
|--------------|--------------|-------------------------|------------------|
| Backend failure-stage reporting | `crates/core/runtime/scripting/src/backend.rs`, `crates/core/runtime/backend/src/lib.rs` | `BackendCommandOutcome`, `BackendServiceEvent::PollFailed`, `BackendServiceEvent::Failed` | Keep runtime events generic and JSON-based; add sharper stage attribution without service-specific branches. |
| Shell latest-state invalidation | `crates/core/shell/src/shell/mod.rs`, `crates/core/shell/src/shell/types.rs` | `record_latest_service_state()`, `handle_backend_lifecycle()`, theme sync helpers | Reuse canonical-interface state cache and active-provider checks; synthesize unavailable/error state in the shell rather than in service-specific Rust code. |
| Diagnostics dedup buckets | `crates/core/foundation/diagnostics/src/lib.rs` | `record_handler_error()`, `record_lifecycle_error()`, `DiagnosticsCollector::record_lifecycle_error()` | Replace `HashSet` tuple dedup with a keyed map that updates count and last-seen metadata while preserving health/error count semantics. |
| Runtime status visibility | `crates/core/shell/src/shell/mod.rs`, debug snapshot types | `backend_runtime_statuses`, `build_debug_snapshot()` | Extend existing status records instead of creating parallel shell-only diagnostics stores. |
| Fresh reference provider | `packages/plugins/backend/core/reference-media/plugin.json`, `packages/plugins/backend/core/reference-media/src/main.luau` | `packages/plugins/backend/core/pipewire-audio/src/main.luau`, `packages/plugins/backend/core/upower-power/src/main.luau` | Use top-level exported `state`, `init()` poll interval setup, shared refresh helpers, and command handlers returning result tables. |
| Backend author docs | `docs/plugins/backend/core/reference-media/README.md`, `docs/plugins/backend/core/README.md` | Existing backend provider README files under `docs/plugins/backend/core/*/README.md` | Keep concise plugin README structure, but correct architecture statements to match Phase 2-4 decisions. |

## Concrete Patterns

### Runtime failure signaling

Preferred shape:

```rust
match ctx.run_command_with_result(&msg.command, &msg.payload) {
    Ok(outcome) => { /* command result + optional state */ }
    Err(err) => {
        tx.send(BackendServiceEvent::Failed {
            service: service_name.clone(),
            source_plugin: plugin_id.clone(),
            stage: "command".to_string(),
            message: err.to_string(),
        })?;
    }
}
```

Keep stage names generic strings and preserve existing event-channel patterns.

### Shell-owned stale-state clearing

Preferred shape:

```rust
if active_provider_failed {
    self.latest_service_state.insert(
        interface.clone(),
        LatestServiceState {
            interface,
            provider_id,
            state: unavailable_payload,
        },
    );
}
```

Do not reintroduce service-specific branches. Use contract-aware unavailable payload generation where possible.

### Diagnostics bucket aggregation

Preferred shape:

```rust
struct LifecycleErrorRecord {
    provider_id: String,
    stage: String,
    latest_message: String,
    count: u64,
    last_seen: SystemTime,
}
```

Index by `(provider_id, stage)` and update `count` / `last_seen` on repeats.

### Reference provider structure

Preferred provider shape:

```lua
state = {
    available = true,
    title = "Reference Track",
    artist = "MESH",
    album = "Backend MVP",
    state = "paused",
}

function init()
    mesh.log.info("reference-media init")
    mesh.service.set_poll_interval(5000)
end

function on_command_play()
    state.state = "playing"
    return { ok = true }
end
```

Keep state deterministic and OS-independent so tests stay stable.

## Integration Warnings

- `docs/plugins/backend/core/README.md` still documents fallback selection and `mesh.exec_shell`; Phase 5 docs must actively remove those claims.
- Interface TOMLs still contain some `source_plugin` narrative from older assumptions; docs and proof artifacts should not copy that pattern back into provider state.
- Shell failure handling already uses `backend_runtime_statuses`; avoid creating a second stale-state authority.
- The fresh reference plugin must not reuse `mpris-media` or `mock-notifications` paths; the phase context explicitly rejected retrofitting existing placeholders.

## Recommended File Ownership

- Plan 01 owns runtime failure-stage semantics in `crates/core/runtime/scripting/src/backend.rs` and `crates/core/runtime/backend/src/lib.rs`.
- Plan 02 owns shell lifecycle visibility and diagnostics aggregation in `crates/core/shell/src/shell/mod.rs`, `crates/core/shell/src/shell/types.rs`, and `crates/core/foundation/diagnostics/src/lib.rs`.
- Plan 03 owns the fresh reference provider under `packages/plugins/backend/core/reference-media/` plus focused runtime tests.
- Plan 04 owns backend author docs under `docs/plugins/backend/core/`.

