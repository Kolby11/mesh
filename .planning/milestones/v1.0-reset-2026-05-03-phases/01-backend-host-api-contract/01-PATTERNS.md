# Phase 1 Pattern Map: Backend Host API Contract

**Phase:** 01 — Backend Host API Contract
**Generated:** 2026-05-01

## Closest Existing Analogs

| Planned Area | Closest Existing Analog | Pattern to Reuse |
|--------------|-------------------------|------------------|
| Backend Luau host API injection | `crates/core/runtime/scripting/src/backend.rs::install_host_api` | Create `mlua` tables/functions, capture runtime state with `Arc<Mutex<_>>`, install under global `mesh`. |
| Command result tables | `exec_outcome_to_lua()` in `backend.rs` | Return a Luau table with `success`, `stdout`, `stderr`, and `code`. |
| Service emission | `mesh.service.emit`, `emit_json`, `emit_unavailable` in `backend.rs` | Convert Luau values to `serde_json::Value` and store one pending payload per handler call. |
| Backend polling loop | `crates/core/runtime/backend/src/lib.rs::spawn_backend_service` | Load script, call `init()`, tick on interval, dedupe payloads, send `BackendServiceUpdate`. |
| Shell backend spawn | `crates/core/shell/src/shell/mod.rs` backend spawn block | Resolve plugin script, create command channel, forward backend updates into shell `ServiceEvent::Updated`. |
| Real service fixtures | `packages/plugins/backend/core/*/src/main.luau` | `init()`, `on_poll()`, `on_command_*`, `mesh.exec_shell`, `mesh.service.payload`, `mesh.service.emit`. |

## File Roles

### `crates/core/runtime/scripting/src/backend.rs`

Role: Backend Luau API surface and unit-test home.

Expected changes:
- Add structured `mesh.exec(program, args)` support.
- Add backend settings storage and `mesh.config()`.
- Add `mesh.log(level, msg)` plus `warn`/`error` aliases.
- Add unit tests for HOST-01 through HOST-05.

### `crates/core/runtime/scripting/src/host_api.rs`

Role: Shared host API notes and capability/interface helper types.

Expected changes:
- Update public API comments so backend documented forms do not conflict with implemented forms.

### `crates/core/runtime/backend/src/lib.rs`

Role: Async backend service orchestration.

Expected changes:
- Pass plugin settings into `BackendScriptContext` once shell plumbing exists.
- Make poll interval changes effective while the backend loop is running.
- Add async tests for update sending and interval behavior where practical.

### `crates/core/shell/src/shell/mod.rs`

Role: Shell-level backend plugin spawn and settings source.

Expected changes:
- Pass plugin settings JSON to `spawn_backend_service()` or a configuration struct used by it.

## Data Flow to Preserve

```text
backend main.luau
  -> BackendScriptContext host API
  -> pending_emit: serde_json::Value
  -> spawn_backend_service()
  -> BackendServiceUpdate
  -> ShellMessage::Service(ServiceEvent::Updated)
```

## Implementation Constraints

- Do not move audio/network/power/media logic into Rust.
- Preserve existing bundled Luau plugin compatibility.
- Keep Phase 1 backend-only; do not implement frontend service proxies here.
- Prefer unit tests in `mesh-core-scripting` for host API behavior and integration tests in `mesh-core-backend` for polling/channel behavior.
