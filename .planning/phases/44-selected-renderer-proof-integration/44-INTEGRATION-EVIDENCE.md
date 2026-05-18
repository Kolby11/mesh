# Phase 44 Integration Evidence

## Requirement Coverage

| Requirement | Evidence |
|-------------|----------|
| INTG-01 | `FocusedProofSnapshot` carries retained node identity, dirty geometry/material/text/accessibility fields, damage evidence, selected paint evidence, and non-fatal diagnostics through shell paint. |
| INTG-02 | `phase44_navigation_audio_surface_emits_focused_proof_snapshot` and `phase44_navigation_behavior_survives_focused_proof_path` cover shipped navigation/audio surfaces after focused proof snapshots are created. |
| INTG-03 | `proof_snapshot_preserves_theme_owned_selection_payload`, `phase44_selection_paint_and_proof_use_theme_colors`, and `phase44_selection_restyle_keeps_focused_text_payload` cover text selection color and geometry proof payloads. |
| INTG-04 | `FocusedAccessibilityEvidence`, `FocusedAccessKitUpdate`, and `proof_snapshot_builds_accesskit_update_from_retained_nodes` derive AccessKit-compatible IDs from retained MESH `NodeId` values. |

## Retained Identity and Invalidation

The render proof adapter stores every focused proof node, selected paint record, and accessibility record with the original MESH `NodeId` plus `stable_node_id = node_id.to_string()`. Dirty proof evidence keeps explicit `geometry`, `material`, `text`, and `accessibility` fields from `RenderObjectDirtySummary`.

Shell integration stores the focused snapshot next to the existing invalidation snapshot without changing retained-tree, render-object, display-list, selected paint, or present damage behavior.

## Damage, Profiling, and Diagnostics

`phase44_focused_proof_preserves_invalidation_and_damage_payloads` proves a shipped navigation paint still exposes:

- `take_invalidation_snapshot()`
- `take_present_damage()`
- focused proof nodes
- focused accessibility records
- explicit dirty-category fields

Focused proof diagnostics are recorded through the existing diagnostics handle with the prefix `focused renderer proof:` and use degraded, non-fatal diagnostics rather than returning `ComponentError`.

## Navigation and Audio Regression

`phase44_navigation_audio_surface_emits_focused_proof_snapshot` paints the shipped navigation bar and audio popover fixtures, then asserts focused proof snapshots contain retained nodes, text/icon paint slots, and accessibility records.

`phase44_navigation_behavior_survives_focused_proof_path` paints the shipped navigation bar, performs Tab navigation, repaints, and asserts focus behavior still survives while focused proof evidence remains available.

This phase did not intentionally edit `modules/frontend/navigation-bar/src/main.mesh` or `modules/frontend/audio-popover/src/main.mesh`. Those files had pre-existing local modifications before plan 44-04 execution.

## Text Selection Proof

Focused text evidence preserves:

- `selection_background`
- `selection_foreground`
- `selection_anchor`
- `selection_focus`

The proof path observes shell/theme-owned `_mesh_selection_*` attributes; it does not become the authority for selection colors. Painter and shell tests verify the selected path still uses the theme-owned color strings.

## AccessKit-Compatible Boundary

`build_accesskit_update` creates a retained-node accessibility update with `root_id` from the first focused accessibility record or `accesskit_node_id::empty` for empty snapshots. Every update node keeps `stable_node_id == node_id.to_string()` and a deterministic `accesskit_node_id::...` ID.

## Deferred Items

Audio Popover Transition Delay Polish remains deferred. Phase 44 preserved navigation/audio renderer proof coverage and did not intentionally fix shell-owned show/hide transition timing.

## Commands

All Rust commands are run inside the Nix dev shell in this environment because direct host linking lacks required native libraries.

| Command | Status |
|---------|--------|
| `cargo test -p mesh-core-render proof` | Passed via `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render proof` |
| `cargo test -p mesh-core-shell phase44` | Passed via `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase44` |
| `cargo test -p mesh-core-shell phase44_navigation` | Passed via `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase44_navigation` |
| `cargo test --workspace` | Passed via `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test --workspace` |
