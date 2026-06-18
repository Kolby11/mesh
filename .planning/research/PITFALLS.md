# Pitfalls Research

**Domain:** Retrofitting retained Taffy layout tree and rope-style display list onto an existing incremental shell UI framework
**Researched:** 2026-06-18
**Confidence:** HIGH — grounded in actual MESH source code (layout.rs, display_list.rs, runtime_tree.rs, component.rs) plus verified Taffy 0.10.1 API behavior from Context7

---

## Critical Pitfalls

### Pitfall 1: TaffyTree::remove Does Not Recursively Remove Descendants

**What goes wrong:**
When a widget subtree is deleted from the MESH `WidgetNode` tree, the corresponding `TaffyNodeId`s for all descendant nodes must be removed from the retained `TaffyTree`. If you call `tree.remove(parent_taffy_id)`, Taffy removes the parent but **leaves all descendants as orphaned root nodes** that consume memory indefinitely. The `total_node_count()` keeps growing every frame a new subtree is inserted in place of an old one.

**Why it happens:**
Taffy's documented behavior for `remove()` is: detach from parent, clear `parent` pointer of direct children, keep descendants in tree as orphans. Engineers coming from arena allocators expect "remove the subtree" semantics — Taffy does not provide this.

**How to avoid:**
Before calling `tree.remove(taffy_id)`, walk the MESH `WidgetNode` subtree being deleted and collect all descendant `TaffyNodeId`s from the `widget_id → taffy_id` map. Call `tree.remove(taffy_id)` for each in **leaf-first, post-order** traversal so children are detached before parents. In the `node_id → taffy_id` bidirectional map, erase all entries for the removed subtree at the same time.

```rust
fn remove_taffy_subtree(
    node: &WidgetNode,
    taffy: &mut TaffyTree<NodeId>,
    id_map: &mut HashMap<NodeId, TaffyNodeId>,
) {
    // Post-order: children first
    for child in &node.children {
        remove_taffy_subtree(child, taffy, id_map);
    }
    if let Some(taffy_id) = id_map.remove(&node.id) {
        let _ = taffy.remove(taffy_id); // parent detachment is automatic
    }
}
```

**Warning signs:**
- `TaffyTree::total_node_count()` grows across frames even when the widget tree size is stable
- Memory usage grows monotonically during widget-replacing operations (e.g., list rebuilds, tab switches)
- Layout pass takes progressively longer due to accumulating ghost nodes

**Phase to address:** Retained TaffyTree phase (first phase of v1.21)

---

### Pitfall 2: Style-Only Dirty Path Must Still Call mark_dirty Before compute_layout

**What goes wrong:**
MESH's `STYLE_ONLY` dirty path (currently `ComponentDirtyFlags::STYLE | PAINT` without `LAYOUT`) correctly skips layout recomputation for pure color/opacity/font changes that don't affect geometry. When the retained `TaffyTree` is introduced, the old behavior of rebuilding the tree from scratch each frame meant stale Taffy state was never a problem. With a retained tree, if a `STYLE` change affects a property that Taffy uses (flexbox gap, padding, border-width, font-size driving text measurement) and `set_style()` or `mark_dirty()` is not called, `compute_layout` returns cached layout data that no longer reflects the node's current style.

**Why it happens:**
The style-property-to-layout-impact mapping is not bijective. Engineers classify `padding` as a "style" property for paint purposes (changes box fill) but it is also a layout input. Without calling `tree.set_style(taffy_id, updated_style)` the dirty bit says "no layout needed" but Taffy's internal cache disagrees with what the current `ComputedStyle` now says.

**How to avoid:**
When applying a style update to the retained tree, always call `tree.set_style(taffy_id, taffy_style_for_node(node, &mut report))` regardless of whether MESH considers it layout-affecting. `set_style` calls `mark_dirty` internally (HIGH confidence from Context7 docs). Taffy's internal dirty propagation to ancestors means only affected subtrees are re-solved on the next `compute_layout`. The cost of `set_style` on a clean tree is negligible compared to a full rebuild.

Alternatively, create an explicit Taffy-affecting property set and gate the `set_style` call on membership in that set:
```rust
fn taffy_style_affecting(old: &ComputedStyle, new: &ComputedStyle) -> bool {
    old.width != new.width
        || old.height != new.height
        || old.padding != new.padding
        || old.margin != new.margin
        || old.gap != new.gap
        || old.flex_grow != new.flex_grow
        || old.font_size != new.font_size  // text measurement changes
        || old.display != new.display
        || old.position != new.position
        || old.flex_direction != new.flex_direction
}
```

