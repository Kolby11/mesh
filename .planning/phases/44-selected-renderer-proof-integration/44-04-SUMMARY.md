---
phase: 44-selected-renderer-proof-integration
plan: 04
subsystem: shell
tags: [shipped-surfaces, navigation, audio, evidence, regression]

requires:
  - phase: 44-02
    provides: Shell focused proof snapshots
  - phase: 44-03
    provides: Selection and AccessKit proof evidence
provides:
  - Navigation/audio shipped-surface regression proof with focused snapshots present
  - Final INTG-01 through INTG-04 integration evidence
  - Workspace-green verification for the selected proof integration
affects: [renderer, shell, navigation, audio, verification]

tech-stack:
  added: []
  patterns: [shipped-surface proof regression, evidence-backed verification]

key-files:
  created:
    - .planning/phases/44-selected-renderer-proof-integration/44-INTEGRATION-EVIDENCE.md
  modified:
    - crates/core/shell/src/shell/component/tests/integration/real_surfaces.rs
    - crates/core/shell/src/shell/component/tests/interaction/navigation.rs
    - crates/core/shell/src/shell/component.rs
    - crates/core/shell/src/shell/tests.rs

key-decisions:
  - "Keep navigation/audio module source files untouched during Phase 44 execution."
  - "Record Audio Popover Transition Delay Polish as deferred rather than changing transition behavior."

patterns-established:
  - "Phase evidence records exact commands and final statuses."
  - "Shipped-surface proof tests inspect focused snapshots after normal navigation/audio paints."

requirements-completed: [INTG-01, INTG-02, INTG-03, INTG-04]

duration: 35min
completed: 2026-05-18
---

# Phase 44-04: Shipped Surface Regression and Evidence Summary

**Navigation and audio shipped-surface tests now prove focused snapshots exist during normal paints, with final evidence covering INTG-01 through INTG-04**

## Performance

- **Duration:** 35 min
- **Started:** 2026-05-18T15:33:00+02:00
- **Completed:** 2026-05-18T16:08:00+02:00
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments

- Added a real-surface regression test for navigation and audio focused proof snapshots.
- Added a keyboard navigation regression test that verifies focused proof evidence remains present after Tab navigation.
- Wrote final integration evidence for retained identity, invalidation, diagnostics, shipped behavior, selection proof, and AccessKit-compatible updates.
- Restored existing workspace suite expectations discovered during the final gate.

## Task Commits

1. **Tasks 44-04-01/44-04-02: Shipped-surface proof and evidence** - `cdcfaf7` (test)

## Files Created/Modified

- `crates/core/shell/src/shell/component/tests/integration/real_surfaces.rs` - Adds navigation/audio focused proof snapshot regression coverage.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` - Adds keyboard navigation proof-path regression coverage.
- `.planning/phases/44-selected-renderer-proof-integration/44-INTEGRATION-EVIDENCE.md` - Records requirement evidence and command status.
- `crates/core/shell/src/shell/component.rs` - Restores layout as part of interaction restyle invalidation.
- `crates/core/shell/src/shell/tests.rs` - Gives existing keyboard-mode tests explicit initial modes matching their expectations.

## Decisions Made

- Did not modify `modules/frontend/navigation-bar/src/main.mesh` or `modules/frontend/audio-popover/src/main.mesh` for this plan.
- Treated the workspace-gate fixes as auto-fixes because they restore existing test contracts and are not renderer proof feature scope.

## Deviations from Plan

### Auto-fixed Issues

**1. Existing interaction restyle invalidation contract**
- **Found during:** `cargo test --workspace`
- **Issue:** `typed_invalidations_distinguish_restyle_from_script_rebuild` expected layout invalidation for interaction restyle, but `INTERACTION_RESTYLE` no longer included layout.
- **Fix:** Restored `ComponentDirtyFlags::LAYOUT` to `INTERACTION_RESTYLE`.
- **Files modified:** `crates/core/shell/src/shell/component.rs`
- **Verification:** `cargo test -p mesh-core-shell typed_invalidations_distinguish_restyle_from_script_rebuild`

**2. Existing keyboard-mode test setup**
- **Found during:** `cargo test --workspace`
- **Issue:** Two shell tests asserted restored keyboard modes without setting the starting mode they expected.
- **Fix:** Set the initial surface keyboard modes explicitly inside the tests.
- **Files modified:** `crates/core/shell/src/shell/tests.rs`
- **Verification:** `cargo test -p mesh-core-shell pointer_click_claims_keyboard_owner_without_forcing_exclusive_mode` and `cargo test -p mesh-core-shell pointer_click_after_transfer_clears_transfer_forced_exclusive_override`

**Total deviations:** 2 auto-fixed existing test-contract issues.
**Impact on plan:** No change to shipped navigation/audio module sources or renderer proof scope.

## Issues Encountered

The first workspace run exposed an existing icon opacity cache race, but the individual test passed and the final workspace run passed after the deterministic shell failures were fixed.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Phase 44 has command-backed evidence for the selected proof integration and is ready for phase verification and state completion.

---
*Phase: 44-selected-renderer-proof-integration*
*Completed: 2026-05-18*
