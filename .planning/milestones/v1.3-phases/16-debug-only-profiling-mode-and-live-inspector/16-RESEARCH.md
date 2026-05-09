---
phase: 16
slug: debug-only-profiling-mode-and-live-inspector
status: complete
created: 2026-05-08
---

# Phase 16 Research: Debug-Only Profiling Mode and Live Inspector

## Research Complete

Phase 16 does not need a new profiler data model. Phases 14 and 15 already established the hard parts: profiling remains debug-only, snapshots are bounded, shell/per-surface/backend summaries are typed, and the shell debug path already toggles profiling independently from overlay visibility. The missing work is the delivery path and presentation layer: replacing the current native debug panel with a shell-shipped `.mesh` inspector surface that consumes the existing debug/profiling snapshot data without creating a parallel diagnostics mode.

Phase 16 therefore should be planned as an integration phase across four seams:

- debug activation and shell state ownership
- shell-to-inspector data exposure
- shell-owned loading/lifecycle of the inspector module
- `.mesh` inspector UI views for overview, surfaces, backend services, and benchmark scaffolding

## Current State

### The debug path already owns activation state

- `crates/core/foundation/debug/src/lib.rs`
  - `DebugOverlayState` already owns:
    - `enabled`
    - `active_tab`
    - `profiling_enabled`
    - `profiling_session_id`
  - `DebugSnapshot` already carries:
    - modules
    - interfaces
    - backend runtime health
    - active surfaces
    - optional profiling payload
- `crates/core/shell/src/shell/runtime/request.rs`
  - `CoreRequest::ToggleDebugOverlay`
  - `CoreRequest::ToggleDebugProfiling`
  - `CoreRequest::CycleDebugTab`
  - profiling enable resets the collector for a new session but does not auto-open any new UI
- `crates/core/shell/src/shell/ipc.rs`
  - already exposes:
    - `shell:debug_overlay`
    - `shell:debug_profiling`
    - `shell:debug_cycle_tab`

This matches the locked Phase 16 decisions already:

- profiling stays inside the existing debug path
- profiling is explicitly toggled
- profiling collection is independent from inspector visibility

The current contract should be extended, not replaced.

### The profiling payload is already rich enough for an inspector

- `crates/core/foundation/debug/src/lib.rs`
  - `ProfilingSnapshot` already includes:
    - `shell`
    - `surfaces`
    - `backends`
- `crates/core/shell/src/shell/runtime/debug.rs`
  - already builds one deterministic `DebugSnapshot`
  - already sorts `surfaces` and `backends`
  - already emits profiling only when `profiling_enabled` is true

This means Phase 16 should not invent a second inspector-specific profiling format. The inspector should consume the same `DebugSnapshot`/`ProfilingSnapshot` contract the shell already owns.

### The existing UI host is still a native renderer

- `crates/core/ui/render/src/surface/debug_overlay.rs`
  - paints a native right-side panel directly into the pixel buffer
  - current tabs are hard-coded to:
    - `Modules`
    - `Interfaces`
    - `Health`
  - layout, typography, palette, and row rendering are all native-only

This is the direct replacement target for `INSP-01`. Phase 16 must move this panel to a `.mesh` frontend surface instead of layering more native diagnostics code into the renderer.

### The repo already has the right `.mesh` packaging patterns

- `modules/frontend/navigation-bar/module.json`
  - shows a shipped shell-owned surface manifest pattern
- `modules/frontend/navigation-bar/src/main.mesh`
  - shows normal `.mesh` composition, shell event publishing, service binding, and responsive styling
- `modules/frontend/audio-popover/module.json`
  - shows companion shell-owned surface packaging for a smaller secondary UI
- `modules/frontend/audio-popover/src/main.mesh`
  - shows state-driven fallback UX and compact interaction patterns

Phase 16 should reuse these patterns so the inspector behaves like a normal module package, even if the shell loads it internally only while debug mode is active.

### Test anchors already exist at the shell and real-surface layers

- `crates/core/shell/src/shell/tests.rs`
  - already holds profiling/debug path regression proof
- `crates/core/shell/src/shell/component/tests.rs`
  - already exercises real shipped/frontend component behavior and is the likely home for inspector-surface integration proof

That suggests a two-layer proof strategy:

- shell tests for debug state, request handling, snapshot exposure, and disabled-mode silence
- surface/component tests for the shipped inspector module/package contract and empty-state tolerance

## Recommended Implementation Shape

### 1. Treat the inspector as a shell-shipped frontend module, not a renderer feature

The cleanest Phase 16 shape is:

- keep shell ownership of debug/profiling state in Rust
- introduce a core-shipped frontend inspector module/package under `modules/frontend/`
- load that module only while debug mode is active
- mount it in the familiar right-side panel placement so the visible interaction model stays consistent

This satisfies both sides of the phase boundary:

- shell-owned and debug-only by distribution
- normal `.mesh` surface/module in implementation style

### 2. Replace tab-specific native rendering with shell-exposed debug data

The native overlay currently mixes:

- panel chrome
- active-tab state
- rendering logic for modules/interfaces/health

Phase 16 should split those concerns:

- the shell remains the source of truth for debug state and snapshot payloads
- the inspector `.mesh` surface renders the UI
- the shell exposes the current debug/profiling payload through one debug-facing API shape usable by the built-in inspector and, in principle, by user-authored modules

That API should expose at least:

