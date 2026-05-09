---
phase: 14
slug: profiling-data-model-and-timing-hooks
status: complete
created: 2026-05-08
---

# Phase 14 Research: Profiling Data Model and Timing Hooks

## Research Complete

Phase 14 is a shell-runtime instrumentation phase, not a UI-feature phase. The codebase already has a debug-only snapshot boundary (`mesh-core-debug` plus `Shell::build_debug_snapshot()`), a central Wayland/input dispatch loop, and a frontend render pipeline whose `build_tree -> restyle -> layout -> paint -> present` shape matches the milestone's required stage model closely enough to instrument without introducing a parallel diagnostics subsystem.

## Current State

### Debug-only shell state already has a stable crate boundary

- `crates/core/foundation/debug/src/lib.rs` defines `DebugSnapshot`, `DebugOverlayState`, and `DebugTab` as the shared debug contract between shell runtime and renderer.
- `DebugOverlayState` currently only tracks overlay visibility, layout-bounds visibility, and the active tab. There is no profiling enable bit or profiling-specific session state yet.
- `crates/core/shell/src/shell/runtime/debug.rs` already centralizes snapshot assembly. This is the cleanest place to convert runtime profiling state into a stable debug snapshot payload for later inspector work.
- `crates/core/ui/render/src/surface/debug_overlay.rs` currently paints only modules, interfaces, health, and active surfaces. Phase 14 should avoid forcing inspector UI scope into this file; new profiling fields can exist in the snapshot without immediately expanding the overlay renderer.

### The control path for debug-only features is already centralized

- `crates/core/shell/src/shell/types.rs` defines `CoreRequest::ToggleDebugOverlay` and `CoreRequest::CycleDebugTab`, which are the current shell-owned debug controls.
- `crates/core/shell/src/shell/runtime/request.rs` applies those requests by mutating `self.debug`.
- `crates/core/shell/src/shell/ipc.rs` exposes debug commands through the running shell IPC server.
- `crates/tools/cli/src/main.rs` currently maps `mesh-shell debug` to `shell:debug_overlay`.
- This means Phase 14 can keep profiling debug-only by extending the same request and IPC path rather than inventing a normal settings/config surface.

### The runtime loop already exposes the milestone's top-level timing boundaries

- `crates/core/shell/src/shell/runtime/mod.rs` runs the main loop in a stable order: theme/locale/settings reload, module reload, Wayland event dispatch, IPC/backend message drain, component tick, request drain, throttled commands flush, render, Wayland flush, present pump, sleep.
- `crates/core/shell/src/shell/runtime/wayland.rs` is the right shell-owned seam for `input handling` timing because it receives raw `WindowEvent`s, routes them, invokes `handle_input`, and drains emitted requests.
- `crates/core/shell/src/shell/runtime/request.rs` is the right seam for `runtime update handling` timing because it applies shell-owned state changes and dispatches service commands independent of per-surface rendering.
- `crates/core/shell/src/shell/runtime/render.rs` is the right seam for outer render-loop spans such as total surface render time, present/commit timing, and redraw counts.

### The per-surface render pipeline already contains explicit build/style/layout/paint steps

- `crates/core/shell/src/shell/component/shell_component.rs` splits `render()` and `paint()` for frontend surfaces.
- `render()` currently handles shell-surface layout/configuration visibility work and marks the component clean; it is not the tree/style/layout stage boundary.
- `paint()` calls `build_tree()` and then applies selection, focus pruning, style animations, optional content measurement, metrics publishing, and final buffer paint.
- `crates/core/shell/src/shell/component/rendering.rs::build_tree()` already performs the exact sub-stages Phase 14 needs:
  - `compiled.build_tree_with_state(...)` for tree build
  - `StyleResolver::restyle_subtree(...)` for style/restyle
  - `LayoutEngine::compute_with_measurer(...)` for layout
- This makes `component/rendering.rs` the correct place for fine-grained per-surface stage instrumentation, with `runtime/render.rs` owning shell-wide rollups.

### Existing shell tests already cover adjacent debug and snapshot contracts

- `crates/core/shell/src/shell/tests.rs` already verifies debug shortcuts, IPC parsing, backend lifecycle status in `DebugSnapshot`, and shell-owned request behavior.
- Those tests are the right home for Phase 14 regression coverage around:
  - profiling-disabled silence
  - enable/reset semantics
  - snapshot shape and stable rollups
  - explicit stage totals and redraw counts
- The test suite currently has no profiling contract, so Phase 14 should add shell-level tests before Phase 16 tries to render an inspector on top of unstable data.

## Recommended Implementation Shape

### 1. Extend `mesh-core-debug` instead of creating a profiler-only side channel

Add profiling snapshot/data types directly to `crates/core/foundation/debug/src/lib.rs` and extend `DebugOverlayState` with profiling control state. Keep this crate as the only shared payload contract between shell runtime and debug rendering layers.

Recommended shape:

- `ProfilingSnapshot`
- `ProfilingShellSummary`
- `ProfilingSurfaceSummary`
- `ProfilingStage`
- `ProfilingSample`
- `ProfilingSessionState` or equivalent shell-owned enable/session metadata

Keep the metric model compact and explicit:

