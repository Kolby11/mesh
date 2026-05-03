# Phase 04: Service Provider Contract - Research

**Status:** Complete
**Research question:** What needs to be known to plan Phase 4 well?

## Phase Goal

Connect backend providers to service interfaces generically so exported backend state and service command dispatch work without service-specific Rust branches.

## Current Architecture Findings

### Backend Runtime

- `crates/core/runtime/scripting/src/backend.rs` owns `BackendScriptContext`, host API registration, `run_poll()`, and `run_command()`.
- `BackendRuntime` already stores mutable runtime data behind `Arc<Mutex<BackendRuntime>>`, including `pending_emit`, `current_payload`, settings, and poll interval.
- `mesh.service.emit(payload)` currently sets `pending_emit`; `spawn_backend_service()` consumes that payload and sends `BackendServiceEvent::Update`.
- `run_command()` currently normalizes command names with `-` to `_`, calls `on_command_<name>` or `<name>`, and returns only an optional emitted payload.
- Backend command handler errors currently emit `BackendServiceEvent::Failed { stage: "command" }`; there is no caller-visible command result type.

### Shell State And Provider Metadata

- `crates/core/runtime/backend/src/lib.rs` defines `BackendServiceUpdate { service, source_plugin, payload }`.
- `crates/core/shell/src/shell/mod.rs` bridges backend updates to `ServiceEvent::Updated` and stores latest frontend-facing updates in `Shell.latest_service_events`.
- `Shell.latest_service_events` is keyed by service/interface string and currently stores a whole `ServiceEvent`, so provider identity is already preserved as event metadata.
- `backend_launch_candidates_from_graph()` already resolves active providers from `InstalledModuleGraph::active_provider(interface)` and uses the module id as `candidate.module_id`.
- `scan_plugin_dir()` already loads interface contracts and registers providers into `InterfaceRegistry`, so Phase 4 can reuse `InterfaceContract.state_fields` and `InterfaceContract.methods` for warning-level validation.

### Frontend Interface Proxy

- `crates/core/runtime/scripting/src/context.rs` already supports `require("@mesh/audio")` and versioned `require("@mesh/audio@>=1.0")`.
- The current `create_service_proxy()` returns a Lua table with an `__index` metamethod:
  - Contract methods publish a `PublishedEvent` such as `mesh.audio.set_volume`.
  - State field reads fall through to the live `__mesh_svc_audio` payload table.
- The proxy currently supports `audio.percent`, not `audio.state.percent`.
- Proxy command methods currently return `default_lua_value_for_type(...)`, not a concrete result table.
- `crates/core/shell/src/shell/service.rs::script_events_to_requests()` translates published proxy commands into `CoreRequest::ServiceCommand` and checks `service.<name>.control`.

### Frontend Component Service Updates

- `FrontendSurfaceComponent::handle_service_event()` applies service updates through `apply_service_update()` and calls `ScriptContext::apply_service_payload()`.
- `apply_service_update()` writes `state[service_name] = payload` and updates `last_service_update`.
- `apply_service_payload()` writes `__mesh_svc_<service>` into Lua globals for proxy reads.
- Existing tests cover direct proxy state reads, raw service update repaint scheduling, capability-gated service state, and service proxy command publication.

## Recommended Implementation Approach

### 1. Add Exported Backend State Before Replacing Emit

The lowest-risk backend change is to add snapshot support while preserving `mesh.service.emit(...)` as compatibility:

- Add a helper on `BackendScriptContext`, for example `take_service_state_snapshot()`, that reads top-level global `state`.
- Accept only JSON-compatible `state` values. If conversion fails, return a warning/error value that can become a diagnostic without crashing the shell.
- Snapshot after `call_init()`, `run_poll()`, and `run_command()`.
- If `mesh.service.emit(...)` was called during the callback, prefer the explicit compatibility payload for that callback or copy it into the same snapshot path. The plan should choose one behavior and test it. Preferred: exported `state` is primary, `emit` is a compatibility setter for the same pending state.

### 2. Add Command Result As A Separate Backend Runtime Outcome

Backend command handlers need to produce both:

- `state`: the latest service state after command completion.
- `result`: a small result table for command success/failure.

Recommended mechanics:

