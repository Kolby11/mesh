# Phase 44 Research: Selected Renderer Proof Integration

## Research Complete

Phase 44 should be planned as a constrained production proof, not a renderer migration. The selected Phase 43 path is the MESH-owned focused-crate path, so the implementation should make focused layout/text/paint/accessibility evidence available behind current retained renderer contracts while leaving `mesh-core-render`, `mesh-core-presentation`, and shipped navigation/audio behavior authoritative.

## Phase Constraints That Matter

- `INTG-01` requires retained node identity, typed invalidation categories, damage/profiling payloads, and non-fatal diagnostics to remain visible.
- `INTG-02` requires current navigation/audio behavior to stay covered by automated tests.
- `INTG-03` requires text layout, selection geometry, and theme-owned selection colors to be tested through the selected path.
- `INTG-04` requires accessibility metadata to remain derivable from retained nodes with an AccessKit-compatible update boundary.
- Phase 44 must not adopt Blitz directly, replace Wayland/layer-shell presentation with Winit, or turn the proof into broad renderer migration planning.
- Phase 45 owns the broad migration plan, so Phase 44 should leave clear evidence and reversible boundaries rather than spreading crate-specific assumptions through the shell.

## Local Source Findings

| Source | Finding | Planning implication |
|--------|---------|----------------------|
| `crates/core/shell/src/shell/component/runtime_tree.rs` | `stable_runtime_node_id()` derives deterministic `NodeId` values from stable runtime keys, and `RetainedWidgetTree` reports dirty categories: inserted, removed, layout, style, attributes, children, and state. | The proof adapter should preserve `NodeId` as the identity source and expose retained-tree dirty categories unchanged. |
| `crates/core/frontend/render/src/render_object.rs` | `RenderObjectTree` tracks retained render-object dirty slots: transform, clip, opacity, geometry, material, text, and accessibility. | The proof path should attach focused evidence at or after this boundary so geometry/material/text/accessibility changes remain typed. |
| `crates/core/frontend/render/src/display_list.rs` | `RetainedDisplayList` keys commands by `DisplayListKey { node_id, slot }` and records damage, repaint policy, filtered commands, and batch metrics. Selection payloads already live in display-list text content. | The proof path should emit display-slot evidence keyed by `NodeId` and preserve existing damage/profiling payloads instead of a separate output-only channel. |
| `crates/core/shell/src/shell/component/shell_component.rs` | `paint()` builds/restyles the widget tree, updates retained tree/render objects/display list, computes effective damage, records `ProfilingInvalidationSnapshot`, paints selected commands, and stores present damage. | The safest integration point is after retained display-list update and before or alongside paint metrics, with a snapshot stored for tests/diagnostics. |
| `crates/core/frontend/host/src/lib.rs` | `ShellComponent` exposes `take_profiling_records()`, `take_invalidation_snapshot()`, and `take_present_damage()` as existing observability boundaries. | Any proof-facing observability should be tested through existing component methods where possible. |
| `crates/core/shell/src/shell/component/diagnostics.rs` | Component diagnostics already publish non-fatal issues such as missing icons when a diagnostics sink is present. | Unsupported proof adaptation should record non-fatal diagnostics via existing component diagnostics, not panic. |
| `crates/core/frontend/render/src/surface/painter/text.rs` | Selection geometry reads `_mesh_selection_*` attributes and renders selection highlights using theme-owned colors. | Focused text proof must preserve `color.selection-background` and `color.selection-foreground` behavior and test geometry/paint continuity. |
| `crates/core/frontend/render/src/surface/painter/tests.rs` | Existing tests assert selection paint uses selection colors and does not bleed into neighboring nodes. | Phase 44 can extend paint/display-list tests rather than inventing a separate visual harness. |
| `crates/core/shell/src/shell/component/tests/invalidation/profiling.rs` | Existing tests prove shipped interaction scenarios through profiling/invalidation snapshots. | Add Phase 44 assertions to existing profiling-style tests for focused proof visibility and retained categories. |
| `crates/core/shell/src/shell/component/tests/integration/real_surfaces.rs` | Existing real-surface integration tests cover shipped frontend modules. | Use this area for navigation/audio behavior regression coverage after the proof adapter is wired. |

