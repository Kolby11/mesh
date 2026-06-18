# Phase 104: Retained TaffyTree - Context

**Gathered:** 2026-06-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Per-surface `TaffyTree<NodeId>` and `_mesh_key → TaffyNodeId` map retained on `FrontendSurfaceComponent` and mutated in place across frames. Only dirty nodes pay geometry recomputation cost. `compute_taffy_layout_with_cache` in `layout.rs` currently builds a fresh `TaffyTree` on every call (line 208) — this phase retains that tree and drives incremental mutations from `ComponentDirtyFlags`.

Deliverables: `PerSurfaceLayoutState` struct, incremental mutation path wired to dirty bits, `remove_taffy_subtree` post-order walk, parity tests in `layout.rs`.

</domain>

<decisions>
## Implementation Decisions

### Retained State Ownership
- `PerSurfaceLayoutState` lives as a field on `FrontendSurfaceComponent` in `shell_component.rs` — same layer that owns retained display list and render objects
- Store `last_available: (f32, f32)` alongside the retained tree; call `compute_layout` with new space when `(w, h)` changes, even if no nodes are dirty
- Set a `valid: bool = false` flag on `PerSurfaceLayoutState` in `theme_changed`, `locale_changed`, and `reload_source` resets; when false fall back to full fresh-build path for one frame then re-enter retained path

### Dirty Bit → Taffy Operation Mapping
- `VISUAL_REPAINT` only (STYLE|PAINT, no LAYOUT): call `set_style` on affected nodes — updates Taffy's cached style without marking geometry dirty
- `LAYOUT` dirty nodes: call `mark_dirty` on the node (Taffy propagates to ancestors automatically), then `compute_layout`
- `TREE_REBUILD` (SCRIPT|TEXT set): structural diff — walk new widget tree vs `_mesh_key` map; call `add_child` / `remove_taffy_subtree` / `set_children` as needed, then `compute_layout`
- Stable map key: `String` (`_mesh_key` attribute value) → `TaffyNodeId`; never use `TaffyNodeId` as the stable key (it is ephemeral)

### Parity Proof Strategy
- Compare `LayoutRect` (x, y, width, height) per node — retained output vs fresh-build output
- 5 test cases: style-only dirty, layout-dirty, add node (TREE_REBUILD), remove node (TREE_REBUILD), reorder children (TREE_REBUILD)
- Tests live in `#[cfg(test)]` block in `layout.rs`

### Claude's Discretion
- Exact shape of `PerSurfaceLayoutState` fields beyond the required `tree`, `node_map`, `last_available`, and `valid` flag
- Whether `compute_taffy_layout_with_cache` is renamed or a new incremental entry point is added alongside it
- Exact Taffy API call sequence for the available-space-changed path

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ComponentDirtyFlags` in `shell_component.rs` lines 67–141: VISUAL_REPAINT, STYLE_RELAYOUT, TREE_REBUILD, LAYOUT, STYLE — all in place
- `compute_taffy_layout_with_cache` in `layout.rs` line 200: current fresh-build entry point to replace/extend
- `build_taffy_tree`, `write_taffy_layout`, `zero_layout_subtree` in `layout.rs`: internal helpers that may need incremental variants
- `FrontendSurfaceComponent` in `shell_component.rs`: owns `retained_tree`, `retained_display_list`, `retained_render_objects` — same pattern for new `layout_state` field

### Established Patterns
- Retained state on `FrontendSurfaceComponent` with a `valid`/dirty guard (see `retained_display_list`)
- `take_dirty_for_paint()` returns `dirty_types: ComponentDirtyFlags` — use this as the input to the incremental layout path
- `theme_changed()`, `locale_changed()`, `reload_source()` on `FrontendSurfaceComponent` are the reset sites

### Integration Points
- `layout.rs`: `compute_taffy_layout_with_cache` signature change to accept `&mut PerSurfaceLayoutState`
- `shell_component.rs`: add `layout_state: PerSurfaceLayoutState` field; wire into paint path where layout is currently called
- `rendering.rs`: no expected changes — layout is called from shell_component paint path, not directly from rendering

</code_context>

<specifics>
## Specific Ideas

- `remove_taffy_subtree` must be a post-order walk: Taffy's `remove` does not recursively remove descendants; orphaned `TaffyNodeId`s accumulate silently if parent is removed first
- `_mesh_key` as the stable map key (not MESH's internal `NodeId`): this is architecturally load-bearing — using `NodeId` would cause silent layout corruption on every `TREE_REBUILD`
- `set_style` for VISUAL_REPAINT: Taffy's style includes padding, margin, gap — these affect geometry, so `set_style` must be called even for "paint-only" style changes; the distinction is that `set_style` does NOT call `mark_dirty` unless the geometry-affecting fields actually changed

</specifics>

<deferred>
## Deferred Ideas

- `rpds::Vector` rope index for the display list — that is Phase 105 scope
- Per-stage budget profiling — that is Phase 106 scope
- Available-space change detection beyond `last_available` simple comparison (e.g., hysteresis or rounding) — defer until measured as a problem

</deferred>