- Let a backend `on_command_*` handler return a Luau table. If it returns nil and does not error, default to `{ ok: true }`.
- If the handler throws, convert to `{ ok: false, error: "<message>" }` and still emit a command failure lifecycle/diagnostic event.
- Keep `mesh.service.payload()` as the command input API.
- Add `BackendServiceEvent::CommandResult` or equivalent, with interface/service, source plugin, command, and result JSON.

The current frontend scripting model is synchronous while shell dispatch is asynchronous. Therefore the first phase should make proxy calls return immediate dispatch result tables (`{ ok: true, queued: true }` or `{ ok: false, error: "..." }`) and route backend handler results into diagnostics/lifecycle plumbing. A later richer await/promise model can be a separate feature.

### 3. Store Public State Per Interface, Provider As Metadata

The existing `latest_service_events` map is already close to the desired model:

- Key by canonical interface, e.g. `mesh.audio`.
- Store payload as public `state`.
- Store provider id/source plugin alongside it as metadata.
- Do not inject `source_plugin` into public `state`.

This lets `require("@mesh/audio").state` resolve to the active provider state while diagnostics can still show `@mesh/pipewire-audio`.

### 4. Expose `module.state` Without Breaking Existing Field Reads

The transition should add `module.state` and keep direct field reads for compatibility:

- `audio.state.percent` reads from the latest `__mesh_svc_audio` table.
- `audio.percent` can continue to work as a compatibility alias.
- Tracking should mark `state` or nested reads in a way that service updates still schedule repaints when state changes.
- Tests should assert `require("@mesh/audio").state.percent` and `require("@mesh/audio").percent` both read the same latest state during migration.

### 5. Validate Contracts At Warning Level

Use existing interface contract data:

- Provider declaration validation belongs near graph candidate derivation and interface registry setup.
- Unknown command names should produce a caller-visible `{ ok: false, error: "unsupported command" }` or diagnostic instead of silently dropping.
- State shape mismatches should warn/diagnose, not stop providers in Phase 4.
- Do not add service-specific state validators. Validation should iterate `InterfaceContract.state_fields`.

## Planning Implications

Recommended plan order:

1. Backend runtime state snapshots and command result events.
2. Shell latest-state storage, provider metadata, and warning-level contract validation.
3. Frontend `require("@mesh/<interface>").state` proxy and dispatch result tables.
4. Bundled provider migration from `mesh.service.emit(...)` and `source_plugin` fields to top-level `state`.

## Validation Architecture

### Test Infrastructure

- Framework: Rust built-in test harness with Tokio tests.
- Quick backend command: `nix develop -c cargo test -p mesh-core-scripting backend`
- Quick runtime command: `nix develop -c cargo test -p mesh-core-backend spawn_backend_service`
- Quick shell command: `nix develop -c cargo test -p mesh-core-shell service_contract`
- Full command: `nix develop -c cargo test -p mesh-core-scripting backend && nix develop -c cargo test -p mesh-core-backend spawn_backend_service && nix develop -c cargo test -p mesh-core-shell service_contract`

### Required Test Areas

- Backend top-level `state` snapshot after `init()`, `on_poll()`, and `on_command_*()`.
- `mesh.service.emit(...)` compatibility path during migration.
- Backend command handler returned result table and error result behavior.
- Shell latest state keyed by interface with provider id metadata separate from public payload.
- Warning-level state-field validation using `InterfaceContract.state_fields`.
- Frontend `require("@mesh/audio").state.percent` reads latest state and tracks invalidation.
- Frontend command proxy methods return `{ ok = true, queued = true }` for accepted dispatch and `{ ok = false, error = ... }` for denied/unsupported dispatch.
- Bundled providers expose top-level `state` and stop injecting `source_plugin`.

### Landmines

- Do not convert `mesh.audio` to `audio` in storage layers where canonical interface identity is required.
- Do not make backend provider state include provider identity by default.
- Do not add audio/network/power-specific Rust validation or command handling.
- Do not remove `mesh.service.emit(...)` until bundled providers and tests have a migration path.
- Do not promise synchronous backend command completion to frontend scripts unless a real response channel/await model is implemented.

## Research Complete

Phase 4 can be planned with existing code patterns. No external research is required; the key decisions are project-specific architecture and migration sequencing.

## RESEARCH COMPLETE