## Focused-Crate Integration Findings

The Phase 43 focused evidence used these proof concepts:

- `taffy_layout` records for retained layout evidence.
- `parley_text` records for text shaping/layout boundary evidence.
- `display_slot` records matching MESH display-list primitive slots.
- `accesskit_node_id` records derived from stable MESH node IDs.

For Phase 44, the important production move is not full crate replacement. It is a typed adapter shape that can be backed by focused crates later while preserving current MESH data ownership now. Planning should therefore introduce explicit proof records and tests around them before attempting any broad dependency expansion.

## Recommended Implementation Shape

Create a focused proof module in `mesh-core-render` with deterministic structs and helpers:

- `FocusedProofSnapshot`
- `FocusedProofNode`
- `FocusedLayoutEvidence`
- `FocusedTextEvidence`
- `FocusedPaintEvidence`
- `FocusedAccessibilityEvidence`
- `FocusedProofDiagnostic`
- `build_focused_proof_snapshot(root, render_dirty, display_metrics, selected_paint)`

The snapshot should be built from current MESH retained data and selected display-list commands. It should expose field names from Phase 43 evidence (`stable_node_id`, `taffy_layout`, `parley_text`, `display_slot`, `accesskit_node_id`) while keeping the real authority as MESH `NodeId`.

Integrate the snapshot into `FrontendSurfaceComponent::paint()` after display-list selection and before `self.last_tree = Some(tree)`. Store the latest snapshot in the component for tests and diagnostics. Do not expose it as a public user-facing API unless execution finds an existing debug surface that already expects renderer proof payloads.

## Test Strategy

Use existing Rust test infrastructure:

- Quick command: `cargo test -p mesh-core-render proof`
- Shell/component focused command: `cargo test -p mesh-core-shell phase44`
- Existing regression command: `cargo test -p mesh-core-shell navigation audio_popover`
- Full command: `cargo test --workspace`

Plan-level verification should prefer focused crate-level tests first, then component integration tests, then full workspace checks.

## Validation Architecture

Phase 44 validation should be test-driven around retained proof invariants.

- Quick command: `cargo test -p mesh-core-render proof`
- Integration command: `cargo test -p mesh-core-shell phase44`
- Regression command: `cargo test -p mesh-core-shell navigation`
- Full suite command: `cargo test --workspace`
- Evidence command: `rg -n "INTG-01|INTG-02|INTG-03|INTG-04|Focused proof snapshot|AccessKit-compatible" .planning/phases/44-selected-renderer-proof-integration/44-INTEGRATION-EVIDENCE.md`

Required proof invariants:

1. Stable MESH node IDs appear in focused proof nodes and paint/accessibility records.
2. Typed dirty categories include geometry/material/text/accessibility.
3. Existing invalidation snapshot and present-damage behavior remain observable.
4. Selection proof uses `color.selection-background` and `color.selection-foreground`.
5. AccessKit-compatible node IDs are derived deterministically from retained MESH node IDs.
6. Unsupported proof adaptation records diagnostics instead of panicking.

## Planning Recommendations

1. Plan the render-crate proof snapshot adapter first.
2. Plan shell/component integration after the render adapter compiles.
3. Plan text-selection and AccessKit boundary tests after the snapshot is integrated.
4. Plan shipped navigation/audio regression evidence last.
5. Keep root dependency additions narrow. If real Taffy/Parley/AccessKit crates are introduced, isolate them behind the proof module and record build/Nix implications for Phase 45.

## Sources

- `.planning/phases/44-selected-renderer-proof-integration/44-CONTEXT.md`
- `.planning/phases/43-comparable-renderer-prototype-proofs/43-PHASE44-HANDOFF.md`
- `.planning/phases/43-comparable-renderer-prototype-proofs/43-PROTOTYPE-COMPARISON.md`
- `.planning/prototypes/phase43/evidence/focused-crate.md`
- `crates/core/shell/src/shell/component/runtime_tree.rs`
- `crates/core/frontend/render/src/render_object.rs`
- `crates/core/frontend/render/src/display_list.rs`
- `crates/core/shell/src/shell/component/shell_component.rs`
- `crates/core/frontend/render/src/surface/painter/text.rs`