- rolling aggregate totals per stage
- bounded recent samples per stage/surface
- small optional context tags only (`surface_id`, `trigger_kind`, stable module id when known, redraw count, timestamp/order)

### 2. Keep runtime collection shell-owned and bounded

Introduce profiling collection state in `mesh-core-shell`, not in frontend modules and not in renderer-global state. The collector should:

- be fully inert when profiling is disabled
- reset the current session when profiling is enabled
- store fixed-count rings rather than time-window retention
- preserve shell-wide work separately from per-surface work
- allow lightweight trigger tagging now without overcommitting to Phase 15 attribution

This likely wants a dedicated runtime helper module such as `crates/core/shell/src/shell/runtime/profiling.rs` plus one new field on `Shell`.

### 3. Instrument the real stage boundaries where they already exist

For Phase 14's locked stage list:

- `input handling`: `runtime/wayland.rs`
- `runtime update handling`: `runtime/request.rs` plus the main-loop request/message drain in `runtime/mod.rs`
- `tree build`, `style/restyle`, `layout`: `component/rendering.rs`
- `paint`: `component/shell_component.rs::paint()`
- `present/commit`: `runtime/render.rs` around `render_engine.present(...)`
- `total surface render time`: outer per-surface span in `runtime/render.rs`
- `redraw count`: increment at the shell point where a visible surface actually presents a new frame

Do not try to recover these numbers later from logs or traces; measure them directly where the stage executes.

### 4. Add the profiling toggle through the existing debug request path

Phase 14 should make profiling reachable only through the existing debug control path, even if the richer inspector UI waits for Phase 16.

The safest contract is:

- extend `CoreRequest` with an explicit profiling toggle request
- add a matching IPC command and CLI path
- keep overlay visibility independent from profiling enable state
- reset the profiling session when toggled on
- avoid collecting snapshots when profiling is off

### 5. Prove disabled-mode safety and enabled-mode rollups in tests

The milestone requirements for this phase are mostly behavioral contracts, so test coverage should prove:

- profiling-off mode does not populate live profiling snapshot data
- enabling profiling starts a clean session
- shell-wide summaries include the required stage set
- per-surface summaries use `surface_id` as the canonical key
- redraw count and total surface render time are first-class metrics
- hidden/non-rendering surfaces do not appear unless they actually did work

## Files Most Likely To Change

- `crates/core/foundation/debug/src/lib.rs`
- `crates/core/shell/src/shell/mod.rs`
- `crates/core/shell/src/shell/types.rs`
- `crates/core/shell/src/shell/runtime/debug.rs`
- `crates/core/shell/src/shell/runtime/mod.rs`
- `crates/core/shell/src/shell/runtime/request.rs`
- `crates/core/shell/src/shell/runtime/render.rs`
- `crates/core/shell/src/shell/runtime/wayland.rs`
- `crates/core/shell/src/shell/component/rendering.rs`
- `crates/core/shell/src/shell/component/shell_component.rs`
- `crates/core/shell/src/shell/tests.rs`
- `crates/tools/cli/src/main.rs`

## Risks And Mitigations

| Risk | Mitigation |
|------|------------|
| Profiling scope drifts into Phase 16 inspector UI work | Keep Phase 14 focused on runtime contracts, snapshot shape, and testable toggles; avoid requiring overlay renderer changes for plan completion. |
| Instrumentation overhead distorts the very measurements it records | Use a strict profiling-enabled gate, compact sample structs, fixed-count rings, and shell-owned fast paths that allocate minimally when enabled and not at all when disabled. |
| Stage timings become ambiguous because hooks are added at the wrong layer | Instrument `build_tree`, `restyle`, and `layout` in `component/rendering.rs`, and shell/global stages in `runtime/*`, instead of approximating all stages from outer render spans. |
| Surface-local and shell-global work get conflated | Keep separate shell-wide buckets plus per-surface summaries, with shell-global work allowed to exist without a surface key. |
| Session resets or overlay visibility semantics become inconsistent | Put enable/reset/visibility rules in `DebugOverlayState` plus shell request handling and cover them with shell tests. |

## Validation Architecture

### Test Layers

1. Shell runtime tests in `mesh-core-shell`
   - prove toggle/reset semantics
   - prove profiling-disabled silence
   - prove snapshot stage coverage and per-surface rollups

2. Focused runtime aggregation tests in `mesh-core-shell`
   - bounded recent-sample ring behavior
   - redraw count and total-surface timing accounting
   - hidden-surface exclusion unless actual work occurred

3. Narrow CLI / IPC parsing checks
   - confirm profiling control uses the existing debug path rather than normal configuration

### Commands

- Quick command: `nix develop -c cargo test -p mesh-core-shell debug_`
- Full command: `nix develop -c cargo test -p mesh-core-shell debug_ profiling_`

## Planning Notes

- Phase 14 should produce a runtime contract that Phase 15 can attribute and Phase 16 can render, not a temporary prototype format that gets replaced immediately.
- A dedicated profiling helper module in `mesh-core-shell` is justified here because the stage list spans multiple runtime files and needs shared aggregation logic.
- The best proof of success for this phase is not visible UI; it is a stable, bounded, test-covered snapshot model with the right stage boundaries.

## RESEARCH COMPLETE
