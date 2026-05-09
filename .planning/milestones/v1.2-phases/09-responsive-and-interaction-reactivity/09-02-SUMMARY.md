---
phase: 09-responsive-and-interaction-reactivity
plan: 02
subsystem: ui
tags: [container-query, style-invalidation, render, layout]

requires:
  - phase: 09-01
    provides: Pseudo-state annotation and restyle infrastructure

provides:
  - Surface-size invalidation marks FrontendSurfaceComponent dirty on dimension change
  - observe_surface_size deduplicates identical consecutive sizes to avoid churn
  - container_size_restyle_preserves_runtime_and_local_state regression in mesh-core-shell
  - container_query_* tests in mesh-core-render prove breakpoints apply at different root sizes

affects:
  - 09-03
  - 09-04

tech-stack:
  added: []
  patterns:
    - "observe_surface_size: tracks last rendered dimensions and sets dirty flag only on change"
    - "build_tree receives width/height which feed StyleContext for container query resolution"
    - "CompiledFrontendPlugin.build_preview_tree_with_state is the test seam for container query coverage"

key-files:
  created: []
  modified:
    - crates/core/shell/src/shell/component.rs
    - crates/core/ui/render/src/lib.rs

key-decisions:
  - "Surface size invalidation is explicit: observe_surface_size compares against last_surface_size before marking dirty."
  - "Container query coverage lives in mesh-core-render tests, which use build_preview_tree as the composition seam."

patterns-established:
  - "Size-driven restyle reuses ScriptContext, service state, input values, slider values, checked values, and scroll offsets."
  - "Container queries are proven by building the same plugin twice at different widths and asserting computed_style differs."

requirements-completed: [REACT-01, REACT-04]

duration: 15min
completed: 2026-05-05
---

# Phase 09 Plan 02: Size and Container Query Invalidation Summary

**Surface-size invalidation via `observe_surface_size` with runtime-state-preserving restyles, plus three container query proofs in mesh-core-render confirming @container rules apply at correct dimension thresholds**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-05-05T18:00:00Z
- **Completed:** 2026-05-05T18:14:50Z
- **Tasks:** 2
- **Files modified:** 1 (lib.rs in mesh-core-render; component.rs was in base commit)

## Accomplishments

- Task 09-02-01: `observe_surface_size` / `last_surface_size` / `surface_size_changed` / `wants_render` infrastructure was landed in base commit `4b559c4 feat(09-02): track surface size invalidation`. The regression test `container_size_restyle_preserves_runtime_and_local_state` confirms runtime state survives dimension restyles and identical sizes do not create extra dirty churn.
- Task 09-02-02: Added three targeted tests to `mesh-core-render/src/lib.rs` proving container queries produce breakpoint-correct computed styles when the root surface size changes, that max-width rules invert correctly, and that consecutive builds at different sizes are independent with no shared style state.

## Task Commits

Each task was committed atomically:

1. **Task 09-02-01: Track surface-size invalidation** - `4b559c4` (feat — base commit, landed before this agent)
2. **Task 09-02-02: Prove container queries use current dimensions** - `baa73fc` (test)

## Files Created/Modified

- `crates/core/shell/src/shell/component.rs` — `observe_surface_size`, `last_surface_size`, `surface_size_changed`, regression test (base commit)
- `crates/core/ui/render/src/lib.rs` — three `container_query_*` tests, `make_test_plugin` and `find_first_by_tag` helpers

## Decisions Made

- Surface size invalidation is explicit: `observe_surface_size` compares against `last_surface_size` before marking dirty, so identical consecutive dimensions produce no dirty churn.
- Container query coverage lives in `mesh-core-render` tests using `build_preview_tree` as the composition seam, keeping shell-level and render-level coverage cleanly separated.

## Deviations from Plan

None — plan executed exactly as written. The base commit already contained the Task 09-02-01 implementation. This agent verified it passed and added the Task 09-02-02 render tests.

## Issues Encountered

Pre-existing test failure: `quick_settings_wifi_row_publishes_connect_for_wifi_network_ids` in `mesh-core-shell` fails with a Lua nil-value call error. This was present before this agent started and is out of scope. Logged to deferred-items for follow-up.

## User Setup Required

None.

## Next Phase Readiness

- Size invalidation and container query test coverage are complete.
- Phase 09-03 can proceed with interaction reactivity (hover, focus, active states) knowing restyle correctness is verified.

---
*Phase: 09-responsive-and-interaction-reactivity*
*Completed: 2026-05-05*
