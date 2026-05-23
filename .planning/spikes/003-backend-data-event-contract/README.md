---
spike: 003
name: backend-data-event-contract
type: standard
validates: "Given MESH backend providers publish service state and receive service commands, when the runtime bridge and frontend proxy paths are traced, then the data and event contract is clear enough to complete and harden"
verdict: PARTIAL
related: []
tags: [backend, services, events, state, luau, runtime]
---

# Spike 003: Backend Data/Event Contract

## What This Validates

Given a Luau backend provider, when it emits state, handles commands, and the frontend reads or mutates service data, then MESH should have a clear contract for what flows through state snapshots, what flows through commands, and what counts as an event.

## Research

No external dependency research was needed. This spike is grounded in the current Rust/Luau runtime.

| Area | Source | Finding |
|------|--------|---------|
| Backend loop | `crates/core/runtime/backend/src/lib.rs` | Backends emit changed state snapshots from `init`, `on_poll`, and command handlers. Commands are received through an MPSC channel and can be coalesced. |
| Backend script API | `crates/core/runtime/scripting/src/backend/runtime.rs` | Backend scripts expose `state`, `mesh.service.emit(...)`, `mesh.service.payload()`, `mesh.service.set_poll_interval(...)`, `mesh.exec(...)`, and `on_command_<name>()`. |
| Frontend proxy | `crates/core/runtime/scripting/src/context/proxy.rs` | `require("mesh.audio@>=1.0")` returns a proxy where method calls publish command events and field reads resolve from `__mesh_svc_audio`. |
| Shell bridge | `crates/core/shell/src/shell/runtime/service_state.rs` | Shell records latest provider state by canonical interface, validates shape against the interface contract, and delivers updates to components with read capability. |
| Command dispatch | `crates/core/shell/src/shell/runtime/request.rs` | Frontend commands are capability-checked, contract-checked, sent to the active provider handler, and may optimistically update shell state for audio mute. |

**Chosen approach:** document the real runtime flow and classify the current contract as state snapshots plus commands, not a complete event bus.

## How to Run

Verification commands used:

```bash
cargo test -p mesh-core-backend -- --nocapture
cargo test -p mesh-core-scripting interface_proxy_method_publishes_service_command -- --nocapture
```

The shell-level service tests currently require the system `xkbcommon.pc` package because `mesh-core-shell` builds `smithay-client-toolkit`.

## What to Expect

Backend data currently flows like this:

```text
backend Luau state / mesh.service.emit(...)
  -> BackendServiceEvent::Update { service, source_module, payload }
  -> ShellMessage::BackendServiceUpdate
  -> latest_service_state[mesh.<service>]
  -> component.handle_service_event(...)
  -> ScriptState["audio"] and Lua global __mesh_svc_audio
  -> frontend proxy reads: audio.percent, audio.state.percent, audio.muted
```

Frontend commands flow back like this:

```text
frontend Luau: audio.set_muted("default", true)
  -> PublishedEvent channel "mesh.audio.set_muted"
  -> CoreRequest::ServiceCommand
  -> Shell::dispatch_service_command(...)
  -> active backend command_tx
  -> backend on_command_set_muted()
  -> mesh.service.payload()
  -> command result + optional changed state snapshot
```

## Investigation Trail

- Traced backend lifecycle from `spawn_backend_service`: load script, call `init`, publish initial state, poll on interval, dispatch queued commands, publish changed snapshots only.
- Confirmed command handlers can update public state by mutating the global `state` table or calling `mesh.service.emit(...)`; the backend runtime snapshots after each command.
- Confirmed command results exist as `BackendServiceEvent::CommandResult`, but shell currently logs them in the backend bridge rather than exposing them to frontend callers or debug state as first-class acknowledgements.
- Confirmed frontend service method calls are contract-generated proxy methods. They return an immediate queued result to Luau, not the backend command result.
- Confirmed frontend state reads are live proxy reads from `__mesh_svc_<service>`, and direct `audio.percent` remains a compatibility alias for `audio.state.percent`.
- Confirmed `[[events]]` in `interface.toml` is metadata today. There is no runtime path that takes declared backend interface events like `VolumeChanged` and delivers them to frontend subscribers.
- Confirmed service shape validation is warning-only. Missing or wrong state fields are recorded as lifecycle diagnostics, but the update still flows.
- Confirmed cached service state exists in `latest_service_state` and is replayed during shell startup after mount. A surface/runtime created after the latest update can still observe `nil` proxy fields until another update or an explicit replay path reaches that runtime.

