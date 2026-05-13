---
phase: 27-viewport-culling-and-visibility-elision
plan: 01
subsystem: rendering
tags: [viewport-culling, visibility, retained-rendering, display-list, debug-json]
requires:
  - phase: 26-cpu-render-profiling-and-baseline-proof
    provides: retained paint profiling and invalidation snapshot pipeline
provides:
  - explicit visibility semantics separate from plain opacity
  - viewport-aware subtree omission for clipped and scrollable retained paint collection
  - aggregate pruning counters on the existing invalidation/debug payload
affects: [28, 29, 31, retained-rendering, paint-traversal]
tech-stack:
  added: []
  patterns: [explicit visibility gate, subtree preclip before retained paint command generation, aggregate-only pruning proof]
key-files:
  created:
    - .planning/phases/27-viewport-culling-and-visibility-elision/27-01-SUMMARY.md
    - .planning/phases/27-viewport-culling-and-visibility-elision/27-VERIFICATION.md
  modified:
    - crates/core/ui/elements/src/style/types.rs
    - crates/core/ui/elements/src/style/resolve.rs
    - crates/core/frontend/render/src/display_list.rs
    - crates/core/foundation/debug/src/lib.rs
    - crates/core/shell/src/shell/component.rs
    - crates/core/shell/src/shell/runtime/debug.rs
    - crates/core/shell/src/shell/tests.rs
key-decisions:
  - "CSS visibility now resolves to explicit hidden semantics instead of being collapsed into bare opacity zero."
  - "Viewport omission remains conservative: fully non-intersecting clipped descendants are omitted, partially intersecting descendants still paint."
  - "Pruning proof stays aggregate-only and reuses the existing mesh.debug invalidation payload."
patterns-established:
  - "Retained paint omission should happen at display-list command collection, not via broad command filtering deeper in the painter."
  - "Verification-blocking test harness regressions can be repaired narrowly without changing runtime behavior."
requirements-completed: [CULL-01, CULL-02, CULL-04]
duration: unknown
completed: 2026-05-11
---

# Phase 27 Plan 01: Viewport-aware retained paint omission with explicit hidden semantics

**The retained display-list path now omits explicitly hidden subtrees and fully out-of-viewport clipped descendants before paint traversal, while leaving plain `opacity: 0` nodes paintable and surfacing aggregate pruning counters through the existing `mesh.debug` invalidation payload.**

## Performance

- **Completed:** 2026-05-11T11:55:06Z
- **Tasks:** 3
- **Files modified:** 7 runtime/phase files, plus 4 shell test harness fixes needed to run verification

## Accomplishments

- Added an explicit `Visibility` signal to computed style and resolved CSS `visibility:hidden|collapse` into that signal instead of treating it as plain opacity zero.
- Updated retained display-list collection so explicitly hidden nodes and `hidden` attribute subtrees are omitted before paint command generation.
- Added viewport-aware subtree preclipping for clipped and scrollable branches, with partial intersections intentionally left paintable.
- Extended retained paint metrics and `mesh.debug` invalidation JSON with aggregate pruning counters for omitted subtrees, omitted nodes, omitted commands, and preclipped descendants.
- Added focused render tests for explicit hidden vs opacity-zero semantics and for full-vs-partial viewport intersection behavior.

## Task Commits

No phase-scoped task commits were available in the current history. Phase 27 implementation was verified against the current `HEAD` state (`56ef872`) and closed out by writing the missing summary and verification artifacts.

## Files Created/Modified

- `crates/core/ui/elements/src/style/types.rs` and `crates/core/ui/elements/src/style/resolve.rs` now carry explicit `Visibility` semantics through style resolution.
- `crates/core/frontend/render/src/display_list.rs` now performs explicit-hidden omission, clipped subtree preclipping, and aggregate pruning metric collection.
- `crates/core/foundation/debug/src/lib.rs`, `crates/core/shell/src/shell/component.rs`, and `crates/core/shell/src/shell/runtime/debug.rs` now propagate pruning counters through the existing retained invalidation snapshot and `mesh.debug` JSON.
- `crates/core/shell/src/shell/tests.rs` locks the new aggregate counters in the profiling invalidation payload.

## Decisions Made

- Plain `opacity == 0.0` remains non-prunable in this phase; only explicit hidden semantics and authoritative viewport exclusion trigger omission.
- Omission stays conservative and subtree-based. This phase does not introduce broad per-command smart filtering inside partially visible branches.
- Debug proof remains aggregate-only and is published on the same invalidation/debug path established in Phase 26.

## Deviations from Plan

- Verification was briefly blocked by import/prelude regressions in the shell component test harness introduced outside the Phase 27 runtime files. Those test-only imports were repaired so the required `mesh-core-shell profiling` selector could run, without changing Phase 27 runtime behavior.

## Issues Encountered

- `cargo test -p mesh-core-shell profiling` initially failed to compile because `crates/core/shell/src/shell/component/tests/...` files were missing imports and re-exports. The fix was limited to test harness wiring and did not change shipped renderer behavior.

## User Setup Required

None.

## Next Phase Readiness

- Phase 28 can now rely on explicit hidden semantics and aggregate pruning metrics while refactoring retained paint-command ownership.
- Phase 29 can build on the new clipped-subtree omission counters when evaluating damage-indexed repaint policy tradeoffs.

## Self-Check: PASSED

- Verified `.planning/phases/27-viewport-culling-and-visibility-elision/27-01-SUMMARY.md` and `.planning/phases/27-viewport-culling-and-visibility-elision/27-VERIFICATION.md` exist on disk.
- Verified `cargo test -p mesh-core-render display_list`, `cargo test -p mesh-core-render painter_`, and `cargo test -p mesh-core-shell profiling` passed in the Nix dev shell.
