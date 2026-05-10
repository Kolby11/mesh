---
phase: 26-cpu-render-profiling-and-baseline-proof
plan: 01
subsystem: testing
tags: [profiling, benchmark, retained-rendering, cpu, shell, debug-json]
requires:
  - phase: 17-performance-instrumentation-and-responsiveness
    provides: canonical benchmark scenarios and debug profiling payload
  - phase: 21-retained-render-objects
    provides: retained render-object synchronization boundary
provides:
  - retained CPU render attribution stages for render-object sync, display-list update, traversal, text shaping, and icon/image raster work
  - stable benchmark baseline proof artifact for the five shipped canonical scenarios
  - debug JSON serialization coverage for the extended profiling payload
affects: [27, 28, 29, 30, 31, retained-rendering, benchmark-proof]
tech-stack:
  added: []
  patterns: [extend mesh.debug.profiling in-place, record retained CPU substages as ProfilingStage entries]
key-files:
  created:
    - .planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md
    - crates/core/frontend/render/src/surface/profiling.rs
  modified:
    - crates/core/foundation/debug/src/lib.rs
    - crates/core/frontend/render/src/surface/mod.rs
    - crates/core/frontend/render/src/surface/text.rs
    - crates/core/frontend/render/src/surface/icon.rs
    - crates/core/frontend/render/src/surface/glyph.rs
    - crates/core/shell/src/shell/component/shell_component.rs
    - crates/core/shell/src/shell/runtime/debug.rs
    - crates/core/shell/src/shell/tests.rs
    - crates/core/shell/src/shell/component/tests.rs
key-decisions:
  - "Retained CPU attribution extends the existing ProfilingStage surface instead of introducing a second benchmark or trace system."
  - "Text shaping proof is exposed both as a stage label and as text.shaping_micros in the invalidation payload."
  - "Shared STATE.md and ROADMAP.md remain untouched because this plan executed in an isolated workspace with a pending orchestrator phase-start update."
patterns-established:
  - "Retained paint substage timing: record coarse Paint plus nested RenderObjectSync, RetainedDisplayListUpdate, PaintTraversal, TextShaping, and IconImageRaster samples."
  - "Phase-local proof artifacts: capture reusable benchmark/baseline contracts under the phase directory when shared planning state should stay unchanged."
requirements-completed: [PERF-01, PERF-02]
duration: 12min
completed: 2026-05-10
---

# Phase 26 Plan 01: CPU Render Profiling and Baseline Proof Summary

**Retained CPU render hotspots are now attributed through the existing `mesh.debug.profiling` path, with stable canonical benchmark targets and a reusable Phase 26 baseline proof artifact.**

## Performance

- **Duration:** 12 min
- **Started:** 2026-05-10T20:46:13Z
- **Completed:** 2026-05-10T20:58:52Z
- **Tasks:** 2
- **Files modified:** 15

## Accomplishments

- Added retained CPU render attribution stages for render-object sync, retained display-list update, paint traversal, text shaping, and icon/image raster work without changing the benchmark harness.
- Threaded the new attribution through the shell debug payload and invalidation JSON, including `text.shaping_micros` for direct shell-consumer inspection.
- Recorded the five shipped canonical scenario targets and the Phase 26 before/after profiling surface in a reusable baseline proof artifact.

## Task Commits

Each task was committed atomically:

1. **Task 1: Add retained CPU attribution and coverage** - `aada3c3` (`feat`)
2. **Task 2: Record canonical benchmark baseline proof** - `774f858` (`docs`)

**Plan metadata:** pending summary commit

## Files Created/Modified

- `crates/core/foundation/debug/src/lib.rs` - extended profiling enums and text snapshot metadata for retained CPU substages.
- `crates/core/frontend/render/src/surface/profiling.rs` - added render-local raster timing aggregation used by icon/image profiling.
- `crates/core/frontend/render/src/surface/mod.rs` - returned paint profiling metrics for retained display-list paints.
- `crates/core/frontend/render/src/surface/text.rs` - measured text shaping time on cache-miss layout/shaping work.
- `crates/core/frontend/render/src/surface/icon.rs`, `crates/core/frontend/render/src/surface/glyph.rs` - recorded icon/image raster timing across file, SVG, and glyph-backed icon paints.
- `crates/core/shell/src/shell/component/shell_component.rs` - recorded retained render-object sync, display-list update, traversal, shaping, and raster stages on the shipped shell paint path.
- `crates/core/shell/src/shell/runtime/debug.rs` - serialized the extended profiling payload into `mesh.debug`.
- `crates/core/shell/src/shell/tests.rs`, `crates/core/shell/src/shell/component/tests.rs` - locked the stable benchmark rows, new stage serialization, and retained paint-path attribution with focused tests.
- `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md` - captured the canonical benchmark baseline contract for later v1.5 optimization phases.

## Decisions Made

- Extended the existing debug profiling path instead of inventing a parallel benchmark/profiling mechanism, matching the milestone constraint.
- Kept the benchmark scenario IDs and shipped targets unchanged; the new attribution lives under `mesh.debug.profiling`, not in renamed benchmark rows.
- Left `.planning/STATE.md` and `.planning/ROADMAP.md` untouched in this isolated execution to avoid conflicting with the orchestrator’s pending shared-state update.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- `cargo test` outside the Nix dev shell failed because `xkbcommon.pc` was unavailable for `smithay-client-toolkit`. Verification was rerun successfully with `nix develop -c cargo ...`, which matches the repository testing conventions.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Later v1.5 optimization phases can now compare before/after work against the same five benchmark IDs and shipped targets while inspecting the retained CPU substages through `mesh.debug.profiling`.
- This isolated execution captured the reusable profiling/baseline contract, not compositor-captured live smoothness measurements; later optimization phases should attach their own before/after timing samples to the same benchmark rows when behavior changes.

## Self-Check: PASSED

- Verified `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md` and `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-SUMMARY.md` exist on disk.
- Verified task commits `aada3c3` and `774f858` exist in git history.
