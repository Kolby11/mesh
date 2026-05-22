---
phase: 51-painter-contract-and-backend-boundary
plan: 02
subsystem: renderer
tags: [renderer, painter-api, compatibility, diagnostics]
requires:
  - phase: 51-01
    provides: backend-neutral painter command contract
provides:
  - Compatibility helper lowering through PainterCommand
  - FrontendRenderEngine painter diagnostics access
  - Command-path tests for helper wrappers
affects: [renderer, painter-backend, diagnostics]
tech-stack:
  added: []
  patterns: [compatibility wrapper lowering, renderer-local backend diagnostics]
key-files:
  created: []
  modified:
    - crates/core/frontend/render/src/surface/painter/backend.rs
    - crates/core/frontend/render/src/surface/painter.rs
    - crates/core/frontend/render/src/surface/painter/tests.rs
key-decisions:
  - "FrontendRenderEngine helper methods now build PainterCommand slices before backend execution."
  - "Painter diagnostics are stored outside retained display-list identity and can be inspected/cleared by tests."
patterns-established:
  - "RecordingPaintBackend test doubles prove command lowering without relying on Skia pixels."
requirements-completed: [PAINT-01, PAINT-02, BACKEND-01]
duration: 2 min
completed: 2026-05-22
---

# Phase 51 Plan 02: Compatibility Wrapper Lowering Summary

**Existing renderer paint helpers lowered through the new painter command backend boundary**

## Performance

- **Duration:** 2 min
- **Started:** 2026-05-22T04:40:30Z
- **Completed:** 2026-05-22T04:42:22Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- Routed `FrontendRenderEngine` helper methods through `execute_painter_commands`.
- Added diagnostics collection on `FrontendRenderEngine` without involving retained display-list ordering, material hashes, or damage state.
- Added recording-backend tests proving rect, filtered rounded rect, shadow, and backdrop filter helpers lower into `PainterCommand` variants.

## Task Commits

1. **Task 1-3: Helper lowering, diagnostics, and tests** - `1fc327f`
2. **Post-wave fix: Preserve square border command rendering** - `7e4b3e7`
3. **Code review fix: Align capabilities with diagnostics** - `1093f41`

## Files Created/Modified

- `crates/core/frontend/render/src/surface/painter/backend.rs` - Marks future command variants as intentionally defined before all call sites use them.
- `crates/core/frontend/render/src/surface/painter.rs` - Adds command execution and diagnostics plumbing on the engine.
- `crates/core/frontend/render/src/surface/painter/tests.rs` - Adds recording backend and command-lowering tests.

## Decisions Made

- Kept diagnostics engine-local so unsupported backend behavior is visible without becoming retained rendering identity.
- Used a recording backend test double to verify wrapper behavior independently from Skia output.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Square border command path skipped expected inner edge pixels**

- **Found during:** Phase-level `mesh-core-render` full suite
- **Issue:** `stroke_rounded_rect_clipped` now returned success for square borders, bypassing the old fallback, but the Skia stroke path did not fill pixel `(1, 1)` as expected by existing square-border behavior.
- **Fix:** Updated `stroke_rect_impl` to paint inside-edge border rectangles through backend-internal fill handling.
- **Files modified:** `crates/core/frontend/render/src/surface/painter/backend.rs`
- **Verification:** `painter_draws_border_from_computed_edges` and the full `mesh-core-render` suite pass.
- **Committed in:** `7e4b3e7`

**2. [Rule 1 - Bug] Capability flags overstated deferred clip/layer stack behavior**

- **Found during:** Code review gate
- **Issue:** Skia capabilities reported clip/layer stack support while `execute_commands` still diagnosed stack commands as deferred, and standalone blur-filter commands could silently no-op.
- **Fix:** Marked clip/layer stack capabilities false for the current implementation, diagnosed standalone blur filters, and added capability diagnostic assertions.
- **Files modified:** `crates/core/frontend/render/src/surface/painter/backend.rs`, `crates/core/frontend/render/src/surface/painter/tests.rs`
- **Verification:** `painter_backend_capabilities` and the full `mesh-core-render` suite pass.
- **Committed in:** `1093f41`

---

**Total deviations:** 2 auto-fixed bugs.
**Impact on plan:** Tightened backend capability truthfulness and unsupported-feature behavior without changing the command contract.

## Issues Encountered

- Post-wave full-suite validation caught the square-border regression above; fixed in `7e4b3e7`.
- Code review caught a capability/diagnostic mismatch; fixed in `1093f41`.

## Verification

- `cargo fmt --all -- --check` passed.
- `cargo check -p mesh-core-render` passed with the existing `placement_top` dead-code warning.
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render painter_helper_lowering -- --nocapture` passed.
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render painter_backend_diagnostics -- --nocapture` passed.
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render` passed after the square-border fix.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Ready for Plan 03 to lock the docs, migration map, and Vello compatibility notes around the implemented boundary.

## Self-Check: PASSED

---
*Phase: 51-painter-contract-and-backend-boundary*
*Completed: 2026-05-22*