**Warning signs:**
- Visual geometry mismatches after animations that change `padding` or `font-size`
- A component renders correctly on first load, then layout drifts after a CSS animation completes
- `STYLE`-flagged dirty cycles produce correct paint but wrong hit-testing (hit boxes lag behind visual)
- `tree.dirty(taffy_id)` returns `false` but the rendered layout visually disagrees with the current style

**Phase to address:** Retained TaffyTree phase — define the style-to-Taffy-impact mapping before wiring the retained path.

---

### Pitfall 3: Rope Subtree Span Invalidation After z-index Reorder

**What goes wrong:**
The existing `RetainedDisplayList` stores `RetainedPaintSubtree` per node keyed by `NodeId`. When z-index changes cause `compute_child_order` to produce a different paint order, the existing `child_order: Option<Arc<[usize]>>` inside each `RetainedPaintSubtree` becomes stale. A rope-style extension that references child spans by offset assumes sibling order is stable — a z-index change invalidates that assumption for every ancestor of the reordered node, not just the reordered node itself.

**Why it happens:**
z-index reordering is a paint-only change: no geometry changes, so `LAYOUT` is not dirty. MESH's `PAINT` dirty flag fires but affects only the local subtree. A rope segment for an ancestor node stores its children's command spans by their previous paint order; after reorder, those offset ranges map to the wrong commands.

**How to avoid:**
When any node in a subtree has a dirty `PAINT` flag and the previous and current `child_order` arrays differ, mark the entire parent chain up to the surface root as needing span recomputation. In practice: compare `old_child_order` to `new_child_order` during the dirty-node walk and if they differ, add the parent to the dirty set. This is already partially handled by `collect_dirty_ancestor_ids` — the gap is that the current code only walks ancestors of `dirty_node_ids` when deciding to rebuild, but z-index changes must also trigger ancestor span recomputation even when the z-reordered node's content is clean.

Alternatively, store the `child_order` as part of the rope segment's identity hash — any order change causes the segment to be rebuilt rather than reused.

**Warning signs:**
- After a z-index CSS animation, sibling nodes paint in the wrong stacking order without a full surface redraw
- `entries_reused` metric stays high while visual stacking is wrong
- The discrepancy only appears for a frame or two then self-corrects when a broader dirty fallback fires

**Phase to address:** Rope display list phase — before wiring span-level reuse, audit the child_order invalidation path.

---

### Pitfall 4: Orphaned TaffyNodeId After TREE_REBUILD Causes Silent Lookup Failures

**What goes wrong:**
When `ComponentDirtyFlags::TREE_REBUILD` fires (full Luau re-evaluation), the `WidgetNode` tree is rebuilt from scratch with new `NodeId` values (monotonically increasing via `NEXT_NODE_ID`). If the retained `TaffyTree` keeps the old `TaffyNodeId` entries and a new `widget_id → taffy_id` map is computed by diffing, there is a race window where the old `taffy_id` is still in the `TaffyTree` but the MESH `WidgetNode` with the same logical identity has a different `NodeId`. The diff will correctly create new nodes for new IDs but any lookup by `_mesh_key` (the stable author key) that is used to match old TaffyNodeIds to new WidgetNode IDs must tolerate the transition — stale entries in the bidirectional map will silently return wrong geometry.

**Why it happens:**
MESH uses `NodeId` (a monotonically incrementing u64) for identity within a single frame's tree, and uses `_mesh_key` (an author-provided string) as the stable cross-frame identity for retained rendering. The retained tree's `node_id → taffy_id` map is keyed by the ephemeral `NodeId`, not the stable `_mesh_key`. After TREE_REBUILD, old `NodeId` values are not reused, so the old map entries are dead weight. If the transition code path that handles TREE_REBUILD does not flush the old map and rebuild it from `_mesh_key`-based matching, lookups silently hit stale entries for nodes whose `_mesh_key` survived but whose `NodeId` changed.

**How to avoid:**
Use `_mesh_key` as the primary key for the retained Taffy map, not `NodeId`. Store `HashMap<String, TaffyNodeId>` keyed on `_mesh_key`. For nodes without a `_mesh_key`, fall back to structural position (parent key + child index). On TREE_REBUILD, diff the old and new `_mesh_key` sets: create new TaffyNodeIds for new keys, call `remove_taffy_subtree` for dropped keys, update style for surviving keys. This eliminates the stale-NodeId problem at the cost of slightly more complex diffing.

