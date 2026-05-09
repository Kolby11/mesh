# Phase 17: Canonical Benchmark Scenarios and Proof Flows - Research

**Researched:** 2026-05-09
**Status:** Complete

## Research Question

What does Phase 17 need in order to plan repeatable benchmark/proof flows on top of the existing debug-only profiling pipeline without expanding scope into persistence, trace replay, compositor E2E, or Phase 18 optimization work?

## Source Context

- `.planning/phases/17-canonical-benchmark-scenarios-and-proof-flows/17-CONTEXT.md`
- `.planning/REQUIREMENTS.md`
- `.planning/STATE.md`
- `.planning/ROADMAP.md`
- `.planning/phases/14-profiling-data-model-and-timing-hooks/14-CONTEXT.md`
- `.planning/phases/15-backend-and-surface-attribution/15-VERIFICATION.md`
- `.planning/phases/16-debug-only-profiling-mode-and-live-inspector/16-CONTEXT.md`
- `.planning/phases/13-navigation-bar-rendering-proof/13-CONTEXT.md`
- `.planning/codebase/TESTING.md`
- `.planning/codebase/ARCHITECTURE.md`
- `.planning/codebase/INTEGRATIONS.md`

## Existing Implementation Facts

### Debug and Profiling Contract

- `crates/core/foundation/debug/src/lib.rs` already defines `DebugSnapshot`, `DebugInspectorView::Benchmark`, `ProfilingSnapshot`, `ProfilingScopeSnapshot`, `ProfilingSurfaceSnapshot`, and `ProfilingBackendSnapshot`.
- `ProfilingStage` already covers `input_handling`, `runtime_update_handling`, `tree_build`, `style_restyle`, `layout`, `paint`, `present_commit`, `redraw_count`, and `total_surface_render`.
- `ProfilingBackendStage` already covers `poll_update`, `command_handling`, and `state_publish_delivery`.
- `crates/core/shell/src/shell/runtime/debug.rs` serializes `mesh.debug` state into JSON consumed by the `.mesh` debug inspector.
- `crates/core/shell/src/shell/runtime/profiling.rs` owns bounded rolling sample storage and records shell, surface, and backend samples only when `self.debug.profiling_enabled` is true.
- `crates/core/shell/src/shell/runtime/request.rs` records `runtime_update_handling` around accepted `CoreRequest` handling and resets profiling data when `ToggleDebugProfiling` turns profiling on.

### Inspector and Surface Anchors

- `modules/frontend/debug-inspector/src/main.mesh` already consumes `@mesh/debug@>=1.0`, displays profiling state, and switches among overview, surfaces, backend services, and benchmark views.
- `modules/frontend/debug-inspector/src/components/benchmark-view.mesh` is scaffold-only and lists the five Phase 17 categories: hover, surface open/close, pointer-driven update, keyboard traversal, and backend-driven update.
- `modules/frontend/navigation-bar/src/main.mesh` is the primary shipped shell chrome surface and is the best anchor for hover and keyboard traversal proof.
- `modules/frontend/audio-popover/src/main.mesh` is the natural shipped popover/control surface for surface open/close and pointer-driven update proof.
- Existing shell events already include `shell.toggle-surface`, `shell.show-surface`, `shell.hide-surface`, `shell.activate-popover`, and `shell.toggle-debug-profiling`; benchmark work should reuse or layer on these rather than creating an unrelated control path.

### Backend Correlation

- Phase 15 verified backend provider/stage attribution in `crates/core/shell/src/shell/runtime/profiling.rs`, `request.rs`, and `service_state.rs`.
- `broadcast_service_event` records `state_publish_delivery` for accepted service updates and includes the `source_module` provider id.
- `dispatch_service_command` records provider-aware `command_handling` for service commands.
- `mesh.audio` via `@mesh/pipewire-audio` is the canonical existing provider identity to use in automated backend-driven proof, but implementation must remain generic and must not add audio-specific Rust behavior.

### Test Patterns

- `crates/core/shell/src/shell/tests.rs` already contains shell-level profiling tests for disabled-mode silence, profiling reset, per-surface rollups, backend samples by provider, command handling, poll/update, and state publish/delivery.
- `crates/core/shell/src/shell/component/tests.rs` already contains real-surface debug-inspector tests, including scaffold proof that the benchmark view renders all five Phase 17 labels.
- Project test convention is focused Rust behavior tests under the relevant module, run through `nix develop -c cargo test`.

