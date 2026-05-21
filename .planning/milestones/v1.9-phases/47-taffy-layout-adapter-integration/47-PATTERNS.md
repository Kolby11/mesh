# Phase 47: Taffy Layout Adapter Integration - Pattern Map

## PATTERN MAPPING COMPLETE

## Scope

Phase 47 replaces current MESH layout computation with Taffy while preserving MESH-owned identity, dirty categories, render-object synchronization, and shipped navigation/audio behavior.

## Closest Existing Analogs

| Target | Closest Analog | Existing Pattern To Reuse |
|--------|----------------|---------------------------|
| Taffy layout entrypoint | `crates/core/ui/elements/src/layout.rs` | Keep the public `LayoutEngine::{compute, compute_with_measurer, compute_with_intrinsic_cache_and_measurer}` entrypoints stable so shell rendering call sites continue to work. |
| Retained identity writeback | `crates/core/ui/elements/src/tree.rs` | Preserve `WidgetNode.id` as the MESH identity source and write computed geometry back to `WidgetNode.layout`. |
| Geometry dirty propagation | `crates/core/frontend/render/src/render_object.rs` | Let existing render-object dirty detection compare `LayoutRect` values after Taffy writeback. |
| Shell profiling wrapper | `crates/core/shell/src/shell/component/rendering.rs` | Keep layout computation inside the existing profiling-stage timing path. |
| Feature/status scaffold | `crates/core/frontend/render/src/library_adapters.rs` | Update documentation/status language for Taffy authority without moving non-layout renderer candidates. |
| Shipped-surface tests | `crates/core/shell/src/shell/component/tests/integration/real_surfaces.rs` and `interaction/navigation.rs` | Add Phase 47 tests near existing navigation/audio proof tests rather than creating a separate harness. |

## Concrete File Guidance

### `crates/core/ui/elements/src/layout.rs`

Role: primary implementation boundary.

Current pattern:
- `TextMeasurer` is injected so `mesh-core-elements` does not depend on renderer text internals.
- `IntrinsicLayoutCache` uses `NodeId`, style, and subtree signatures.
- `LayoutEngine` public methods return `()` and mutate the `WidgetNode` tree in place.

Phase 47 target:
- Add Taffy conversion helpers inside this file or a child module such as `layout/taffy.rs`.
- Keep existing `LayoutEngine` method names and call signatures unless a new diagnostic-bearing helper is added.
- Write Taffy results back into `LayoutRect { x, y, width, height }`.

### `crates/core/ui/elements/Cargo.toml`

Role: layout crate dependency ownership.

Current pattern:
- Small direct dependencies only: component, theme, serde, serde_json, thiserror, tracing.

Phase 47 target:
- Add `taffy = { workspace = true }` because layout ownership lives in `mesh-core-elements`.
- Do not require shell/render crates to own layout just because Phase 46 originally placed optional Taffy under `mesh-core-render`.

### `crates/core/shell/src/shell/component/rendering.rs`

Role: runtime integration and profiling.

Current pattern:
- Calls `LayoutEngine::compute_with_intrinsic_cache_and_measurer`.
- Tracks layout timing with `mesh_core_debug::ProfilingStage::Layout`.
- Reuses retained layout when layout-stable restyles occur.

Phase 47 target:
- Avoid broad rewrites here. If diagnostics need surfacing, add a narrow call path that records Taffy diagnostics without moving layout ownership out of `mesh-core-elements`.

### `crates/core/frontend/render/src/render_object.rs`

Role: geometry dirty propagation.

Current pattern:
- `geometry_slot(node)` reads `node.layout`.
- Dirty node IDs identify changed retained render objects.

Phase 47 target:
- Tests should prove Taffy layout changes still produce geometry dirty summaries through the existing render-object path.

### Shell Test Modules

Role: shipped surface proof.

Recommended homes:
- `crates/core/shell/src/shell/component/tests/integration/real_surfaces.rs`
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs`
- `crates/core/shell/src/shell/component/tests/invalidation/profiling.rs`

Phase 47 target:
- Add tests with `phase47` in the test names so `cargo test -p mesh-core-shell phase47` is a focused verification command.