```rust
// Key type: stable author identity, not ephemeral NodeId
type TaffyMap = HashMap<MeshKey, TaffyNodeId>;

fn mesh_key(node: &WidgetNode, parent_key: &str, child_index: usize) -> MeshKey {
    node.attributes
        .get("_mesh_key")
        .map(|k| MeshKey::Author(k.clone()))
        .unwrap_or_else(|| MeshKey::Positional(format!("{parent_key}/{child_index}")))
}
```

**Warning signs:**
- Layout is correct immediately after a full page load, then drifts after any Luau script update that triggers TREE_REBUILD
- `write_taffy_layout` writes geometry to `WidgetNode` but values are from the previous tree's nodes
- Nodes with author-assigned `_mesh_key` display correct layout while adjacent nodes without keys show wrong sizes

**Phase to address:** Retained TaffyTree phase — the key scheme must be designed before wiring any retained path.

---

### Pitfall 5: Rope Segment Reuse Across Scroll Position Changes Produces Stale Clip Offsets

**What goes wrong:**
`RetainedPaintSubtree` stores `DisplayPaintCommand` with absolute screen coordinates including scroll offsets baked into `layout.x` / `layout.y`. When a scrollable container's scroll position changes, every descendant's absolute position changes — but if the scroll container's subtree is considered "clean" (no style or layout dirty bits), the rope segment is reused verbatim. The display commands then paint children at the wrong positions.

**Why it happens:**
Scroll offset is stored as `_mesh_scroll_x` / `_mesh_scroll_y` attributes on the container node. These are runtime attributes, not CSS properties, so they do not flow through the `ComputedStyle` dirty mechanism. When scroll position updates, the attribute changes but does not currently fire `LAYOUT` dirty — only the paint traversal knows to re-apply the offset. With a retained paint subtree keyed on `NodeId` that uses the previous frame's baked coordinates, the scroll update is invisible to the reuse decision.

**How to avoid:**
Scroll offset changes must dirty the entire subtree of the scrolling container at the display-list level. Add a scroll-offset change detector in the retained tree update path: before deciding to reuse a subtree, compare the current `_mesh_scroll_x` / `_mesh_scroll_y` of the node against the previous frame's values. If they differ, force-rebuild the subtree and add the scroll container to the dirty ancestors set. Alternatively, do not bake scroll offsets into `DisplayPaintCommand.node.layout` at storage time — instead store layout-relative coordinates and apply the scroll transform during the paint traversal (not during command storage). This is the architecturally cleaner approach and eliminates the scroll-offset staleness class entirely at the cost of a slightly more complex painter path.

**Warning signs:**
- Scrollable lists repaint with children stuck at their pre-scroll positions for one frame
- `entries_reused` is high in the metrics during active scroll drag
- The issue only appears at 60fps under scroll (fast dirty resolution masks it at lower frame rates)

**Phase to address:** Rope display list phase — design coordinate storage (absolute vs. relative) as the first architectural decision.

---

### Pitfall 6: Profiling Overhead Inserted Into the Hot Layout/Paint Path

**What goes wrong:**
Per-stage budget profiling requires timestamps at the start and end of each `ProfilingStage`. If the timestamp calls are not properly gated behind the existing `profiling_enabled` flag, or if the timer infrastructure allocates on every call, the profiling code adds measurable overhead to the very paths being measured. A common mistake is inserting `std::time::Instant::now()` unconditionally in the layout loop and then checking the flag before recording — `Instant::now()` on Linux is a syscall that takes ~20ns each call; at 200 nodes per frame that is 4µs of overhead per frame even when profiling is off.

**Why it happens:**
Budget profiling is added after the feature works, as a second pass. Engineers instrument every stage entry point for completeness but forget to gate the timestamp acquisition itself, only gating the aggregation.

**How to avoid:**
Wrap both the `Instant::now()` call and the delta recording behind the flag:

```rust
macro_rules! stage_start {
    ($debug:expr, $stage:expr) => {
        if $debug.profiling_enabled { Some(std::time::Instant::now()) } else { None }
    };
}
macro_rules! stage_end {
    ($debug:expr, $stage:expr, $start:expr) => {
        if let Some(t) = $start {
            $debug.record_stage_time($stage, t.elapsed());
        }
    };
}
```