## Recommended Technical Shape

### 1. Add a Typed Benchmark Result Contract

Extend `mesh_core_debug` with benchmark scenario/result data that hangs off the existing debug snapshot, for example:

- `DebugBenchmarkSnapshot`
- `BenchmarkScenarioSnapshot`
- `BenchmarkScenarioId`
- `BenchmarkScenarioStatus`
- `BenchmarkScenarioResult`

The exact names are planner discretion, but the contract should expose at least:

- Stable ids: `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, `backend_update`
- Labels matching the inspector categories
- Target surface/module identity
- Current status: unavailable, ready, running, complete, or skipped
- Relevant shell stage summaries
- Relevant surface stage summaries
- Relevant backend stage summaries for `backend_update`
- Optional guidance/error copy for unavailable or skipped scenarios

Keep this live and session-scoped. Do not add history, files, exports, or trace persistence.

### 2. Assemble Benchmark State in the Existing Debug Snapshot Path

The lowest-risk integration point is `Shell::build_debug_snapshot()` in `crates/core/shell/src/shell/runtime/debug.rs`.

Recommended approach:

- Keep scenario definitions static and shell-owned.
- Derive latest benchmark display state from current `ProfilingSnapshot`, `active_surfaces`, and backend runtime state.
- Serialize benchmark data inside `debug_service_payload` alongside existing `profiling`, not through a new service.
- Preserve `profiling: null` while profiling is disabled.

If launch/run state is needed, add small debug-owned runtime state rather than using long-lived persistence.

### 3. Add Explicit Benchmark Requests Only If Needed

Phase 17 requires repeatable launch/proof flows, not necessarily a full benchmark scheduler.

Reasonable request additions:

- `CoreRequest::RunDebugBenchmark { scenario_id: String }`
- Optional IPC command such as `shell:debug_benchmark:<scenario_id>` only if the planner wants CLI parity.
- `.mesh` event channel such as `shell.run-debug-benchmark` from the inspector.

Keep requests generic. The shell may execute existing actions such as `ToggleSurface`, `ActivatePopover`, or focus transfer for the chosen target, then report results through the same profiling snapshot data.

If the first implementation can satisfy repeatability with scenario definitions plus inspector controls that trigger existing shell requests, avoid adding a heavier scheduler.

### 4. Convert Benchmark View from Scaffold to Control/Result Panel

Update `modules/frontend/debug-inspector/src/components/benchmark-view.mesh` and the parent `main.mesh` so the benchmark view receives benchmark data from `debug_service.benchmarks` or an equivalent field.

Expected UI states:

- Profiling off: scenarios visible but runnable controls explain profiling must be enabled.
- Profiling live/no samples: scenarios visible with ready or waiting state.
- Scenario complete: row shows target and timing summaries.
- Scenario unavailable: row remains stable and explains why, for example no `@mesh/audio-popover` surface or no backend provider samples.

Avoid card-heavy marketing layout. This is an operational debug surface, so dense rows with labels, status, compact timing summaries, and action buttons are the right fit.

### 5. Keep Backend-Driven Correlation Generic

For `BENCH-05` / `BACK-03`, the proof must show both backend stage timing and resulting frontend render cost for the same backend-driven scenario.

Implementation should:

- Use existing backend stage summaries keyed by `(interface, provider_id)`.
- Use existing surface summaries keyed by `surface_id` and `module_id`.
- Associate the backend-driven scenario with `mesh.audio`, `@mesh/pipewire-audio`, and the visible frontend surface in benchmark result state.
- Avoid special-case Rust logic for audio payload parsing or command semantics.

## Candidate Plan Breakdown

### Plan 17-01: Benchmark Contract and Debug Snapshot Shape

Build the typed scenario/result model and serialize it into `mesh.debug`.

Likely files:

- `crates/core/foundation/debug/src/lib.rs`
- `crates/core/shell/src/shell/runtime/debug.rs`
- `crates/core/shell/src/shell/tests.rs`

Requirements: `BENCH-01`, `BENCH-02`, `BENCH-03`, `BENCH-04`, `BENCH-05`

### Plan 17-02: Benchmark Launch Requests and Scenario Execution Hooks

Add explicit benchmark run handling where needed, mapping scenario ids to existing shell actions and trigger kinds.

Likely files:

- `crates/core/foundation/debug/src/lib.rs`
- `crates/core/shell/src/shell/types.rs`
- `crates/core/shell/src/shell/service.rs`
- `crates/core/shell/src/shell/runtime/request.rs`
- `crates/core/shell/src/shell/ipc.rs`
- `crates/core/shell/src/shell/tests.rs`

Requirements: `BENCH-01`, `BENCH-02`, `BENCH-03`, `BENCH-04`

### Plan 17-03: Inspector Benchmark UI

Turn the scaffold view into compact runnable rows and latest-run summaries.

Likely files:

- `modules/frontend/debug-inspector/src/main.mesh`
- `modules/frontend/debug-inspector/src/components/benchmark-view.mesh`
- `crates/core/shell/src/shell/component/tests.rs`

Requirements: `BENCH-01`, `BENCH-02`, `BENCH-03`, `BENCH-04`, `BENCH-05`

### Plan 17-04: Backend Correlation and Final Proof

Prove backend-driven scenario result state shows provider-stage timing plus resulting frontend surface cost.

Likely files:

- `crates/core/shell/src/shell/runtime/debug.rs`
- `crates/core/shell/src/shell/runtime/profiling.rs`
- `crates/core/shell/src/shell/tests.rs`
- `crates/core/shell/src/shell/component/tests.rs`

Requirements: `BACK-03`, `BENCH-05`

## Risks and Constraints

- The current profiler stores rolling samples, not scenario-scoped traces. Plans must not require perfect end-to-end trace correlation unless they also add a bounded session-local latest-run mechanism.
- Full wall-clock benchmark accuracy is not required in Phase 17. Repeatable scenario identity and comparable stage summaries are the acceptance target.
- UI-SPEC is likely needed before final planning because the phase modifies the debug inspector benchmark view. The safety gate should not be bypassed unless the user explicitly chooses `--skip-ui`.
- Backend-driven proof must not add service-specific command behavior to Rust core.
- Compositor-level timing and Wayland E2E automation are out of scope.
- Persistent benchmark history belongs outside Phase 17.

## Validation Architecture

### Must-Have Truths

1. `mesh.debug` exposes five stable benchmark scenarios with ids `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, and `backend_update`.
2. Each benchmark scenario result includes a stable target surface/module identity and at least one relevant profiling summary when data exists.
3. Hover scenario output references input, restyle/style, and render timing buckets.
4. Surface open/close scenario output references total surface render time and redraw activity.
5. Pointer-driven update scenario output references input-to-visible-response relevant shell/surface timing buckets.
6. Keyboard traversal scenario output references focus/input/render timing buckets.
7. Backend-driven update scenario output references backend `poll_update`, `command_handling`, or `state_publish_delivery` timing plus resulting frontend surface render cost.
8. Benchmark state remains absent or inert when profiling is disabled and does not make profiling run by default.
9. The debug inspector benchmark view renders stable empty/unavailable states and a populated result state without breaking the real `.mesh` surface.

### Automated Checks

- Add or update shell tests in `crates/core/shell/src/shell/tests.rs` for benchmark scenario ids, debug payload JSON shape, profiling-disabled behavior, and backend/surface correlation.
- Add or update real-surface component tests in `crates/core/shell/src/shell/component/tests.rs` for benchmark rows, controls, disabled state, unavailable state, and populated result state.
- Focused verification commands:
  - `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark`
  - `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell debug_inspector`
  - `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_`

### Manual/Human Checks

None required for Phase 17 planning. Compositor-level live feel checks may be useful during execution, but the phase should be planned so automated shell/component tests prove the canonical contract.

## Research Conclusion

Phase 17 should be planned as a bounded extension of the existing debug service and inspector: first define benchmark scenario/result shape, then add explicit launch/result handling, then update the inspector benchmark view, then prove backend-to-frontend correlation. The implementation should reuse the current profiling collector and shell/component test patterns rather than building persistence, trace replay, a new profiler entrypoint, or an E2E harness.

## RESEARCH COMPLETE
