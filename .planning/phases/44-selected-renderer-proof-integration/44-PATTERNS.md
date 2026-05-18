# Phase 44 Pattern Map

## Scope

Phase 44 should add a constrained renderer proof adapter and tests. It should not replace production rendering, presentation, shipped frontend modules, or authoring contracts.

## Planned Files and Closest Analogs

| Planned file | Role | Closest existing analog | Pattern to preserve |
|--------------|------|-------------------------|---------------------|
| `crates/core/frontend/render/src/proof.rs` | Focused proof snapshot and adapter helpers | `crates/core/frontend/render/src/display_list.rs`, `crates/core/frontend/render/src/render_object.rs` | Keep `NodeId` authoritative; use explicit structs and deterministic metrics; avoid shell-specific behavior. |
| `crates/core/frontend/render/src/lib.rs` | Public render exports | Current display-list/render-object exports | Re-export only focused proof types needed by shell/tests. |
| `crates/core/shell/src/shell/component.rs` | Store latest proof snapshot | Existing `invalidation_snapshot` and `last_present_damage` fields | Keep proof snapshot component-local and reset with retained caches. |
| `crates/core/shell/src/shell/component/shell_component.rs` | Build proof snapshot during paint | Existing retained tree/render-object/display-list/profiling flow | Build proof after display-list selection, preserve invalidation snapshots and damage behavior. |
| `crates/core/shell/src/shell/component/diagnostics.rs` | Non-fatal proof diagnostics | `record_missing_icon_diagnostic()` | Record diagnostic strings through existing diagnostics sink; no panics for unsupported adapter data. |
| `crates/core/frontend/render/src/surface/painter/tests.rs` | Paint/selection regression tests | Existing selection color tests | Assert theme-owned selection colors and geometry remain visible through display-list/proof payloads. |
| `crates/core/shell/src/shell/component/tests/invalidation/profiling.rs` | Invalidation/profiling integration tests | Existing Phase 26/31 profiling proof tests | Reuse profiling-enabled component flow and assert proof visibility without changing shipped behavior. |
| `crates/core/shell/src/shell/component/tests/integration/real_surfaces.rs` | Real shipped-surface regression tests | Existing navigation/audio integration tests | Add Phase 44 proof assertions while keeping navigation/audio source behavior unchanged. |
| `.planning/phases/44-selected-renderer-proof-integration/44-INTEGRATION-EVIDENCE.md` | Final requirement evidence | `43-PROTOTYPE-COMPARISON.md`, `43-PHASE44-HANDOFF.md` | Record exact commands, requirement coverage, retained contracts, diagnostics, text/selection, and AccessKit boundary. |

## Concrete Source Details to Carry Forward

- Retained identity is `mesh_core_elements::NodeId`.
- Retained-tree dirty categories are `inserted`, `removed`, `layout`, `style`, `attributes`, `children`, and `state`.
- Render-object dirty categories are `inserted`, `removed`, `reordered`, `transform`, `clip`, `opacity`, `geometry`, `material`, `text`, and `accessibility`.
- Display primitive slots are `Background`, `Border`, `Text`, `Icon`, and `Generic`.
- Selection paint reads `_mesh_selection_background`, `_mesh_selection_foreground`, `_mesh_selection_anchor_x`, `_mesh_selection_anchor_y`, `_mesh_selection_focus_x`, `_mesh_selection_focus_y`, `_mesh_selection_text_x`, and `_mesh_selection_text_y`.
- Theme selection tokens are `color.selection-background` and `color.selection-foreground`.
- AccessKit-compatible IDs should be deterministic strings such as `accesskit_node_id::<node_id>` unless execution chooses to add real `accesskit::NodeId` values behind the same visible boundary.

