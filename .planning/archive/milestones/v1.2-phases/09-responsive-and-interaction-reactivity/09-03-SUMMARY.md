---
phase: 09-responsive-and-interaction-reactivity
plan: 03
subsystem: ui-runtime
tags: [rust, shell, restyle, layout, hit-test, accessibility, metrics]

requires:
  - phase: 09-01
    provides: Stable pseudo-state annotation before restyle
  - phase: 09-02
    provides: Surface-size invalidation and container query coverage

provides:
  - Post-restyle layout recomputation in build_tree guaranteeing final bounds
  - Hit-test regression proving pointer events use post-restyle bounds (D-04)
  - Ref/element metrics regression proving published bounds reflect final layout (D-11)
  - Accessibility tree bounds regression synchronized to post-restyle layout (D-13)

affects:
  - phase-11-keyboard-navigation
  - phase-13-navigation-bar-proof

tech-stack:
  added: []
  patterns:
    - "build_tree recomputes LayoutEngine::compute_with_measurer after restyle_subtree"
    - "Hit testing, metrics publishing, and paint all observe the same final post-restyle tree"

key-files:
  created:
    - .planning/phases/09-responsive-and-interaction-reactivity/09-03-SUMMARY.md
  modified:
    - crates/core/shell/src/shell/component.rs

key-decisions:
  - "Layout is recomputed after restyle_subtree in build_tree so pseudo-state and container-query CSS changes that affect width, height, or display are reflected in LayoutRect before hit-testing, metric publishing, and paint."
  - "Tests verify final bounds via node.layout.width (hit-test), refs host value (metrics), and AccessibilityTree.bounds (a11y)."

requirements-completed: [REACT-03]

duration: 15min
completed: 2026-05-05
---

# Phase 09 Plan 03: Post-Restyle Synchronization Summary

**Layout is now recomputed after every restyle so hit-testing, accessibility, and published metrics always observe final post-restyle bounds.**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-05-05T18:30:00Z
- **Completed:** 2026-05-05T18:45:00Z
- **Tasks:** 2
- **Files modified:** 1 code file, 1 summary file

## Accomplishments

- **Task 09-03-01:** Added `LayoutEngine::compute_with_measurer` call after `restyle_subtree` in `build_tree`. This fixes the gap where pseudo-state and container-query CSS changes that affect dimensions or `display` left stale `LayoutRect` values. Added `runtime_bool` test helper. Added `restyle_hit_test_uses_post_restyle_bounds` regression: hovers widen a button from 40px to 80px, a click at x=60 must fire the handler (proves post-restyle bounds used for hit-testing).

- **Task 09-03-02:** Added `restyle_metrics_reflect_post_restyle_bounds` regression: a `:focus` style rule changes button width from 40px to 80px; the `refs.btn.width` host value in the Lua context must reflect 80px after the focused repaint. Added `accessibility_data_synchronized_after_restyle` regression: `AccessibilityTree::from_widget_tree` must report button width ~120px after a `:focus` rule widens it from 60px.

## Task Commits

1. **Tasks 09-03-01 + 09-03-02: Post-restyle synchronization** - `d8bbc53` (feat)

## Files Created/Modified

- `crates/core/shell/src/shell/component.rs` — Added `LayoutEngine` import; post-restyle `LayoutEngine::compute_with_measurer` call in `build_tree`; `runtime_bool` helper; three synchronization regression tests.

## Decisions Made

- Layout recomputation after restyle is unconditional because there is no cheap way to know if a restyle changed any layout-affecting property. The second layout pass is O(n) on the tree and runs from the same surface root, so the cost is acceptable.
- Accessibility tree bounds come directly from `node.layout` (set by layout pass), so fixing the layout ordering is sufficient — no changes to `AccessibilityTree` construction were needed.

## Deviations from Plan

None — plan executed exactly as written. The gap (missing post-restyle layout pass) was real and the fix was a single `LayoutEngine::compute_with_measurer` call with the same measurer already available in `build_tree`.

## Issues Encountered

Pre-existing test failure: `quick_settings_wifi_row_publishes_connect_for_wifi_network_ids` in `mesh-core-shell` fails with a Lua nil-value call error. This was present before this wave and is documented in 09-02-SUMMARY. All 87 other tests pass.

## Verification

- `nix develop -c cargo test -p mesh-core-shell restyle_hit_test` — passed, 1 test.
- `nix develop -c cargo test -p mesh-core-shell restyle_metrics` — passed, 1 test.
- `nix develop -c cargo test -p mesh-core-shell accessibility` — passed, 1 test.
- `nix develop -c cargo test -p mesh-core-shell` — 87 passed, 1 pre-existing failure.

## Known Stubs

None introduced.

## Threat Flags

None. The change is internal to `build_tree` — no new network endpoints, auth paths, or trust boundary crossings.

## Self-Check: PASSED

- Summary file exists: `.planning/phases/09-responsive-and-interaction-reactivity/09-03-SUMMARY.md`
- Commit `d8bbc53` exists.
- No tracked file deletions introduced.

---
*Phase: 09-responsive-and-interaction-reactivity*
*Completed: 2026-05-05*