## Results

Verdict: PARTIAL.

What is complete enough:

- Full-state snapshots from backend to shell to frontend work.
- Frontend service proxy reads work for current state fields.
- Frontend command calls become backend command messages.
- Commands are capability-checked and interface-contract-checked.
- Coalescable commands have last-wins queue behavior.
- Backend command handlers can return result tables and publish updated state.
- Backend runtime failures and state shape mismatches are visible through diagnostics.

What is incomplete or ambiguous:

- Interface-declared `[[events]]` are not a real runtime event mechanism yet.
- Backend command results are not delivered back to the frontend caller; the frontend only knows the command was queued.
- Latest service state replay is not guaranteed at every frontend runtime creation boundary, which can produce transient `nil` service fields in newly shown/imported surfaces.
- The public frontend contract mixes `audio.field` and `audio.state.field`; one should become canonical.
- State snapshots are whole payloads with warning-only validation; there is no strict schema enforcement, versioned migration, or partial update model.
- Backend provider selection and inactive-provider filtering exist, but frontend diagnostics do not clearly expose "command queued to provider X, backend returned Y" as a user-facing trace.

## Desired Module Object Model

The direction that best fits the system is: every backend and frontend module should be represented as a class-like module instance in Luau. The Lua syntax should feel normal, while Rust owns validation, routing, permissions, replay, and serialization underneath.

Conceptually, a backend service instance should look like this to consumers:

```lua
local audio = require("mesh.audio@>=1.0")

print(audio.state.percent)
print(audio.state.muted)

audio:set_muted("default", true)

local unsubscribe = audio.events.VolumeChanged:subscribe(function(event)
  print(event.percent)
end)
```

Conceptually, a frontend module instance should be able to expose values and events in the same shape:

```lua
local tray = require("mesh.frontend.navigation_bar")

print(tray.exports.visible)
tray.exports:set_badge("audio", "muted")

tray.events.ItemActivated:subscribe(function(event)
  print(event.item_id)
end)
```

Under that syntax, each module instance should have four explicit surfaces:

| Surface | Meaning | Transport |
|---------|---------|-----------|
| `module.state.<field>` | Durable current data owned by the module | replayable full snapshots |
| `module:<method>(...)` or `module.methods.<name>(...)` | Request to do work or mutate module state | queued command/call with result |
| `module.events.<Event>:subscribe(fn)` | Transient facts that should not be modeled as state | typed event lane |
| `module.exports.<field>` | Public values exported by frontend modules | same read model as state, but scoped to frontend module exports |

The important rule is that this object model is an API illusion, not a separate transport. The runtime should still route through a small number of explicit lanes:

```text
state/export snapshots: module instance -> shell registry -> subscribed runtimes
method calls: caller runtime -> shell router -> target module instance -> result/ack
events: module instance -> shell event bus -> subscribed handlers
```

This keeps Luau authoring simple while keeping Rust in charge of capabilities, schema checks, lifecycle, and diagnostics.

## Recommended Completion Contract

1. **Module instances:** formalize backend services and frontend modules as runtime-registered object instances. Each instance has a stable module id, interface/version, capabilities, lifecycle state, state/export table, methods, and events.
2. **State/export snapshots:** keep snapshots as the authoritative read model. Every provider or frontend export update should publish a full contract-shaped payload with required fields. Shell should replay latest data into every runtime when it is created, shown, or reloaded.
3. **Methods:** expose callable methods with normal Lua object syntax. Underneath, route calls as queued commands/calls, then expose acknowledgements/results through a real result path instead of tracing only.
4. **Events:** implement a real typed event lane. Backend and frontend module instances should be able to emit declared events, shell should validate them against the interface/module contract, and consumers should subscribe through constrained APIs like `module.events.Name:subscribe(fn)`.
5. **Canonical frontend read API:** standardize docs and examples on `module.state.field` for backend state and `module.exports.field` for frontend exports. Keep `audio.field` only as a compatibility alias if needed.
6. **Observability:** add one debug inspector path that shows registered module instances, latest state/exports, active provider, recent method calls, call results, subscriptions, and recent service events per interface.

## Signal for Build

Do not build new backend or frontend integration features as one-off callback APIs. Complete the existing service model by making module instances and their lanes explicit:

- state/export snapshot lane for durable current data;
- method lane for cross-module calls and mutations;
- event lane for transient facts that consumers can subscribe to.

The target authoring model is "Lua objects over typed runtime lanes": modules feel like classes, but the shell still controls routing, validation, replay, permissions, and diagnostics.
