# Phase 104: Retained TaffyTree - Research

**Researched:** 2026-06-18
**Domain:** Taffy layout engine retention, incremental mutation, dirty-flag routing
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Retained State Ownership**
- `PerSurfaceLayoutState` lives as a field on `FrontendSurfaceComponent` in `shell_component.rs` — same layer that owns retained display list and render objects
- Store `last_available: (f32, f32)` alongside the retained tree; call `compute_layout` with new space when `(w, h)` changes, even if no nodes are dirty
- Set a `valid: bool = false` flag on `PerSurfaceLayoutState` in `theme_changed`, `locale_changed`, and `reload_source` resets; when false fall back to full fresh-build path for one frame then re-enter retained path

**Dirty Bit → Taffy Operation Mapping**
- `VISUAL_REPAINT` only (STYLE|PAINT, no LAYOUT): call `set_style` on affected nodes — updates Taffy's cached style without marking geometry dirty
- `LAYOUT` dirty nodes: call `mark_dirty` on the node (Taffy propagates to ancestors automatically), then `compute_layout`
- `TREE_REBUILD` (SCRIPT|TEXT set): structural diff — walk new widget tree vs `_mesh_key` map; call `add_child` / `remove_taffy_subtree` / `set_children` as needed, then `compute_layout`
- Stable map key: `String` (`_mesh_key` attribute value) → `TaffyNodeId`; never use `TaffyNodeId` as the stable key (it is ephemeral)

**Parity Proof Strategy**
- Compare `LayoutRect` (x, y, width, height) per node — retained output vs fresh-build output
- 5 test cases: style-only dirty, layout-dirty, add node (TREE_REBUILD), remove node (TREE_REBUILD), reorder children (TREE_REBUILD)
- Tests live in `#[cfg(test)]` block in `layout.rs`

### Claude's Discretion
- Exact shape of `PerSurfaceLayoutState` fields beyond the required `tree`, `node_map`, `last_available`, and `valid` flag
- Whether `compute_taffy_layout_with_cache` is renamed or a new incremental entry point is added alongside it
- Exact Taffy API call sequence for the available-space-changed path

### Deferred Ideas (OUT OF SCOPE)
- `rpds::Vector` rope index for the display list — Phase 105 scope
- Per-stage budget profiling — Phase 106 scope
- Available-space change detection beyond `last_available` simple comparison (e.g., hysteresis or rounding)
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| LAYOUT-01 | `TaffyTree` and `_mesh_key → TaffyNodeId` map are retained per surface and mutated in place across frames instead of rebuilt from scratch each layout pass | `PerSurfaceLayoutState` struct design; TaffyTree API verified in docs.rs |
| LAYOUT-02 | STYLE-only dirty nodes call `set_style` without geometry invalidation; LAYOUT-dirty nodes call `mark_dirty` / `set_children` to propagate geometry invalidation | Taffy source confirms `set_style` calls `mark_dirty` internally — critical nuance documented below |
| LAYOUT-03 | Structural changes (node add/remove/reorder) use `_mesh_key` as the stable identity key — not the ephemeral `TaffyNodeId` — so `TREE_REBUILD` frames never serve stale geometry | `_mesh_key` confirmed throughout codebase; `NodeId` (u64) is per-run ephemeral |
| LAYOUT-04 | `remove_taffy_subtree` performs a post-order walk to remove all descendants before removing a parent (Taffy does not recursively remove) | Taffy source code verified: `remove()` orphans children, does NOT recurse |
| LAYOUT-05 | Layout output is pixel-equivalent to the current per-frame rebuild approach across style-only, layout-dirty, and tree-rebuild dirty scenarios | Test strategy: run both paths on same tree, compare `LayoutRect` per node |
</phase_requirements>

## Summary

Phase 104 introduces per-surface retained `TaffyTree` state on `FrontendSurfaceComponent` so that layout geometry computation is incremental rather than rebuilt from scratch on every paint. The existing `compute_taffy_layout_with_cache` in `layout.rs` builds a fresh `TaffyTree` on every call (line 208); this phase retains that tree between frames and drives incremental mutations from `ComponentDirtyFlags`.

