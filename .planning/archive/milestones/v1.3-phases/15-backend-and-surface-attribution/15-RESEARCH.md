---
phase: 15
slug: backend-and-surface-attribution
status: complete
created: 2026-05-08
---

# Phase 15 Research: Backend and Surface Attribution

## Research Complete

Phase 15 builds directly on the Phase 14 rolling profiler. The shell already records shell-wide and per-surface stage timings, but the snapshot remains aggregate-only for backend work: there is no backend profiling payload, no provider/service stage rollup, and no stable way to inspect whether visible cost comes from a specific provider, from service-event fanout, or from one expensive surface.

No phase-specific `15-CONTEXT.md` exists from discuss-phase. Planning therefore treats `.planning/ROADMAP.md`, `.planning/REQUIREMENTS.md`, milestone research, and the landed Phase 14 runtime seams as the authoritative source of truth.

## Current State

### Phase 14 already preserves the main surface attribution shape

- `crates/core/foundation/debug/src/lib.rs` exposes `ProfilingSnapshot` with:
  - shell-wide `ProfilingScopeSnapshot`
  - per-surface `ProfilingSurfaceSnapshot`
  - `module_id`, `surface_id`, `redraw_count`, and `total_surface_render_time_micros`
- `crates/core/shell/src/shell/runtime/profiling.rs` already stores:
  - shell-wide stage accumulators
  - per-surface accumulators keyed by `surface_id`
  - bounded recent samples with stage, duration, surface/module identity, redraw count, and trigger kind
- `crates/core/shell/src/shell/runtime/render.rs` already records surface-local stage samples and redraw counts into the shared collector.

This means `TIME-02` is partially prepared already: the shell has per-surface storage, but the contract still needs explicit planning proof that the per-surface view remains first-class and stable for comparison against shell totals.

### Backend activity currently reaches the shell through three distinct seams

- `crates/core/shell/src/shell/backend/spawn.rs`
  - bridges backend runtime events into shell messages
  - sees `BackendServiceEvent::Update`, `CommandResult`, `Started`, `InitFailed`, `PollFailed`, `Failed`, and `Stopped`
- `crates/core/shell/src/shell/runtime/request.rs`
  - owns `dispatch_service_command(...)`
  - knows the target `interface`, command name, source module, active provider lookup, and coalescing path
- `crates/core/shell/src/shell/runtime/service_state.rs`
  - owns `broadcast_service_event(...)`
  - records latest provider state, rejects stale updates, and fans service updates into every mounted component

These are the natural Phase 15 stage boundaries:

- backend `poll/update`
  - represented at the shell seam where a backend update arrives and is accepted
- backend `command handling`
  - represented at the shell-owned service command dispatch path
- backend `state publish/delivery`
  - represented at `broadcast_service_event(...)` and the component delivery fanout it triggers

### The debug snapshot contract has no backend profiling section yet

- `crates/core/foundation/debug/src/lib.rs` only models shell-wide and per-surface profiling.
- `crates/core/shell/src/shell/runtime/debug.rs` only emits `self.profiling.snapshot(...)`.
- `backend_runtimes` already exist in `DebugSnapshot`, but those are lifecycle/health records, not rolling performance summaries.

Phase 15 therefore needs a new typed backend profiling section rather than trying to overload lifecycle status entries with timing data.

### Provider identity is already available at the right shell seams

- `backend/spawn.rs` knows `interface` and `provider_id` when backend events cross into the shell.
- `service_state.rs` gets `source_module` on each `ServiceEvent::Updated`.
- `request.rs` can resolve the active provider for a service command via the shell-owned runtime slot/handler state.

That is enough to satisfy `BACK-01` without changing the provider architecture: the collector can tag backend samples with interface and provider IDs at the shell boundary.

## Recommended Implementation Shape

### 1. Extend the profiling contract with backend snapshots and backend stage types

Add typed backend profiling payloads to `mesh-core-debug` rather than storing backend attribution as ad hoc strings inside shell-wide samples.

Recommended additions:

- `ProfilingBackendSnapshot`
- `ProfilingBackendServiceSnapshot`
- `ProfilingBackendStageSummary`
- `ProfilingBackendStage`

The backend stage enum should be explicit and locked to the requirement language:

- `PollUpdate`
- `CommandHandling`
- `StatePublishDelivery`

Each backend summary should expose:

- interface name
- provider ID
- stage summaries with bounded recent samples
- total counts and durations comparable to the existing surface/shell summaries

