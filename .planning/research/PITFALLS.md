# Domain Pitfalls: Typed Dependency Tracking (v1.18)

**Domain:** Narrow invalidation for retained-mode shell framework
**Researched:** 2026-06-07
**Confidence:** HIGH (all pitfalls identified from direct MESH codebase analysis)

## Critical Pitfalls

Mistakes that cause stale displays or incorrect invalidation.

### Pitfall 1: Inherited Style Values Not Propagated on Partial Restyle

**What goes wrong:** When a node is restyled due to a state change but its children are NOT restyled (they have no state dependency), the children's inherited text-style values (color, font-family, font-size, font-weight, line-height) become stale. The parent's `color` changed, but the child keeps its old inherited `color`.

**Why it happens:** `restyle_subtree_with_index()` currently walks the full tree — every node gets `inherit_retained_text_style()` from its parent. Under narrow restyle, only affected nodes are visited, and children of restyled-but-not-dirty nodes are invisible to the inheritance pass.

**Consequences:** Text color changes on a container but child text nodes keep old color. Font-family changes don't propagate. Visual inconsistency between parent and children.

**Prevention:** In `restyle_nodes_with_index()`, after restyling a target node, walk its direct children and re-apply `inherit_retained_text_style()` from the restyled node. Track which 5 properties inherit (color, font-family, font-size, font-weight, line-height). Only a node whose inherited-property set changed needs this pass on its children.

**Detection:** Test: change `.container:hover { color: red }` on hover → all descendant text nodes should show red. Pixel-equivalence test against full restyle baseline.

**Phase:** Selector dependency tracking (Phase A).

---

### Pitfall 2: Script + Interaction Co-Dirtying Destroys Narrow Work

**What goes wrong:** A `:hover` restyle (Phase A path, restyles ~5 nodes) and a backend service update (Phase C path, dirties 2 nodes) arrive in the same frame. The current `take_dirty_for_paint()` calls `requires_tree_rebuild` first — if SCRIPT is dirty, it jumps to `build_tree()` which discards ALL narrow invalidation work and produces a fresh tree from source.

**Why it happens:** Dirty resolution order is currently: check `requires_tree_rebuild` → `build_tree()` (full) or `restyle_retained_tree()` (retained with skip). There's no "apply narrow then merge" path.

**Consequences:** Narrow invalidation work is wasted when co-occurring with script changes. The rebuilt tree may not reflect interaction state if build-time state is stale.

**Prevention:** The dirty-type priority order must be: SCRIPT > TEXT > STATE > STYLE > LAYOUT > PAINT. If SCRIPT is dirty, skip ALL narrow invalidation — full `build_tree()` is correct. Only apply narrow restyle when SCRIPT and TEXT are clean. The existing code already implements this logic via `requires_tree_rebuild` gate — narrow invalidation must fit within the existing retained path (not add a third path).

**Detection:** Test: simultaneous `:hover` change + service state update → one frame produces correct output (not flickering stale styles).

**Phase:** Narrow invalidation (Phase C).

---

### Pitfall 3: Text Measure Cascade Into Layout

**What goes wrong:** A service field change updates a text node's content (e.g., `audio_label` changes from "65%" to "66%"). The text measure changes (different glyph width). Under narrow invalidation, only the text node is flagged dirty. But its parent flex container must recalculate because the child's intrinsic width changed.

**Why it happens:** Text measure is computed during layout by `SharedTextMeasurer`. Under narrow invalidation, the parent is not flagged LAYOUT dirty, so Taffy doesn't recompute the parent's layout, and the child's new bounds are not reflected.

**Prevention:** When a text node's content changes, after computing the new text measure, compare old vs new intrinsic size. If size changed, flag the node's ANCESTORS as LAYOUT dirty (propagate up). This is the `mark_layout_ancestors_dirty()` method on `RetainedWidgetTree`.

**Detection:** Test: service field changes a label's text → parent container re-layouts if needed. Pixel-equivalence test against full rebuild baseline.

**Phase:** Narrow invalidation (Phase C), during `invalidate_service_nodes()`.

---

### Pitfall 4: Service Field Tracking False Negatives on First Render

**What goes wrong:** On first component mount, `tracked_service_fields` is empty (no render has happened yet). A service event arrives before the first render completes. `tracked_service_fields_changed()` returns `false` (no fields tracked) → component skips invalidation → stale display.

**Why it happens:** Narrow invalidation depends on `tracked_service_fields` being populated. Before the first render, no fields have been read, so the index is empty.

**Consequences:** Components that mount and immediately receive service events show stale data.

**Prevention:** Before the first render, `tracked_service_fields` is empty. The `handle_service_event` must fall back to `TREE_REBUILD` when tracked set is empty. Alternatively, seed the tracked set with all fields from the service contract's `state_fields` definition (from `InterfaceContract`). The existing code already applies the payload unconditionally — the question is whether the downstream invalidation is correct. Since `invalidate_script_state()` → `TREE_REBUILD` handles this, and the fallback to `TREE_REBUILD` when tracked set is empty is the narrow-equivalent, this is a design constraint, not a bug.

**Detection:** Test: mount component, send service event before first render → component shows correct data.

**Phase:** Service event routing (Phase C).

---

## Moderate Pitfalls

### Pitfall 5: Nested JSON Field Indistinguishability

**What goes wrong:** Service payload is `{"playback": {"title": "Song", "artist": "Artist"}}`. A script does `playback.title`. The `__index` trap fires on `playback` (returns a table), then on `.title`. The tracker records `"playback"` as the read field. When the next update changes `playback.artist` but not `playback.title`, the tracker sees `"playback"` changed → triggers rebuild unnecessarily.

