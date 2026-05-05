---
phase: 09-responsive-and-interaction-reactivity
plan: 04
subsystem: ui-runtime
tags: [rust, shell, state-preservation, cleanup, restyle, interaction]

requires:
  - phase: 09-01
    provides: Stable pseudo-state annotation before restyle
  - phase: 09-02
    provides: Surface-size invalidation and container query coverage
  - phase: 09-03
    provides: Post-restyle layout synchronization

provides:
  - collect_all_keys() free function for building the post-restyle key set
  - prune_stale_interaction_targets() clearing hover/focus/active for absent keys
  - Paint-time pruning so every repaint deterministically clears stale targets
  - state_preservation_restyle_* tests proving service payload, runtime count, and user-input state survive restyles
  - restyle_state_cleanup_* tests proving stale hover/focus/active are cleared; valid targets retained

affects:
  - phase-11-keyboard-navigation
  - phase-13-navigation-bar-proof

tech-stack:
  added: []
  patterns:
    - "collect_all_keys: walks the final widget tree recursively to collect every _mesh_key"
    - "prune_stale_interaction_targets: clears focused_key, hovered_key/path/start, pointer_down_key, active_slider_key when absent from the final tree"
    - "Pruning runs in paint() after build_tree(), ensuring every repaint sees the correct interaction set"

key-files:
  created:
    - .planning/phases/09-responsive-and-interaction-reactivity/09-04-SUMMARY.md
  modified:
    - crates/core/shell/src/shell/component.rs

key-decisions:
  - "prune_stale_interaction_targets clears only absent keys; keys present in the final tree always retain their state regardless of display:none or disabled."
  - "Input, slider, checked, and scroll maps are not pruned here — their cleanup is deliberate and governed by separate user-action logic."
  - "Pruning is called from paint() rather than build_tree() to avoid a borrow conflict between building the tree and mutating self fields."

requirements-completed: [REACT-04, REACT-02]

duration: ~20min
completed: 2026-05-05
---

# Phase 09 Plan 04: State Preservation and Cleanup Summary

**Service payload, runtime instance, and user-input state are provably preserved through restyles; stale hover/focus/active targets are deterministically cleared after every repaint.**

## Performance

- **Duration:** ~20 min
- **Started:** 2026-05-05T18:20:00Z
- **Completed:** 2026-05-05T18:38:18Z
- **Tasks:** 2
- **Files modified:** 1 code file, 1 summary file

## Accomplishments

- **Task 09-04-01:** Added three `state_preservation_restyle_*` tests:
  - `state_preservation_restyle_service_payload_survives_hover_restyle`: applies `__mesh_svc_audio` via `apply_service_payload`, triggers a hover restyle, confirms vol_pct survives as 72.
  - `state_preservation_restyle_does_not_reinitialize_runtime`: proves the Lua top-level block doesn't re-execute on focus restyles and runtime instance count stays at 1.
  - `state_preservation_restyle_user_input_state_survives_focus_restyle`: seeds input_values, slider_values, checked_values, and scroll_offsets; proves all four maps survive a focus-driven restyle.

- **Task 09-04-02:** Implemented `collect_all_keys()` and `prune_stale_interaction_targets()`:
  - `collect_all_keys(node, keys)`: recursively walks the final widget tree and inserts every `_mesh_key` into a `HashSet<String>`.
  - `prune_stale_interaction_targets(&self, tree)`: clears `focused_key`, `hovered_key/hovered_path/hover_start`, `pointer_down_key`, and `active_slider_key` when their key is absent from the final tree.
  - Hooked into `paint()` immediately after `build_tree()`.
  - Added four `restyle_state_cleanup_*` tests proving hover, focus, and active are cleared for removed nodes; valid targets are not affected.

## Task Commits

1. **Tasks 09-04-01 + 09-04-02: State preservation and cleanup** - `ceeb88a` (feat)

## Files Created/Modified

- `crates/core/shell/src/shell/component.rs` — `collect_all_keys()`, `prune_stale_interaction_targets()`, `paint()` integration, 7 new regression tests.

## Decisions Made

- Pruning only removes absent keys. Nodes with `display:none` still have their `_mesh_key` in the tree after layout, so they continue to hold focus/hover/active state — this is intentional; use `{#if}` to remove a node if interaction state should clear.
- Input, slider, checked, and scroll maps are intentionally not pruned by this mechanism. Their lifecycle is governed by explicit user-action semantics, not tree membership.
- The pruning call lives in `paint()` rather than inside `build_tree()` to avoid the borrow checker conflict of returning the tree while simultaneously clearing self fields.

## Deviations from Plan

None — plan executed exactly as written. The scope stayed within the targeted file and the threat model (T-09-04: prune only absent keys, preserve state by stable key) was fully addressed.

## Issues Encountered

- Pre-existing test failure: `quick_settings_wifi_row_publishes_connect_for_wifi_network_ids` in `mesh-core-shell` fails with a Lua nil-value call error. This was present before this wave and is documented in 09-02-SUMMARY and 09-03-SUMMARY. All 94 other tests pass.
- The first draft of `state_preservation_restyle_service_payload_survives_hover_restyle` used `require("@mesh/audio@>=1.0")` with `on_change`, but the frontend catalog needed to satisfy the proxy wasn't wired in `test_frontend_component_with_catalog`. Fixed by reading `__mesh_svc_audio` directly from the global table in the `onRender` hook instead.
- The first draft of `state_preservation_restyle_user_input_state_survives_focus_restyle` asserted `_mesh_scroll_y == "42.00"`, but `annotate_overflow_tree` clamps the scroll offset to the actual overflow range (0 when no real overflow exists). Fixed by asserting the raw `scroll_offsets` map entry is preserved instead, and adding overflow-capable CSS to the scroll node so the test component has real overflow headroom.

## Verification

- `nix develop -c cargo test -p mesh-core-shell state_preservation_restyle` — 3 passed.
- `nix develop -c cargo test -p mesh-core-shell restyle_state_cleanup` — 4 passed.
- `nix develop -c cargo test -p mesh-core-shell` — 94 passed, 1 pre-existing failure.

## Known Stubs

None introduced.

## Threat Flags

None. All changes are internal to `build_tree`/`paint()` — no new network endpoints, auth paths, file access patterns, or trust boundary crossings.

## Self-Check: PASSED

- Summary file: `.planning/phases/09-responsive-and-interaction-reactivity/09-04-SUMMARY.md` — written.
- Commit `ceeb88a` exists.
- No tracked file deletions introduced.

---
*Phase: 09-responsive-and-interaction-reactivity*
*Completed: 2026-05-05*