The key insight from reading Taffy 0.10.1 source: `set_style` always calls `mark_dirty` internally. This means the CONTEXT.md's claim that `set_style` "does NOT call `mark_dirty` unless geometry-affecting fields actually changed" is partially incorrect — Taffy marks dirty unconditionally on `set_style`. The implication: calling `set_style` for VISUAL_REPAINT-only changes will still trigger geometry recomputation unless the planner avoids calling `set_style` on unchanged geometry fields. The correct interpretation is that `set_style` DOES mark dirty, but `mark_dirty` is idempotent (stops propagation early if already dirty). For VISUAL_REPAINT paths (no LAYOUT bit), the implementation should either skip `compute_layout` entirely (since geometry hasn't changed), or rely on Taffy's internal cache to make recomputation cheap.

The structural diff algorithm (`TREE_REBUILD`) requires walking the new widget tree against the retained `_mesh_key → TaffyNodeId` map to detect additions, removals, and reorders. The `remove_taffy_subtree` helper must use post-order traversal because `TaffyTree::remove()` only detaches a node from its parent and orphans its children — it does not recursively free descendants.

**Primary recommendation:** Add `PerSurfaceLayoutState` as a field on `FrontendSurfaceComponent`, wire it into `finalize_tree()` in `rendering.rs` at the site where `LayoutEngine::compute_with_intrinsic_cache_and_measurer` is called, and implement the three dirty-level paths (VISUAL_REPAINT skip, LAYOUT mark_dirty+compute, TREE_REBUILD structural diff+compute) inside `layout.rs` as a new `LayoutEngine` entry point that accepts `&mut PerSurfaceLayoutState`.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| `PerSurfaceLayoutState` struct | `mesh-core-elements` (`layout.rs`) | `mesh-core-shell` (`component.rs`) | Layout types belong in elements; state is owned by shell component |
| Retained state field ownership | `mesh-core-shell` (`shell_component.rs`) | — | Same layer as `retained_tree`, `retained_display_list` |
| Incremental layout entry point | `mesh-core-elements` (`layout.rs`) | — | `LayoutEngine` lives here; all Taffy calls stay local |
| Dirty-flag dispatch to layout tier | `mesh-core-shell` (`rendering.rs`) | — | `finalize_tree()` already dispatches dirty-flag-conditional layout at line 332 |
| Post-order subtree removal helper | `mesh-core-elements` (`layout.rs`) | — | Operates on `TaffyTree`; co-located with other tree helpers |
| Parity tests | `mesh-core-elements` (`layout.rs` `#[cfg(test)]`) | — | Tests call `LayoutEngine` directly, no shell dependency needed |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| taffy | 0.10.1 | Flexbox/grid layout engine | Already in workspace; `TaffyTree` is the retained structure |

No new dependencies required for this phase. [VERIFIED: Cargo.toml line 80]

**Installation:** No new packages needed.

## Package Legitimacy Audit

No new packages are introduced in this phase. Taffy 0.10.1 is already a workspace dependency.

| Package | Registry | Age | Downloads | Source Repo | slopcheck | Disposition |
|---------|----------|-----|-----------|-------------|-----------|-------------|
| taffy | crates.io | 4+ yrs | Millions | github.com/DioxusLabs/taffy | N/A (existing dep) | Already in use |