- debug overlay visibility or mounted-state context
- profiling enabled/session status
- modules/interfaces/health data from `DebugSnapshot`
- profiling shell/surface/backend summaries from `ProfilingSnapshot`

### 3. Keep profiling activation independent from inspector visibility

Do not collapse these into one action.

Recommended behavior:

- `shell:debug_overlay`
  - shows or hides the inspector surface
- `shell:debug_profiling`
  - starts or stops collection and resets session state on enable
- inspector view switching
  - local to the inspector UI state or shell debug state, but must not implicitly enable profiling

This preserves `PROF-01` and the locked discuss-phase decisions.

### 4. Ship the required view set as information architecture, not feature creep

The phase goal requires four views:

- overview
- surfaces
- backend services
- benchmark/interaction

The benchmark/interaction view should remain scaffolded in Phase 16:

- define benchmark categories
- describe what each category measures
- show empty/pending states when no real benchmark execution data exists yet

Do not add actual scenario launch/proof automation here. That belongs to Phase 17.

### 5. Design empty and sparse states as first-class behavior

`INSP-03` is not just defensive coding. It should drive the data model for the UI:

- no active profiling session
  - inspector explains profiling is off and how to enable it
- profiling on but no samples yet
  - overview/surfaces/backend views show stable zero-state summaries
- services or surfaces with no recent data
  - render explicit empty rows or helper copy, not broken layouts or missing containers
- benchmark scaffold before Phase 17
  - render category cards with measurement intent and “not yet recorded” style messaging

### 6. Preserve a clean boundary between lifecycle health and profiling data

`DebugSnapshot` already contains:

- `interfaces`
- `backend_runtimes`
- `health`
- `profiling`

The inspector should present these as related but separate concerns:

- runtime/provider health is not the same thing as profiling cost
- profiling backend timing is not a replacement for interface/provider lifecycle status

This avoids muddying Phase 15 attribution work and keeps the inspector legible.

## Files Most Likely To Change

- `crates/core/foundation/debug/src/lib.rs`
- `crates/core/shell/src/shell/runtime/debug.rs`
- `crates/core/shell/src/shell/runtime/request.rs`
- `crates/core/shell/src/shell/ipc.rs`
- `crates/core/shell/src/shell/types.rs`
- `crates/core/ui/render/src/surface/debug_overlay.rs`
- `crates/core/shell/src/shell/tests.rs`
- `crates/core/shell/src/shell/component/tests.rs`
- `modules/frontend/<new-inspector-module>/module.json`
- `modules/frontend/<new-inspector-module>/src/main.mesh`
- `modules/frontend/<new-inspector-module>/src/components/*.mesh`

## Risks And Mitigations

| Risk | Mitigation |
|------|------------|
| The implementation keeps bolting profiling UI onto `debug_overlay.rs`, leaving a native diagnostics subsystem in place | Plan a dedicated replacement step that moves panel rendering responsibility into a `.mesh` module and reduces the native renderer to mount/host responsibilities only, or removes it entirely if the shell can mount the inspector directly. |
| Profiling becomes implicitly enabled whenever the inspector opens | Keep `ToggleDebugOverlay` and `ToggleDebugProfiling` as separate flows and add shell tests proving overlay visibility does not mutate profiling state. |
| The inspector needs a private data path that user-authored modules could not reuse | Expose debug/profiling data through a shell-owned debug API contract and treat the built-in inspector as a reference consumer. |
| The benchmark view leaks Phase 17 work into Phase 16 | Limit the Phase 16 benchmark surface to category definitions, explanatory copy, and empty states only. |
| Sparse profiling data produces unstable layouts or crashes in `.mesh` components | Make zero-state and empty collections a first-class test target in both shell and component tests. |
| The replacement drops useful current debug content like modules/interfaces/health while chasing profiling-only UI | Keep the current debug snapshot categories available in the inspector alongside profiling views so the Phase 16 replacement is truly a debug panel replacement, not a narrower profiler-only tool. |

## Validation Architecture

### Test Layers

1. Shell debug/profiling tests in `mesh-core-shell`
   - prove overlay visibility and profiling enable remain separate
   - prove debug snapshot exposure still stays silent when profiling is off
   - prove inspector-facing debug snapshot data stays deterministic and complete

2. Frontend/component integration tests in `mesh-core-shell`
   - prove the inspector surface/module loads like a normal `.mesh` surface
   - prove overview/surfaces/backend/benchmark scaffold states render with sparse data
   - prove replacing the native panel does not regress shipped-surface patterns

3. Manual debug-path verification
   - toggle the debug overlay
   - toggle profiling independently
   - inspect right-panel behavior with and without live samples
   - confirm empty benchmark scaffolding stays stable

### Commands

- Quick shell-focused command: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell debug_`
- Quick profiling-focused command: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_`
- Full package command: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell`

## Planning Notes

- The plan should start with the shell contract and mounting path, not the visual details. Without the data bridge and lifecycle model, the `.mesh` UI will be forced into ad hoc native hooks.
- The implementation should be split so the inspector can land incrementally:
  - shell-exposed debug/profiling state and mount path
  - inspector module/package scaffold
  - overview/surfaces/backend views
  - benchmark scaffold and regression proof
- A Phase 16 UI-SPEC is required before planning because the phase is explicitly a frontend/debug-panel replacement. The workflow should not proceed to `PLAN.md` generation without that design contract.

## RESEARCH COMPLETE