**Prevention:** Service proxy objects must support recursive nested field tracking. When `__index` on the root proxy returns a table, wrap it in a child proxy with its own `__index` that prepends the parent key path. Accessing `.title` on the child proxy records `"playback.title"`.

**Detection:** Benchmark: component reading `playback.title` is NOT invalidated when only `playback.artist` changes.

**Phase:** Service event routing (Phase C). This is deferred past the MVP because flat payloads (audio: `percent`, `muted`) are the common case.

---

### Pitfall 6: Metatable Overhead in Render Hot Path

**What goes wrong:** The `__index` metatable on service proxy fires on every field read. A render hook that reads 100 service fields pays 100 metatable dispatches. Luau metatable dispatch is ~50-100ns per call on x86_64, so 100 reads = 5-10μs — typically negligible. But 1,000 reads = 50-100μs — potentially significant for complex render hooks.

**Prevention:** Cache tracked status on the Lua side after first read. The metatable `__index` should: (1) check if field is already tracked (fast path — bit test on a Lua-side table), (2) if not tracked, record and notify Rust side (slow path — only hit once per field per lifecycle). Profile with existing `mesh-core-debug` infrastructure during Phase B.

**Detection:** Profiling shows script execution time growth >5% after field tracking is added.

**Phase:** Per-node service dependency tracking (Phase B). The existing `tracked_service_fields` already uses this pattern; per-node tracking adds the snapshot stage only, not more metatable overhead.

---

## Minor Pitfalls

### Pitfall 7: State Bitmask Non-Orthogonality (Disabled + Hover)

**What goes wrong:** `disabled` and `hovered` are independent bits in `ElementState`. A node can be both `disabled=true` and `hovered=true`. CSS cascade handles this via rule ordering (later rule wins). But if the dependency set treats bits independently, a `:hover` change on a disabled node triggers restyle that the cascade then suppresses. This is wasted work, not incorrect output.

**Prevention:** Dependency sets should still track all state bits independently. The cascade, not the dependency set, resolves conflicts. This is correct but suboptimal for mutually exclusive states. Optimization deferred.

**Detection:** Test: disabled button receives hover → no visual change, but profiling shows style restyle occurred. Acceptable for v1.18.

**Phase:** Selector dependency tracking (Phase A). Accept as known overhead.

---

### Pitfall 8: Fallback Rule Bucket Dominance

**What goes wrong:** The `StyleRuleIndex.fallback` bucket contains selectors that couldn't be indexed (universal `*`, unindexable compounds). These are ALWAYS re-evaluated during restyle. If the fallback bucket is large (>5% of rules), the benefit of narrow restyle is eroded.

**Prevention:** Audit the fallback bucket during Phase A. If >5% of rules land there, improve `selector_index_key()` to handle more compound cases. Universal selectors can be cached — evaluate once per theme change, not per-restyle.

**Detection:** `StyleRuleIndex.fallback.len()` is small relative to `rules.len()`. Audit during index construction.

**Phase:** Selector dependency tracking (Phase A). Part of the `StateRuleIndex::new()` audit.

---

## Phase-Specific Warnings

| Phase | Likely Pitfall | Mitigation |
|-------|---------------|------------|
| Phase A: Selector deps | Inherited style not propagated (Pitfall 1) | Walk children after restyle, apply `inherit_retained_text_style()` |
| Phase A: Selector deps | Fallback bucket dominance (Pitfall 8) | Audit `fallback.len()`; cache universal selectors |
| Phase A: Selector deps | State bitmask non-orthogonality (Pitfall 7) | Accept as acceptable overhead |
| Phase B: Per-node tracking | Metatable overhead (Pitfall 6) | Cache tracked status on Lua side; profile |
| Phase C: Narrow invalidation | Co-dirtying destroys work (Pitfall 2) | Respect dirty-type priority order; only narrow when SCRIPT+TEXT clean |
| Phase C: Narrow invalidation | Text measure cascade (Pitfall 3) | Propagate LAYOUT dirty to ancestors on text size change |
| Phase C: Routing | Nested fields indistinguishable (Pitfall 5) | Recursive proxy wrapping; deferred past MVP |

## Testing Strategy

### Required Equivalence Tests (CI Gate)

For every benchmark scenario (hover, open/close, slider, traversal, backend-update):

1. Run narrow invalidation path → capture `WidgetNode` tree + `PixelBuffer`
2. Run full restyle/rebuild path (force `TREE_REBUILD`) → capture same
3. Assert pixel-identical output AND widget-tree identical (all `computed_style` fields, all `layout` bounds)

### Required Profiling Tests

- `component.style` count: should drop from 1/frame to 0 most frames (only on state transition)
- `retained.inserted` / `retained.removed`: should be 0 for state-only changes
- `paint.damage_area / paint.surface_area`: should decrease

## Sources

- MESH codebase: `crates/core/shell/src/shell/component/rendering.rs` — `finalize_tree()` targeted_interaction_restyle path (L217)
- MESH codebase: `crates/core/ui/elements/src/style/resolve.rs` — `inherit_retained_text_style()` (L992-1009), inherited properties
- MESH codebase: `crates/core/shell/src/shell/component/shell_component.rs` — `handle_service_event` (L119-182), `paint()` (L289-489)
- MESH codebase: `crates/core/runtime/scripting/src/context/proxy.rs` — metatable `__index` dispatch (L152-159)
- MESH v1.3 benchmark scenarios: hover, open/close, slider, traversal, backend-update
- CSS cascade spec: inheritance behavior for `color`, `font-family`, `font-size`, `font-weight`, `line-height`
- Luau performance notes: metatable dispatch cost ~50-100ns: https://luau.org/performance/
