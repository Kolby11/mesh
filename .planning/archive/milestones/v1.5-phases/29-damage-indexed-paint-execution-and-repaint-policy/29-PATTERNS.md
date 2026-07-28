# Phase 29 - Pattern Map

## Scope

Phase 29 extends the existing retained render path. It should not create a new renderer layer, global spatial index, benchmark harness, or diagnostics channel.

## File Pattern Map

| Planned file | Role | Closest analog | Pattern to follow |
|--------------|------|----------------|-------------------|
| `crates/core/frontend/render/src/display_list.rs` | Retained command ownership, damage/span metadata, policy accounting, focused render tests | Existing Phase 28 subtree cache and Phase 27 pruning metrics in the same file | Keep metadata private to `mesh-core-render`, expose only narrow methods and aggregate metrics. Tests should use local `WidgetNode` fixtures and assert exact command counts/order. |
| `crates/core/frontend/render/src/surface/mod.rs` | Paint entrypoint and traversal profiling boundary | Existing `paint_display_list_for_module_with_profiling_metrics(...)` | Preserve tooltip overlay outside traversal timing. Add filtered command input without moving tooltip work into display-list traversal. |
| `crates/core/frontend/render/src/surface/painter/tree.rs` | Ordered display-list traversal | Existing `render_display_list_for_module(...)` | The painter consumes an already selected ordered command input. It may keep command clip intersection, but it should not own the damage index. |
| `crates/core/foundation/debug/src/lib.rs` | Shared debug data contract | Existing `RetainedPaintSnapshot` fields for subtree reuse, damage, pruning, batching | Add aggregate fields for repaint policy and filtered execution. Use stable primitive data types suitable for JSON serialization. |
| `crates/core/shell/src/shell/component.rs` | Shell-side conversion from render metrics to debug snapshots | Existing `retained_paint_snapshot(...)` | Map new render metrics into `RetainedPaintSnapshot`; do not compute retained command filtering here. |
| `crates/core/shell/src/shell/component/shell_component.rs` | Shell orchestration of retained display list, effective damage, paint call, tooltip damage | Existing `select_effective_damage(...)`, `select_damage_policy(...)`, and retained paint call | Shell may select effective damage and pass policy/damage to render-owned filtering, but command-span metadata stays in `display_list.rs`. |
| `crates/core/shell/src/shell/runtime/debug.rs` | `mesh.debug` serialization | Existing `profiling_invalidation_json(...)` paint object | Add fields under `invalidation.paint`, not a sibling diagnostics channel. |
| `crates/core/shell/src/shell/tests.rs` | Debug JSON contract tests | Existing profiling invalidation assertions | Assert exact new policy/filter counters in the existing payload shape. |
| `crates/core/shell/src/shell/component/tests.rs` | Shipped-surface benchmark proof | Phase 26 real-surface baseline proof test | Reuse canonical scenario IDs and record Phase 29 proof values without introducing a new benchmark system. |

## Data Flow

1. Retained widget/render-object updates produce dirty summaries and dirty node IDs.
2. `RetainedDisplayList` updates subtree command caches and span metadata.
3. Shell selects effective damage and policy from render metrics, tooltip damage, reorder damage, and rebuild state.
4. `RetainedDisplayList` produces a full or filtered ordered command input for paint.
5. The software painter traverses only that command input, preserving order and clip checks.
6. Render metrics flow into `RetainedPaintSnapshot` and then into `mesh.debug.profiling.surfaces[].invalidation.paint`.

## Implementation Guardrails

- Keep command-span ownership inside `mesh-core-render`.
- Preserve the existing display-list command order for every filtered paint input.
- Treat scrollbars as display-list-owned chrome attached to the owning node/subtree.
- Keep tooltip rendering outside display-list traversal.
- Fall back to full-surface repaint whenever clip, z-order, transform ancestry, root state, or dirty summaries make filtered execution unclear.
- Publish aggregate proof only; no per-command trace stream in Phase 29.