For the layout loop specifically, check the flag once before entering the loop and use a `Duration::ZERO` accumulator path when off. Do not add per-node profiling unless a node-level breakdown is explicitly needed — per-node timestamps in a 200-node tree would add 400 `Instant::now()` calls per frame.

**Warning signs:**
- Baseline frame time increases after adding profiling even when the debug overlay is closed
- Profiling overhead itself appears as noise in the `Layout` stage timing
- Flamegraph shows `clock_gettime` syscalls proportional to tree size rather than proportional to dirty node count

**Phase to address:** Per-stage budget profiling phase — design the gating pattern before instrumenting.

---

### Pitfall 7: Rope Dirty Fallback Threshold Misaligned With Existing >50% Threshold

**What goes wrong:**
`RetainedDisplayList::local_reuse_decision` already has a `dirty_node_ids.len() > broad_limit` fallback that forces a full rebuild when more than ~50% of subtrees are dirty. The rope-style extension will add a second threshold decision at the command-span level. If the two thresholds are tuned independently, they can produce inconsistent behavior: the display list decides to do a partial update but the span-level rope decides to fall back to full, or vice versa. This causes metrics reporting that misrepresents what actually happened.

**Why it happens:**
Two independent fallback decisions for the same dirty set. The display list's decision is made in `local_reuse_decision` before `build_paint_subtree`; the rope's span-level decision would be made during span assembly. They share the same `dirty_node_ids` input but apply different thresholds and reason about different granularities.

**How to avoid:**
Make the rope span-level fallback read the same decision that `local_reuse_decision` already computed rather than computing a second threshold. Thread the `LocalReuseDecision` enum through to span assembly so the span path always agrees with the subtree path. If the display list decided `FallbackFull`, spans must also use full rebuild. The fallback threshold is a single policy decision, not two.

**Warning signs:**
- `full_fallback_count` and `broad_dirty_fallback_count` are both zero but span reuse is also zero — indicates an untracked third fallback path
- The `filtered_commands_skipped` metric diverges from what the dirty summary predicts
- Debug inspector shows "partial update" in display list metrics but "full surface damage" in paint metrics

**Phase to address:** Rope display list phase — unify the fallback decision point before both layers exist separately.

---

### Pitfall 8: TaffyTree Retained Across Surface Resize Serves Stale Available Space

**What goes wrong:**
The retained `TaffyTree` is keyed per surface. When a surface resizes (anchor changes, user drags a panel, `SurfaceSizePolicy::ContentMeasured` adjusts size), the available space passed to `compute_layout_with_measure` changes but the retained Taffy tree's internal layout cache was computed for the old available space. Taffy's `mark_dirty` propagates from a changed node upward to ancestors but it does not propagate downward to children. A surface resize requires re-solving the root with the new available space — but if no node is marked dirty, Taffy returns cached results computed for the old dimensions.

**Why it happens:**
`compute_layout(root, new_available_space)` in Taffy will recompute if the root is dirty or if `available_space` changes relative to the stored cache. The key is whether Taffy internally treats an `available_space` change as requiring a recompute. Per Taffy's behavior, it does not automatically re-dirty the root when only the available space changes — the root must be explicitly marked dirty or the whole tree rebuilt. Engineers relying on Taffy to "notice" the new available space are mistaken.

**How to avoid:**
When the surface's logical size changes (detected by comparing current `(width, height)` to the previously stored dimensions), call `tree.mark_dirty(root_taffy_id)` explicitly before `compute_layout_with_measure`. Alternatively, compare the `available_space` argument to the previous frame's value and if it differs, force a root mark_dirty regardless of other dirty bits. This is cheap — mark_dirty on the root propagates in O(1).

```rust
if self.last_available_width != available_width
    || self.last_available_height != available_height
{
    if let Some(root_taffy_id) = self.taffy_root {
        let _ = self.taffy.mark_dirty(root_taffy_id);
    }
    self.last_available_width = available_width;
    self.last_available_height = available_height;
}
```

**Warning signs:**
- After a surface anchor changes or window is resized, widgets maintain their old sizes for one or more frames
- Nodes with `width: 100%` or `flex-grow: 1` show incorrect sizes after resize
- `ContentMeasured` surfaces grow/shrink but children don't reflow to the new constraint