**Packages removed due to slopcheck [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
FrontendSurfaceComponent.paint()
  └─ take_dirty_for_paint() → dirty_types: ComponentDirtyFlags
       └─ finalize_tree()  [rendering.rs ~line 332]
            └─ if !reused_retained_layout:
                 LayoutEngine::compute_incremental(
                   tree: &mut WidgetNode,
                   layout_state: &mut PerSurfaceLayoutState,
                   dirty_types: ComponentDirtyFlags,
                   width, height,
                   intrinsic_cache,
                   measurer,
                 )
                   ├─ if !layout_state.valid → fresh_build path (sets valid=true)
                   ├─ if available changed → compute_layout only (no node mutations)
                   ├─ if TREE_REBUILD → structural_diff() + compute_layout
                   ├─ if LAYOUT dirty → mark_dirty affected nodes + compute_layout
                   └─ if VISUAL_REPAINT only → set_style on affected nodes, SKIP compute_layout
```

Key: layout output flows back into `write_taffy_layout()` which sets `node.layout` on the `WidgetNode` tree.

### Recommended Project Structure

Changes are contained within two existing files:

```
crates/core/ui/elements/src/
  layout.rs           ← add PerSurfaceLayoutState, LayoutEngine::compute_incremental,
                         remove_taffy_subtree, structural_diff helpers; parity tests
crates/core/shell/src/shell/component/
  component.rs        ← add layout_state: PerSurfaceLayoutState field + reset in
                         theme_changed / locale_changed; init in constructor
  shell_component.rs  ← wire valid=false into theme_changed, locale_changed, reload_source
  rendering.rs        ← change call site at ~line 338 to use new incremental entry point
```

No new files needed; no new crates.

### Pattern 1: `PerSurfaceLayoutState` Struct

**What:** Retained state wrapper holding the persistent `TaffyTree`, the `_mesh_key → TaffyNodeId` map, last available space, and validity flag.

**When to use:** Instantiated once per surface on `FrontendSurfaceComponent`. Reset to `valid=false` on theme/locale/source-reload events; the next paint triggers a fresh-build that re-populates the tree.

```rust
// In crates/core/ui/elements/src/layout.rs
use std::collections::HashMap;
use taffy::{TaffyTree, prelude::NodeId as TaffyNodeId};
use crate::tree::NodeId;

pub struct PerSurfaceLayoutState {
    pub tree: TaffyTree<NodeId>,
    /// Maps _mesh_key attribute String → TaffyNodeId for stable retained identity.
    pub node_map: HashMap<String, TaffyNodeId>,
    /// (width, height) used in last compute_layout call.
    pub last_available: (f32, f32),
    /// False after reset (theme/locale change). Forces fresh-build for one frame.
    pub valid: bool,
    // Optional: text_nodes map for leaf measure data (for compute_layout_with_measure)
    pub text_nodes: HashMap<NodeId, TextMeasureData>,
}

impl Default for PerSurfaceLayoutState {
    fn default() -> Self {
        Self {
            tree: TaffyTree::new(),
            node_map: HashMap::new(),
            last_available: (0.0, 0.0),
            valid: false,
            text_nodes: HashMap::new(),
        }
    }
}
```

### Pattern 2: VISUAL_REPAINT Path (No Geometry Recompute)

**What:** When only paint/style-visual properties changed (no LAYOUT bit in `dirty_types`), call `set_style` on changed nodes but skip `compute_layout` entirely. Existing geometry from prior frames remains valid.

**Critical Taffy nuance:** `set_style` internally calls `mark_dirty`. This means a `compute_layout` call after `set_style` would recompute geometry, which is correct for the LAYOUT path. For the VISUAL_REPAINT-only path, skip `compute_layout` altogether — call `set_style` to keep Taffy's internal style state consistent, but do not follow with `compute_layout`. [VERIFIED: docs.rs + Taffy source code]

```rust
// VISUAL_REPAINT only — no LAYOUT bit
// Walk new widget tree, for each node whose _mesh_key is in node_map:
for (mesh_key, taffy_id) in &state.node_map {
    if let Some(widget_node) = find_by_mesh_key(root, mesh_key) {
        let style = taffy_style_for_node(widget_node, &mut report);
        let _ = state.tree.set_style(*taffy_id, style); // marks dirty internally
    }
}
// Do NOT call compute_layout — geometry is unchanged
// Do NOT call write_taffy_layout — WidgetNode.layout values are still valid
// NOTE: because set_style marks dirty, the cached layout is invalidated;
// if compute_layout IS called later (e.g. available-space change), it recomputes correctly
```

### Pattern 3: LAYOUT-Dirty Path (Geometry Invalidation)

**What:** When LAYOUT bit is set but SCRIPT/TEXT are not (no structural change), call `mark_dirty` on affected nodes and then `compute_layout`.

```rust
// LAYOUT dirty (non-structural)
// mark_dirty propagates to ancestors automatically — only call on leaf of change
for (mesh_key, taffy_id) in &state.node_map {
    if let Some(widget_node) = find_by_mesh_key(root, mesh_key) {
        // Only mark nodes whose layout-affecting style actually changed
        let _ = state.tree.mark_dirty(*taffy_id);
    }
}
let available_space = TaffySize {
    width: TaffyAvailableSpace::Definite(available_width),
    height: TaffyAvailableSpace::Definite(available_height),
};
state.tree.compute_layout_with_measure(root_taffy_id, available_space, measure_fn)?;
write_taffy_layout(root, &state.tree, &mesh_key_to_taffy_id_map);
```

### Pattern 4: TREE_REBUILD Path (Structural Diff)

**What:** When SCRIPT or TEXT bits are set, walk the new widget tree and reconcile it against the retained `_mesh_key → TaffyNodeId` map.

**Algorithm:**
1. Collect all `_mesh_key` values in the new tree (depth-first walk)
2. For each node in new tree:
   - If `_mesh_key` found in `node_map` → update style via `set_style` (marks dirty)
   - If not found → create new TaffyNode, insert into `node_map`
3. For each `_mesh_key` in `node_map` NOT in new tree → `remove_taffy_subtree`
4. Reconcile children order for each parent: use `set_children` with the new ordered slice
5. Call `compute_layout_with_measure`

**Nodes without `_mesh_key`:** Every `WidgetNode` has an `id: NodeId` (u64, atomic counter, ephemeral per run). The `_mesh_key` attribute is set by the template compiler to stable path-based strings like `"root/0"`, `"root/1/button"`. Nodes without `_mesh_key` cannot be stably tracked — treat them as always-new (remove old, add new).

### Pattern 5: `remove_taffy_subtree` Post-Order Walk

**What:** Recursively remove all descendants before removing a parent, because `TaffyTree::remove()` only detaches the node and orphans (does not delete) its children.

**Verified behavior from Taffy source:** `remove()` sets `parents[child] = None` for each child of the removed node, then removes the node from `nodes`, `parents`, and `children` slotmaps. Children remain allocated in the slotmap as orphans, consuming memory and leaking `TaffyNodeId` handles. [VERIFIED: Taffy source via WebFetch]

```rust
/// Remove a node and all its descendants from the TaffyTree, post-order.
/// Must be called before removing a parent to avoid orphaned TaffyNodeIds.
pub fn remove_taffy_subtree(
    tree: &mut TaffyTree<NodeId>,
    node_id: TaffyNodeId,
) -> Result<(), taffy::TaffyError> {
    // Collect children first (before removal invalidates the handle)
    let children = tree.children(node_id).unwrap_or_default();
    // Post-order: remove children before parent
    for child in children {
        remove_taffy_subtree(tree, child)?;
    }
    tree.remove(node_id)?;
    Ok(())
}
```

### Anti-Patterns to Avoid

- **Using `NodeId` (u64) as the retained map key:** `NodeId` is generated by atomic increment on each `WidgetNode::new()` call. After a `TREE_REBUILD`, all nodes are newly constructed with new IDs. Using `NodeId` as the key would cause a complete map miss every rebuild, defeating retention. Use `_mesh_key` (the stable template-path string) instead.
- **Removing parent before children in `TREE_REBUILD`:** Taffy's `remove()` orphans children — they persist in the slotmap as unreachable nodes. Always walk post-order.
- **Calling `compute_layout` after style-only `set_style` in VISUAL_REPAINT path:** Unnecessary geometry recomputation; defeats the purpose of the VISUAL_REPAINT optimization. Skip `compute_layout` when no LAYOUT bit is set.
- **Not resetting `valid=false` on `theme_changed`/`locale_changed`:** Stale Taffy styles (from previous theme) would persist in the retained tree for one frame, producing geometry based on wrong token values.
- **Putting `PerSurfaceLayoutState` in `mesh-core-shell` as an opaque blob:** The struct contains `TaffyTree` and interacts with `taffy_style_for_node` — both are in `mesh-core-elements`. Defining `PerSurfaceLayoutState` in `layout.rs` keeps all Taffy coupling in one file.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Layout dirty propagation to ancestors | Custom upward-walk marking parents | `taffy::TaffyTree::mark_dirty()` | Already propagates to root automatically; no ancestor walk needed |
| Flexbox geometry calculation | Custom flex algorithm | `taffy::TaffyTree::compute_layout_with_measure()` | Taffy handles all flex edge cases: wrapping, basis, RTL, absolute positioning |
| Available-space struct construction | Custom type | `taffy::geometry::Size<taffy::prelude::AvailableSpace>` | Already used in existing `compute_taffy_layout_with_cache` |
| Slotmap iteration over TaffyTree nodes | Custom storage | `TaffyTree::children(parent)` | Returns `Vec<NodeId>` for structural diff |

**Key insight:** The entire point of retaining the `TaffyTree` is to let Taffy's internal dirty-cache skip recomputation for clean subtrees. Don't reimplement Taffy's cache logic externally.

## Common Pitfalls

### Pitfall 1: `set_style` Always Marks Dirty — VISUAL_REPAINT Must Skip `compute_layout`

**What goes wrong:** Developer reads CONTEXT.md's description of VISUAL_REPAINT path as "call `set_style`, then optionally call `compute_layout` — it'll be cheap." In practice, `set_style` marks the node (and all ancestors) dirty, so `compute_layout` will recompute the full path from the dirty node to root. If called on 20 icon nodes in a busy panel, that's 20 ancestor chains invalidated and recomputed.

**Why it happens:** The Taffy API does not distinguish "paint-only" style changes from geometry-affecting ones. `set_style` unconditionally calls `mark_dirty`.

**How to avoid:** For VISUAL_REPAINT-only frames, call `set_style` (to keep Taffy's style state consistent with the widget tree) but do NOT call `compute_layout`. Read layout results from the already-valid `node.layout` fields populated in the prior frame.

**Warning signs:** Frame times increasing on hover/animation frames where geometry doesn't change; `ProfilingStage::Layout` timing in profiling shows unexpected non-zero on VISUAL_REPAINT paths.

### Pitfall 2: Orphaned `TaffyNodeId`s After Parent Removal

**What goes wrong:** A `TREE_REBUILD` removes a parent node that has children. `TaffyTree::remove(parent)` detaches children but does not free them. Over many frames with repeated structural changes, orphaned nodes accumulate in the slotmap, growing memory unboundedly.

**Why it happens:** Taffy's `remove()` implementation only removes the parent from `nodes`/`children`/`parents` slotmaps, and sets `parents[child] = None` for each child. The children's own slotmap entries remain.

**How to avoid:** Always use `remove_taffy_subtree(tree, taffy_id)` instead of `tree.remove(taffy_id)` directly. The helper post-order walks all descendants before removing the parent.

**Warning signs:** `TaffyTree::total_node_count()` growing monotonically across frames despite the widget tree being stable in size.

### Pitfall 3: Nodes Without `_mesh_key` in Structural Diff

**What goes wrong:** Not all `WidgetNode`s have a `_mesh_key` attribute. Nodes inserted without a key (e.g., synthetic nodes from runtime primitives) cannot be stably tracked. If the structural diff assumes every node has a key, it will silently skip unkeyed nodes.

**Why it happens:** `_mesh_key` is set by the template compiler for component-authored nodes. Internally-synthesized nodes (spacers, runtime-injected wrappers) may lack it.

**How to avoid:** During structural diff, treat nodes without `_mesh_key` as position-keyed (index in parent's children list). If the tag+index matches, reuse the existing `TaffyNodeId`; if not, remove and recreate. This is less efficient than key-based diff but correct.

**Warning signs:** `TREE_REBUILD` frames producing incorrect geometry for panels that use runtime-injected wrapper elements.

### Pitfall 4: `last_available` Must Use Same Float Representation

**What goes wrong:** The check `if (w, h) != state.last_available` uses float equality. On some paths, `content_width` may be computed as `1280.0` via integer arithmetic and on other paths as `1279.9999` via scaling. The comparison triggers an unnecessary full `compute_layout` re-run.

**Why it happens:** Surface size comes from `u32` width/height but is used as `f32` for Taffy. `as f32` conversion is exact for small integers but integer arithmetic via floats can introduce sub-pixel differences.

**How to avoid:** Cast `u32` to `f32` once at the call site and store the result. Compare with `==` (bit-exact equality is fine for values derived from the same u32→f32 cast path). Avoid mixing `width as f32` computed at different stack frames.

**Warning signs:** Profiling shows `compute_layout` running on frames where only hover state changed (no size change expected).

### Pitfall 5: `text_nodes` Map Must be Rebuilt for TREE_REBUILD Frames

**What goes wrong:** The `text_nodes: HashMap<NodeId, TextMeasureData>` (used by the `compute_layout_with_measure` closure) is keyed by `NodeId` (ephemeral u64). On `TREE_REBUILD`, new `WidgetNode`s have new `NodeId`s. If `text_nodes` is retained from the previous frame, the measurer closure cannot find the text data for the new `NodeId`s and returns zero sizes.

**Why it happens:** `NodeId` is generated by `next_node_id()` (atomic u64) on each `WidgetNode::new()` call. Every `build_tree` call creates new nodes with new IDs.

**How to avoid:** Rebuild `text_nodes` from the current widget tree on every frame that calls `compute_layout_with_measure`. It is cheap (one walk, no allocation beyond the map). The `IntrinsicLayoutCache` keyed on `TextMeasureKey` (content + font + size) remains valid across frames and handles the actual deduplication.

## Code Examples

### Existing layout entry point to extend

```rust
// Source: crates/core/ui/elements/src/layout.rs, line 200
fn compute_taffy_layout_with_cache(
    root: &mut WidgetNode,
    available_width: f32,
    available_height: f32,
    intrinsic_cache: &mut IntrinsicLayoutCache,
    measurer: Option<&dyn TextMeasurer>,
) {
    let mut report = TaffyLayoutReport::default();
    let mut tree = TaffyTree::<NodeId>::new();     // ← THIS is the fresh-build to replace
    let mut node_map = HashMap::new();
    let mut text_nodes = HashMap::new();
    // ...
}
```

The new incremental entry point adds `layout_state: &mut PerSurfaceLayoutState` and `dirty_types: ComponentDirtyFlags` parameters and routes to the appropriate sub-path.

### Existing call site to change

```rust
// Source: crates/core/shell/src/shell/component/rendering.rs, line 338
LayoutEngine::compute_with_intrinsic_cache_and_measurer(
    tree,
    width as f32,
    height as f32,
    &mut self.intrinsic_layout_cache,
    Some(&measurer),
);
```

This becomes a call to the new incremental entry point, passing `&mut self.layout_state` and `dirty_types`.

### Reset in theme/locale/source-reload

```rust
// theme_changed in shell_component.rs (line 696)
// After existing resets, add:
self.layout_state = PerSurfaceLayoutState::default(); // valid=false, tree cleared

// locale_changed (line 724) — same pattern
self.layout_state = PerSurfaceLayoutState::default();

// reload_source (line 805) — same pattern
self.layout_state = PerSurfaceLayoutState::default();
```

### Parity test scaffold

```rust
// In layout.rs #[cfg(test)] block
#[test]
fn retained_layout_style_only_parity() {
    // Build a small tree, run fresh-build layout, capture LayoutRects.
    // Then call incremental path with VISUAL_REPAINT dirty_types, no style change.
    // Assert LayoutRects are identical.
}

#[test]
fn retained_layout_add_node_parity() {
    // Build tree, run incremental. Add a child node (TREE_REBUILD).
    // Run fresh-build on same updated tree. Compare LayoutRects.
}

// ... etc for remove, reorder, layout-dirty
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Fresh `TaffyTree` every frame | Retained `TaffyTree` mutated per dirty-bit | Phase 104 | Eliminates O(n) tree reconstruction; dirty subtrees only pay geometry cost |
| `HashMap<NodeId, TaffyNodeId>` (ephemeral key) | `HashMap<String, TaffyNodeId>` keyed by `_mesh_key` | Phase 104 | Survives TREE_REBUILD frames; stable across script re-evaluation |

**Deprecated/outdated patterns after this phase:**
- `compute_taffy_layout_with_cache` building `let mut tree = TaffyTree::new()` on every call: removed or unreachable for the per-surface hot path (may remain as a fallback for the first `valid=false` frame).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Taffy 0.10.1's `set_style` always calls `mark_dirty` internally (not conditionally on style change) | Common Pitfalls #1, Code Examples | If wrong, VISUAL_REPAINT path could safely call compute_layout cheaply; actual impact is low but understanding is incorrect |
| A2 | Nodes without `_mesh_key` exist (runtime-synthesized wrappers) | Pitfall #3 | If every node always has `_mesh_key`, the fallback to position-keyed diff is unnecessary complexity |
| A3 | `text_nodes` map keyed by `NodeId` must be rebuilt each frame even in retained path | Pitfall #5, Code Examples | If `NodeId` were stable across frames (it is not — see `next_node_id()` atomic), text_nodes could be retained |

A1 is [VERIFIED: Taffy source via WebFetch]. A2 and A3 are [ASSUMED] based on reading the codebase.

## Open Questions

1. **Should `PerSurfaceLayoutState` live in `mesh-core-elements` or `mesh-core-shell`?**
   - What we know: `TaffyTree` and all Taffy types are in `mesh-core-elements`; `FrontendSurfaceComponent` is in `mesh-core-shell` (depends on elements)
   - What's unclear: Whether the planner wants all Taffy coupling in elements (cleaner) or prefers the struct co-located with its consumer
   - Recommendation: Define `PerSurfaceLayoutState` in `layout.rs` (`mesh-core-elements`), export it pub. Shell imports it as a field type — no circular dependency. This keeps all Taffy imports in one file.

2. **How to identify "affected nodes" for set_style on VISUAL_REPAINT path?**
   - What we know: The `restyle_retained_tree` path in `rendering.rs` already knows which nodes changed style (via `collect_interaction_changed_keys`). The dirty_types from `take_dirty_for_paint()` carry flag-level granularity but not node-level lists.
   - What's unclear: Whether to call `set_style` on ALL retained nodes (safe, slightly wasteful) or only on nodes whose computed style changed
   - Recommendation: For initial implementation, call `set_style` on all nodes in a full walk (equivalent to fresh-build cost for Taffy style ingestion, but skips compute_layout). Optimize to diff-driven in a later phase.

## Environment Availability

Step 2.6 SKIPPED — this phase is purely code changes within an existing Rust workspace. No external tools, services, CLIs, or databases are required beyond `cargo`.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[cfg(test)]` + `cargo test` |
| Config file | none — workspace uses `cargo test` directly |
| Quick run command | `cargo test --package mesh-core-elements -- layout` |
| Full suite command | `cargo test --package mesh-core-elements && cargo test --package mesh-core-shell` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| LAYOUT-01 | Retained tree survives across frames; no fresh-build when valid=true | unit | `cargo test --package mesh-core-elements -- retained_layout` | ❌ Wave 0 |
| LAYOUT-02 | STYLE-only → set_style; LAYOUT → mark_dirty + compute_layout | unit | `cargo test --package mesh-core-elements -- retained_layout` | ❌ Wave 0 |
| LAYOUT-03 | `_mesh_key` identity survives TREE_REBUILD | unit | `cargo test --package mesh-core-elements -- retained_layout` | ❌ Wave 0 |
| LAYOUT-04 | `remove_taffy_subtree` removes all descendants post-order | unit | `cargo test --package mesh-core-elements -- remove_taffy_subtree` | ❌ Wave 0 |
| LAYOUT-05 | Retained output == fresh-build output for all 5 dirty scenarios | unit | `cargo test --package mesh-core-elements -- retained_layout_parity` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --package mesh-core-elements -- layout`
- **Per wave merge:** `cargo test --package mesh-core-elements && cargo test --package mesh-core-shell`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/core/ui/elements/src/layout.rs` — add `#[cfg(test)]` block with 5 LAYOUT-05 parity tests and `remove_taffy_subtree` test
- [ ] No new test files needed — tests live inline in `layout.rs` per project convention

*(Existing test infrastructure covers all other requirements — 23 layout tests already pass as of research date)*

## Security Domain

This phase is internal layout-engine retention. No external input is processed, no network access, no user-facing authentication or authorization paths changed. ASVS categories do not apply. `security_enforcement` is not explicitly set to false in config, but this phase has no attack surface — all data flows from the retained widget tree (already validated by the component compiler) to Taffy geometry structures.

## Sources

### Primary (HIGH confidence)
- `https://docs.rs/taffy/0.10.1/taffy/struct.TaffyTree.html` — Full TaffyTree API surface: `set_style`, `mark_dirty`, `remove`, `add_child`, `set_children`, `compute_layout_with_measure`, `children`, `layout` [VERIFIED]
- `https://github.com/DioxusLabs/taffy/blob/main/src/tree/taffy_tree.rs` — Taffy source confirming `set_style` calls `mark_dirty` unconditionally; `remove()` orphans children without recursion [VERIFIED]
- `/home/kolby/projects/mesh/crates/core/ui/elements/src/layout.rs` — Current `compute_taffy_layout_with_cache` (line 200), `build_taffy_tree`, `write_taffy_layout`, existing test patterns [VERIFIED: read directly]
- `/home/kolby/projects/mesh/crates/core/shell/src/shell/component.rs` — `ComponentDirtyFlags` bit definitions (lines 67–141), `FrontendSurfaceComponent` struct fields, `take_dirty_for_paint()` [VERIFIED: read directly]
- `/home/kolby/projects/mesh/crates/core/shell/src/shell/component/rendering.rs` — `finalize_tree()` call site for `LayoutEngine::compute_with_intrinsic_cache_and_measurer` (line 338), `reused_retained_layout` guard (line 302) [VERIFIED: read directly]
- `/home/kolby/projects/mesh/crates/core/shell/src/shell/component/shell_component.rs` — `theme_changed`, `locale_changed`, `reload_source` reset sites; existing retained field reset pattern [VERIFIED: read directly]

### Secondary (MEDIUM confidence)
- `/home/kolby/projects/mesh/crates/core/shell/src/shell/component/runtime_tree.rs` — `RetainedWidgetTree` pattern as model for `PerSurfaceLayoutState` design [VERIFIED: read directly]

### Tertiary (LOW confidence)
- None.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — Taffy 0.10.1 already in workspace; no new deps
- Architecture: HIGH — read existing code directly; verified Taffy API from docs.rs + source
- Taffy `set_style` behavior: HIGH — verified from source code
- Taffy `remove()` orphan behavior: HIGH — verified from source code
- `_mesh_key` stability: HIGH — confirmed throughout codebase grep
- Pitfalls: HIGH for #1/#2 (verified from source); MEDIUM for #3/#5 (inferred from codebase patterns)

**Research date:** 2026-06-18
**Valid until:** 2026-08-18 (taffy 0.10.x is stable; API unlikely to change within 60 days)
