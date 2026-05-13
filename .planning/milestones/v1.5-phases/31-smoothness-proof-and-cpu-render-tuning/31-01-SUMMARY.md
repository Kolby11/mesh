---
phase: 31-smoothness-proof-and-cpu-render-tuning
plan: 01
subsystem: rendering
tags: [cpu-rendering, profiling, repaint-policy, smoothness, uat]

requires:
  - phase: 26-cpu-render-profiling-and-baseline-proof
    provides: Canonical shipped-surface baseline proof
  - phase: 29-damage-indexed-paint-execution-and-repaint-policy
    provides: Retained paint filtering and repaint-policy counters
  - phase: 30-raster-cache-hardening-for-icons-images-and-text
    provides: Raster and text cache proof
provides:
  - Phase 31 machine-readable proof rows for all five canonical scenarios
  - Conservative two-thirds full-surface repaint threshold
  - Cache capacity no-change decision based on shipped-surface proof rows
  - UAT and verification artifacts with live visual UAT gap recorded
affects: [v1.5-cpu-rendering, future-skia-gpu-renderer, future-parallel-paint-layout]

tech-stack:
  added: []
  patterns: [existing-profiling-payload, retained-display-list-policy-tests, evidence-gated-tuning]

key-files:
  created:
    - .planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-01-BENCHMARK.md
    - .planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-UAT.md
    - .planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-VERIFICATION.md
  modified:
    - crates/core/shell/src/shell/component/shell_component.rs
    - crates/core/shell/src/shell/component/tests/invalidation/profiling.rs

key-decisions:
  - "Full-surface repaint promotion now uses a two-thirds surface-area threshold while preserving the three-quarters changed-entry tree rebuild fallback."
  - "Raster and text cache capacities remain unchanged because current proof rows do not show capacity-driven misses for warm steady-state shipped-surface paths."
  - "Phase 31 visible smoothness acceptance remains deferred until live visual UAT runs on shipped shell surfaces."

patterns-established:
  - "PHASE31_PROOF rows extend the existing canonical proof command without adding a new benchmark harness."
  - "Counter-only smoothness evidence is recorded as deferred rather than accepted when live UAT is unavailable."

requirements-completed: ["PERF-03", "SMTH-01", "SMTH-02", "SMTH-03"]

duration: 48min
completed: 2026-05-13
---

# Phase 31 Plan 01: Conservative CPU Smoothness Tuning and Shipped-Surface Proof Summary

**Conservative repaint-policy tuning with canonical proof rows, cache no-change evidence, and deferred live visual UAT acceptance**

## Performance

- **Duration:** 48 min
- **Started:** 2026-05-13T15:53:00Z
- **Completed:** 2026-05-13T16:41:21Z
- **Tasks:** 5
- **Files modified:** 6

## Accomplishments

- Added `PHASE31_PROOF` rows to the existing canonical shipped-surface proof command for `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, and `backend_update`.
- Tuned repaint-policy escalation so full-surface repaint starts at two-thirds damage area while retaining the three-quarters changed-entry fallback for tree rebuilds.
- Added focused shell policy tests for minimal, bounding-rect, and full-surface policy choices.
- Recorded evidence that raster and text cache capacities should remain unchanged for this phase.
- Created benchmark, UAT, and verification artifacts that explicitly defer acceptance pending live visual UAT.

## Task Commits

Each task was committed atomically:

1. **Task 31-01-01: Establish Phase 31 benchmark comparison artifact and machine-readable proof rows** - `914f6e9` (`test`)
2. **Task 31-01-02: Create focused manual UAT record for visible smoothness acceptance** - `633b938` (`docs`)
3. **Task 31-01-03: Tune repaint-policy thresholds with display-list correctness guardrails** - `6047143` (`perf`)
4. **Task 31-01-04: Apply evidence-gated cache and clear/background tuning without weakening conservatism** - `0a5d563` (`docs`)
5. **Task 31-01-05: Complete mixed smoothness proof, UAT sign-off, and future-boundary documentation** - `3a1a192` (`docs`)

## Files Created/Modified

- `crates/core/shell/src/shell/component/shell_component.rs` - Added named repaint-policy thresholds and focused policy tests.
- `crates/core/shell/src/shell/component/tests/invalidation/profiling.rs` - Added `PHASE31_PROOF` output rows for canonical scenarios.
- `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-01-BENCHMARK.md` - Records Phase 26 baseline, Phase 30 cache proof, Phase 31 automated rows, policy/filtering evidence, and deferred acceptance decisions.
- `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-UAT.md` - Captures five manual UAT scenarios, all skipped in this headless execution session.
- `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-VERIFICATION.md` - Records automated verification and the remaining live visual UAT gap.

## Decisions Made

- Keep the current cache capacities: `RASTER_CACHE_CAPACITY=256` and `TEXT_LAYOUT_CACHE_CAPACITY=128`.
- Keep clear behavior unchanged: full-surface policy clears the full buffer, non-full-surface policy clears the effective damage rect.
- Mark all benchmark acceptance decisions `deferred` because automated counters cannot substitute for live visual UAT.

## Deviations from Plan

None - plan executed exactly as written. The plan allowed `skipped` UAT rows and `gaps_found` verification status when live visual acceptance was unavailable.

---

**Total deviations:** 0 auto-fixed.
**Impact on plan:** The implementation is complete, but final user-visible smoothness acceptance remains a recorded verification gap.

## Issues Encountered

- Live visual shell UAT was not run from this headless terminal session. The five UAT rows are marked `skipped`, and verification status is `gaps_found`.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Phase 31 has automated proof and tuning complete, but milestone completion should wait for `$gsd-verify-work 31` or a live visual UAT pass. If lag remains visible, the documented next candidates are Skia/GPU renderer investigation, later parallel paint/layout exploration, or deeper diagnostics overlays.

## Self-Check: PASSED

- Plan tasks completed: 5/5
- Summary created
- Task commits exist
- Verification artifact exists
- Remaining UAT gap is documented explicitly

---
*Phase: 31-smoothness-proof-and-cpu-render-tuning*
*Completed: 2026-05-13*