**Phase to address:** Retained TaffyTree phase — add available-space change detection as a required invariant check.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Keep `widget_id → taffy_id` as `HashMap<NodeId, TaffyNodeId>` (ephemeral key) instead of `_mesh_key` | Simpler initial implementation | TREE_REBUILD silently serves stale layout; requires full map flush on every script invalidation | Only if TREE_REBUILD always fully rebuilds the Taffy tree too (negating retention benefit) |
| Skip `remove_taffy_subtree` on TREE_REBUILD (just call `TaffyTree::new()`) | Avoids subtree-walk logic | Memory-safe but loses all retention benefit for full rebuilds; the retained tree helps nothing on the most common invalidation path | Acceptable as a v1 starting point if TREE_REBUILD fallback is explicitly flagged as a perf gap to close |
| Bake absolute scroll-offset coordinates into rope segments | Simpler command storage | Scroll changes cause full subtree invalidation defeating rope reuse for any scrollable content | Never for scrollable content; acceptable if MESH surfaces never scroll (they currently do) |
| Always call `set_style` on every node every frame (no dirty check) | Eliminates stale-style bug class | Taffy's `set_style` calls `mark_dirty` which propagates to all ancestors; at 200 nodes this is 200 ancestor walks per frame | Only during initial integration; remove before shipping |
| Single global profiling timestamp per frame for each stage | Zero per-node overhead | Cannot attribute budget overruns to specific subtrees or node types | Acceptable for milestone 1 budget profiling; per-subtree later |

---

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| `TaffyTree::remove` | Calling remove on a subtree root and assuming descendants are cleaned up | Walk the MESH subtree post-order, remove leaf TaffyNodeIds first, then parents |
| `mark_dirty` propagation | Assuming dirty propagates downward to children | `mark_dirty` propagates upward to ancestors only; children must be marked separately if their constraints change |
| `set_style` on style-only changes | Skipping `set_style` for non-geometric style properties | Call `set_style` any time `ComputedStyle` changes — Taffy determines internally if it affects layout |
| `_mesh_scroll_x` / `_mesh_scroll_y` attributes | Treating scroll as a style change that flows through ComputedStyle dirty | Scroll position changes bypass the ComputedStyle path; must be tracked separately for retained coordinate validity |
| Rope segment `child_order` | Treating z-index paint order as paint-only, not span-structure-affecting | z-index reorder invalidates all ancestor span offsets; propagate dirty to ancestors on order change |
| `SCRIPT_NARROW` path | Assuming narrow script updates are always geometry-preserving | `SCRIPT_NARROW` can change text content, which changes text measurement, which changes layout geometry — must re-run Taffy for affected text nodes |

---

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Unconditional `set_style` on every node per frame | Layout stage time is O(node count) even when nothing changed | Gate `set_style` calls on style diff; only call when `taffy_style_affecting()` returns true | Immediately with 100+ node trees |
| `Instant::now()` outside profiling gate | Baseline frame time increases by 2-5µs per 100 nodes even with profiling off | Macro-gate both the `Instant::now()` acquisition and the delta record | At any tree size in production |
| Rope segment clones for ancestor traversal | GC pressure from `Arc<[DisplayPaintCommand]>` clone-on-read during dirty ancestor walk | Use span index ranges into a flat command buffer instead of per-node `Arc` clones | At tree depth > 8 or > 50 dirty nodes |
| Full `TaffyTree::new()` on every TREE_REBUILD | Retaining the tree is never cheaper than rebuilding for the most common invalidation path | Use `_mesh_key`-based diffing so TREE_REBUILD preserves stable subtrees | Immediately — TREE_REBUILD fires on every script event without this |

---

## "Looks Done But Isn't" Checklist