### 2. Keep backend attribution in the same bounded collector

Do not create a second profiler store for backend timings. Extend `crates/core/shell/src/shell/runtime/profiling.rs` so one shell-owned collector can snapshot:

- shell-wide stage totals
- per-surface stage totals
- per-backend provider/service stage totals

Use the same fixed-capacity recent-sample pattern Phase 14 already established.

### 3. Instrument backend stages where the shell actually owns them

For this phase, the most honest and maintainable backend timing seams are:

- `backend/spawn.rs` + `runtime/mod.rs`
  - identify provider/service when backend-originated update messages cross into the shell
- `runtime/request.rs`
  - record backend command-handling attribution when the shell dispatches service commands to the active provider
- `runtime/service_state.rs`
  - record state publish/delivery timing around latest-state validation and frontend fanout

This keeps Phase 15 bounded to shell-owned measurement seams and avoids pretending the shell can see arbitrary backend-internal work it does not currently trace.

### 4. Preserve Phase 17 scope boundaries

Phase 15 should make backend hotspots actionable, but it should not overreach into `BACK-03`.

That means:

- do add provider/service stage summaries
- do keep per-surface timings stable and explicit
- do not promise end-to-end benchmark correlations yet
- do not add benchmark scenario logic or inspector UI work here

Phase 17 can later stitch backend-driven interactions into canonical proof flows once the attribution model is stable.

### 5. Prove attribution behavior with shell-level tests

The shell test suite should verify:

- backend profiling snapshots are absent when profiling is disabled
- accepted backend update work records `PollUpdate` against the correct `(interface, provider_id)`
- service command dispatch records `CommandHandling` against the active provider/service
- service-event fanout records `StatePublishDelivery`
- per-surface snapshots still expose stage totals plus stable `module_id` and `surface_id`
- shell totals remain the sum of underlying surface/backend samples rather than drifting into disconnected counters

## Files Most Likely To Change

- `crates/core/foundation/debug/src/lib.rs`
- `crates/core/shell/src/shell/runtime/profiling.rs`
- `crates/core/shell/src/shell/runtime/debug.rs`
- `crates/core/shell/src/shell/runtime/mod.rs`
- `crates/core/shell/src/shell/runtime/request.rs`
- `crates/core/shell/src/shell/runtime/service_state.rs`
- `crates/core/shell/src/shell/backend/spawn.rs`
- `crates/core/shell/src/shell/runtime/render.rs`
- `crates/core/shell/src/shell/tests.rs`

## Risks And Mitigations

| Risk | Mitigation |
|------|------------|
| Backend attribution collapses back into one aggregate bucket hidden inside shell-wide samples | Add a first-class backend profiling section with explicit interface/provider/stage identity. |
| The shell reports backend work for stale or inactive providers | Reuse `record_latest_service_state(...)` acceptance logic and active-provider ownership before recording accepted samples. |
| Phase 15 accidentally promises end-to-end frontend/backend causality that belongs to Phase 17 | Keep this phase focused on stable per-surface and per-provider stage rollups, not benchmark-flow correlation. |
| Added backend profiling storage becomes unbounded or duplicates collector logic | Extend the existing Phase 14 bounded collector rather than adding a second retention system. |
| Command attribution becomes misleading because results are asynchronous | Record the command-handling stage at the shell-owned dispatch seam and document that deeper interaction correlation remains Phase 17 scope. |

## Validation Architecture

### Test Layers

1. Shell profiling unit tests in `mesh-core-shell`
   - prove backend snapshots and stage enums appear only when profiling is enabled
   - prove provider/interface attribution for update, command, and delivery stages

2. Snapshot rollup tests in `mesh-core-shell`
   - prove per-surface breakdowns remain stable and comparable with shell totals
   - prove backend snapshots sort deterministically by interface/provider

3. Focused grep/contract checks
   - prove the typed backend profiling contract exists in `mesh-core-debug`

### Commands

- Quick command: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_`
- Full command: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell`

## Planning Notes

- The most important Phase 15 deliverable is not a UI. It is a stable profiling contract that names backend hotspots by provider/service stage and preserves per-surface render costs as first-class evidence.
- The cleanest implementation path is contract first, then collector/storage extension, then shell seam instrumentation, then regression proof.
- The phase should reuse the same debug-only profiling boundaries already locked in Phase 14 rather than introducing any new always-on diagnostics path.

## RESEARCH COMPLETE
