---
phase: 16-debug-only-profiling-mode-and-live-inspector
plan: 02
subsystem: debug
tags: [profiling, inspector, mesh, shell, surface]
requires:
  - phase: 16-01
    provides: shell-owned `mesh.debug` state, inspector view ids, and debug control events
provides:
  - shell-shipped `@mesh/debug-inspector` surface manifest and `.mesh` host
  - debug overlay toggles routed to inspector surface visibility without changing profiling state
  - native debug overlay reduced to layout-bounds-only rendering
affects: [phase-16-inspector-data-views, phase-17-benchmarks, debug-overlay]
tech-stack:
  added: []
  patterns: [shell-shipped built-in surface bypasses package graph filter, overlay visibility owned by the inspector surface]
key-files:
  created: [modules/frontend/debug-inspector/module.json, modules/frontend/debug-inspector/src/main.mesh]
  modified:
    [
      crates/core/shell/src/shell/discovery.rs,
      crates/core/shell/src/shell/runtime/request.rs,
      crates/core/shell/src/shell/runtime/render.rs,
      crates/core/ui/render/src/surface/debug_overlay.rs,
      crates/core/shell/src/shell/tests.rs
    ]
key-decisions:
  - "Kept `@mesh/debug-inspector` shell-shipped by force-including it in frontend discovery even when `config/package.json` does not enable it."
  - "Made `CoreRequest::ToggleDebugOverlay` synchronize `@mesh/debug-inspector` visibility directly instead of preserving any native panel paint path."
patterns-established:
  - "Built-in debug surfaces should mount through the normal frontend catalog and surface lifecycle rather than private renderer UI."
  - "Debug overlay visibility and profiling enable remain separate shell-owned states, even when the inspector surface is the only visible debug UI."
requirements-completed: [PROF-01, INSP-01]
duration: 8min
completed: 2026-05-08
---

# Phase 16 Plan 02: Shell-Shipped Debug Inspector Module and Overlay Mount Path Summary

**A built-in `@mesh/debug-inspector` `.mesh` surface now owns the right-side debug panel path while profiling remains explicitly opt-in**

## Performance

- **Duration:** 8 min
- **Started:** 2026-05-08T18:30:30Z
- **Completed:** 2026-05-08T18:38:38Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments

- Added the shell-shipped `@mesh/debug-inspector` module manifest with fixed right-edge overlay defaults and a `.mesh` inspector host surface.
- Routed `ToggleDebugOverlay` to the inspector surface lifecycle and kept profiling state independent from overlay visibility.
- Removed native debug panel painting from the renderer, leaving layout-bounds overlays as the only native debug overlay responsibility, and covered the path with focused shell regressions.

## Task Commits

1. **Task 1: Create the built-in `@mesh/debug-inspector` module package with fixed right-panel surface settings** - `c52531b` (`feat`)
2. **Task 2: Route debug overlay visibility to the `.mesh` inspector surface and retire native panel content painting** - `a55a095` (`feat`)

## Files Created/Modified

- `modules/frontend/debug-inspector/module.json` - Shell-owned surface manifest for the shipped inspector panel.
- `modules/frontend/debug-inspector/src/main.mesh` - Inspector host UI with profiling controls and Overview, Surfaces, Backend, and Benchmark sections.
- `crates/core/shell/src/shell/discovery.rs` - Keeps the built-in inspector available even when the installed frontend graph omits it.
- `crates/core/shell/src/shell/runtime/request.rs` - Synchronizes debug overlay toggles with `@mesh/debug-inspector` surface visibility.
- `crates/core/shell/src/shell/runtime/render.rs` - Stops invoking the native debug panel painter during normal surface renders.
- `crates/core/ui/render/src/surface/debug_overlay.rs` - Retains only layout-bounds painting for the native overlay helper.
- `crates/core/shell/src/shell/tests.rs` - Adds regressions for inspector loading and overlay visibility behavior.

## Decisions Made

- The built-in inspector stays outside the normal package-graph enablement check so debug tooling remains shell-shipped rather than user-install dependent.
- Direct overlay toggles now own the inspector surface visibility contract; the native renderer no longer paints a second panel UI.

## Deviations from Plan

### Auto-fixed Issues

None.

### Ownership Constraint

- **Issue:** Standard GSD execution flow would also update `.planning/STATE.md`, `.planning/ROADMAP.md`, and `.planning/REQUIREMENTS.md`.
- **Resolution:** Left those files untouched because task ownership was limited to the seven source files above plus this summary file.
- **Impact on plan:** No impact on code delivery or verification; only global planning metadata updates were deferred.

## Issues Encountered

- A transient `.git/index.lock` blocked one staging command during task 2. Retrying once the lock disappeared was sufficient; no manual cleanup was required.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 16-03 can now wire richer live data and sparse-state rendering into the shipped inspector without revisiting overlay ownership.
- The benchmark view is intentionally scaffold-only, which keeps Phase 17 free to define the repeatable benchmark flows separately.

## Known Stubs

- `modules/frontend/debug-inspector/src/main.mesh:121` - Overview copy is still scaffold text pending richer live metric cards in Plan 16-03.
- `modules/frontend/debug-inspector/src/main.mesh:122` - Surfaces copy is still scaffold text pending per-surface rollup rendering in Plan 16-03.
- `modules/frontend/debug-inspector/src/main.mesh:123` - Backend copy is still scaffold text pending backend/service detail rendering in Plan 16-03.