- [ ] **Retained TaffyTree:** Verify `TaffyTree::total_node_count()` is stable across 1000 frames of a list that inserts/removes items — if it grows, `remove_taffy_subtree` is incomplete
- [ ] **Stale style:** Verify that a CSS animation changing `padding` over 500ms produces correctly reflowing siblings at every keyframe, not just the first and last
- [ ] **Scroll rope:** Verify that fast-scrolling a list (> 5 items/frame) produces zero stale-position paint artifacts
- [ ] **Surface resize:** Verify that switching a surface from `anchor: top` to `anchor: bottom` (which changes available height) immediately produces correct child geometry, not one-frame-stale geometry
- [ ] **TREE_REBUILD identity:** Verify that a `_mesh_key`-tagged node that survives a TREE_REBUILD cycle does not cause Taffy to run a full layout solve for its unchanged subtree
- [ ] **Profiling overhead:** Verify that enabling then disabling profiling does not leave any unconditional timing code active; measure baseline frame time with and without the profiling feature compiled in
- [ ] **z-index reorder:** Verify that animating `z-index` between two siblings produces correct stacking every frame, not only on the first and last frame of the animation

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Orphaned TaffyNodeId memory leak | MEDIUM | Add `remove_taffy_subtree` post-order walk; audit all call sites that remove MESH nodes to ensure they go through the walk |
| Stale style → wrong layout geometry | MEDIUM | Switch to unconditional `set_style` on every style update as a safe fallback; optimize with diff later |
| Stale scroll coordinates in rope | HIGH | Switch coordinate storage to layout-relative (remove scroll offset from stored commands); requires touching all command readers |
| TREE_REBUILD key confusion (NodeId vs _mesh_key) | HIGH | Refactor `TaffyMap` key type before any retained behavior is production-wired; cannot be patched incrementally |
| Misaligned fallback thresholds | LOW | Thread `LocalReuseDecision` through to span assembly; single-site fix |
| Profiling overhead in production | LOW | Wrap `Instant::now()` in gate macro; search for unchecked `Instant::now()` calls in the layout/paint path |
| Surface resize serving stale available space | LOW | Add available-space change detector before `compute_layout_with_measure`; one-site fix |

---

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| TaffyTree::remove not recursive | Retained TaffyTree phase | `total_node_count()` stable over 1000-frame list insert/remove stress test |
| Style-only dirty misses mark_dirty | Retained TaffyTree phase | CSS padding animation regression test at all keyframes |
| z-index reorder invalidates ancestor spans | Rope display list phase | z-index animation correctness test every frame, not just first/last |
| Orphaned TaffyNodeId after TREE_REBUILD | Retained TaffyTree phase | TREE_REBUILD under script event stress, verify no memory growth |
| Stale scroll coordinates in rope | Rope display list phase | Scroll stress test: 100 fast-scroll frames, zero stale-position artifacts |
| Profiling overhead off-path | Per-stage budget profiling phase | Frame time regression test: baseline must not change with profiling compiled in but disabled |
| Rope fallback threshold mismatch | Rope display list phase | Verify `full_fallback_count + subtree_segments_reused + subtree_segments_rebuilt` accounts for 100% of nodes each frame |
| Surface resize stale available space | Retained TaffyTree phase | Anchor-switch test: immediate correct geometry on frame after resize |
| _mesh_key vs NodeId map key confusion | Retained TaffyTree phase (design step 1) | TREE_REBUILD cycle preserves stable-key nodes without full Taffy re-solve |

---

## Sources

- Taffy 0.10.1 API — `TaffyTree::remove` orphans children: verified via Context7 `/dioxuslabs/taffy`; HIGH confidence
- Taffy 0.10.1 API — `TaffyTree::set_style` automatically calls `mark_dirty`: verified via Context7 `/dioxuslabs/taffy`; HIGH confidence
- Taffy 0.10.1 API — `mark_dirty` propagates upward to ancestors only, not downward: verified via Context7 `/dioxuslabs/taffy`; HIGH confidence
- MESH `ComponentDirtyFlags` and `TREE_REBUILD` definition: `crates/core/shell/src/shell/component.rs` lines 67-125
- MESH retained display list span/subtree structure: `crates/core/frontend/render/src/display_list.rs`
- MESH `build_taffy_tree` full rebuild per frame: `crates/core/ui/elements/src/layout.rs` lines 206-272
- MESH scroll offset as runtime attributes `_mesh_scroll_x` / `_mesh_scroll_y`: `display_list.rs` attribute reads in `collect_display_entries` and `build_paint_subtree`
- MESH `RetainedNodeDirtyFlags` and `RetainedTreeDirtySummary`: `crates/core/shell/src/shell/component/runtime_tree.rs`
- MESH `ProfilingStage` enum: `crates/core/foundation/debug/src/lib.rs` lines 357-396
- MESH `local_reuse_decision` 50% broad-dirty threshold: `display_list.rs` line 1219
- SlotMap generational index stale-key detection: [Generational indices guide](https://lucassardois.medium.com/generational-indices-guide-8e3c5f7fd594)

---
*Pitfalls research for: Adding retained Taffy layout tree and rope-style display list to MESH v1.21*
*Researched: 2026-06-18*
