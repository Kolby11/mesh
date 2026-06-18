# Feature Research: MESH v1.21 Retained Layout & Display List

**Domain:** Retained-mode UI rendering — incremental layout tree mutation, rope-style command store, per-stage budget profiling
**Researched:** 2026-06-18
**Confidence:** HIGH for tree mutation semantics (Taffy API verified via Context7); HIGH for display list span contract (derived from current codebase + Mozilla Firefox retained display list published architecture); MEDIUM for rope/persistent-vector store design (pattern is clear from Flutter/WebRender, specific tradeoffs are judgment calls)

---

## Context: What Already Exists

The existing MESH retained pipeline (v1.4–v1.5) already has:
- Stable `_mesh_key` node IDs on `WidgetNode`
- `RetainedDisplayList` with `HashMap<NodeId, RetainedPaintSubtree>` keyed subtree caching
- `RetainedPaintSubtree` storing `Arc<[DisplayPaintCommand]>` and `Arc<[DisplayPaintCommandKind]>` per node
- `RetainedCommandSpan` indexing into a flat `paint_commands: Arc<[DisplayPaintCommand]>` array for damage-range queries
- `PaintSubtreeBuilder` that copies child commands into parent command vectors on each update
- Typed dirty bits: `STYLE_ONLY / LAYOUT / PAINT / TEXT / TREE_REBUILD` per node (v1.18)
- `ProfilingStage` enum with `Layout`, `RetainedDisplayListUpdate`, `PaintTraversal`, `TextShaping`, `Paint` stages

The current `LayoutEngine::compute_taffy_layout_with_cache` **creates a fresh `TaffyTree` on every layout pass**, builds the full node-to-TaffyNodeId map from scratch, runs `compute_layout_with_measure`, then writes results back to `WidgetNode.layout`. This is the specific regression being fixed.

The current `PaintSubtreeBuilder::append_child` **copies child `Arc<[DisplayPaintCommand]>` slices into the parent's command vec** on each dirty update, even for clean children. This is the copy behavior the rope store replaces.

---

## Feature Landscape

### Table Stakes (Required for This Milestone to Be Correct)

Features that, if missing, leave the retained pipeline with correctness holes or make the milestone's goals unachievable.

| Feature | Why Required | Complexity | Dependency |
|---------|--------------|------------|------------|
| Retain `TaffyTree` and `NodeId` map per surface across layout passes | Without this, the entire TaffyTree rebuild happens every frame; the milestone goal is impossible | MEDIUM | Needs `TaffyTree` stored on `FrontendSurfaceComponent` or equivalent render state, not local to `compute_taffy_layout_with_cache` |
| In-place style mutation via `TaffyTree::set_style` for STYLE/PAINT-dirty nodes | Style changes that do not affect structural layout must update in-place; new tree rebuild on style-only changes defeats the purpose | LOW | Requires the `NodeId` map to be stable so dirty-bit annotations can look up the right `TaffyNodeId` |
| Structural mutation via `add_child`/`remove_child`/`remove` for TREE_REBUILD nodes | Structural changes (child added/removed) must mutate in-place, not trigger full tree rebuild | MEDIUM | Requires reconciliation pass comparing WidgetNode children vs TaffyTree children and issuing minimum edit operations |
| `TaffyTree::mark_dirty` propagation for LAYOUT-dirty subtrees | Layout changes on an interior node must invalidate its Taffy ancestors so layout re-runs only the affected subtree | LOW | Taffy already propagates `mark_dirty` to ancestors automatically; MESH just needs to call it at the right nodes |
| `TaffyTree::remove` cleanup for nodes removed from WidgetNode tree | Orphaned Taffy nodes must be freed; otherwise the TaffyTree grows unboundedly across TREE_REBUILD cycles | MEDIUM | Needs a diff between the previous `NodeId→TaffyNodeId` map and the new WidgetNode tree to identify removed nodes |
| Span ownership contract for retained command store | The display list must define which entity owns a command span, when that span is valid to reuse, and when it must be rebuilt | LOW | Mostly documentation and type-level enforcement of existing semantics |
| Arc-based span sharing (no copy for clean subtrees) | The core value of a rope-style store: clean child spans are referenced, not copied into parent command vectors | HIGH | Requires changing `PaintSubtreeBuilder::append_child` to store `Arc` references to child command slices rather than copying them |
| Damage rect computation from referenced spans, not flattened array positions | When spans are `Arc<[DisplayPaintCommand]>` references rather than index ranges, damage queries must still work correctly | MEDIUM | `RetainedCommandSpan` currently indexes into a flat array by position; rope-style sharing changes those positions each frame for unmodified children |

### Differentiators (Valuable but Not Required for Correctness)

Features that improve performance or observability beyond what the basic milestone requires.

| Feature | Value Proposition | Complexity | Notes |
|---------|------------------|------------|-------|
| Per-stage wall-clock budget pins for canonical workloads | Gives future optimization work a concrete target: "Layout must complete in ≤2ms on navigation-bar hover" | MEDIUM | Builds on existing `ProfilingStage` enum; adds budget thresholds per stage per workload |
| Separate profiling stages for TaffyTree retention vs structural reconciliation | Distinguishes "layout cache hit" from "in-place style update" from "structural edit"; current `ProfilingStage::Layout` conflates all three | LOW | Add `TaffyRetainedLayout` and `TaffyStructuralReconcile` stages to the enum |
| Span reuse rate as a debug metric | Exposes what fraction of command spans were reused vs rebuilt per frame; helps diagnose whether dirty-bit routing is working | LOW | Add `span_reuse_count` and `span_rebuild_count` to `DisplayListMetrics` — skeleton already exists |
| Clean-subtree detection before reconciliation | Skip the Taffy child-diff for subtrees with no dirty descendants; avoids walking clean branches of a large tree | MEDIUM | Requires propagating "has any dirty descendant" up the WidgetNode tree during dirty-bit marking |
| TaffyTree node count diagnostic | Report `tree.total_node_count()` in debug state to verify the tree is not leaking nodes between TREE_REBUILD cycles | LOW | One-line addition to `DebugSnapshot` |
| Intrinsic text measurement cache surviving layout tree retention | The existing `IntrinsicLayoutCache` is currently local to each layout call; if the TaffyTree is retained, the cache should be too | LOW | Move `IntrinsicLayoutCache` to the same storage as the retained TaffyTree |

### Anti-Features (Explicitly Out of Scope)

Features that are commonly reached for but should not be built in this milestone.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| Full browser-style DOM change-set diffing | Computes a minimal edit script (Myers diff) between the previous and new virtual tree; high implementation overhead for a non-browser toolkit where structural changes are rare | MESH uses stable `_mesh_key` IDs; a simple keyed reconciliation (match by ID, detect adds/removes) is sufficient |
| Persistent immutable tree nodes (functional data structure) | Persistent vectors / structural sharing at the element level add memory overhead and complicate mutable style writeback | MESH `WidgetNode` tree is already mutable and stable-ID; retain the Taffy tree alongside it without changing WidgetNode ownership |
| Per-pixel dirty tracking | Tracks which pixels changed at the pixel level; used by some GPU compositing engines but adds >10x memory overhead vs rect-based damage | The existing `DamageRect` + per-node paint dirty bits are the right granularity for a software renderer |
| Command-level deduplication hashing | Hashing every `DisplayPaintCommand` to detect semantic equivalence between frames; already partially done via `DisplayListEntry::signature` and `primitive_signature()` | Extend the existing signature approach rather than adding a new hashing layer |
| Separate rope crate dependency | Using `ropey` or `im` for the command store; these are text-oriented or general-purpose, not tuned for the display-list access pattern | The `Arc<[DisplayPaintCommand]>` pattern already in `RetainedPaintSubtree` is the right approach — it provides structural sharing without a new dependency |
| Skia/Vello rendering integration | GPU backend is explicitly deferred per `PROJECT.md` key decisions | Keep the software painter path; profiling results from this milestone inform the GPU migration |
| Full layout cache invalidation on any style property change | Forces relayout on opacity, color, shadow, or other paint-only properties that have no effect on box geometry | Use dirty-bit type to gate: `PAINT`-only dirty nodes call `set_style` but do NOT call `mark_dirty` for geometry (Taffy will not relayout them) |
| Invalidating the retained TaffyTree when surface size changes | Surface resize is rare; the TaffyTree can be retained with a forced root-level `mark_dirty` and a single re-layout | Only fully rebuild the TaffyTree if the root node ID changes (indicating a component reload) |

---

## Feature Dependencies

```
Retain TaffyTree per surface
    └──enables──> In-place style mutation (set_style)
    └──enables──> Structural mutation (add_child/remove/mark_dirty)
    └──enables──> Intrinsic text cache retention

In-place style mutation
    └──requires──> Stable NodeId map (WidgetNode.id → TaffyNodeId)
    └──requires──> MESH dirty-bit type to gate set_style vs mark_dirty correctly

Structural mutation
    └──requires──> NodeId map diff (old map vs new WidgetNode tree)
    └──requires──> TaffyTree::remove for orphaned nodes

Arc-based span sharing (no copy for clean subtrees)
    └──requires──> Span ownership contract documented
    └──requires──> Damage queries work on referenced spans (not flat array positions)

Per-stage budget pins
    └──requires──> Retained layout implemented (so Layout stage reflects cache-hit cost)
    └──requires──> Rope-style display list implemented (so RetainedDisplayListUpdate reflects span-reuse cost)
```

### Dependency Notes

- **Retaining the TaffyTree requires retaining the NodeId map**: The `HashMap<NodeId, TaffyNodeId>` currently created per layout call must live alongside the `TaffyTree`. Both need to be stored on the surface render state.
- **Structural mutation requires a diff of the NodeId map**: After building the new WidgetNode tree, nodes present in the old `NodeId→TaffyNodeId` map but absent from the new tree must be cleaned up via `TaffyTree::remove`. Nodes newly present in the WidgetNode tree must be created via `new_leaf_with_context` or `new_with_children`.
- **Arc-based span sharing conflicts with the current flat array indexing**: `RetainedCommandSpan` stores `start/end` byte offsets into the flat `paint_commands: Arc<[DisplayPaintCommand]>` array. If child spans are referenced rather than copied, those offsets are no longer stable across frames. Either: (a) switch to a tree of `Arc` spans without a flat array, or (b) rebuild the flat array from the tree of spans as a final assembly step (maintaining the span index for damage queries). Option (b) preserves the existing damage-query contract with no caller changes.
- **Budget pins depend on the stages they measure existing first**: Add `TaffyRetainedLayout` and `TaffyStructuralReconcile` sub-stages before pinning budgets.

---

## MVP Definition

### This Milestone (v1.21 — Launch With)

The minimum needed to deliver the milestone goals from `PROJECT.md`.

- [ ] Retained `TaffyTree` and `NodeId` map stored per surface, surviving across layout passes — without this the milestone goal is not met
- [ ] In-place style mutation (`set_style`) for style/paint-only dirty nodes — avoids rebuilding Taffy for common hover/focus/value changes
- [ ] Structural reconciliation (add/remove/mark_dirty) for TREE_REBUILD-dirty nodes — handles list items appearing/disappearing
- [ ] Orphaned-node cleanup via `TaffyTree::remove` — prevents memory growth
- [ ] Arc-based span sharing: `PaintSubtreeBuilder::append_child` references child `Arc<[DisplayPaintCommand]>` instead of copying — the rope-style improvement
- [ ] Final flat-array assembly from the span tree — preserves damage query contract for `select_paint_commands`
- [ ] Per-stage budget profiling for canonical workloads (navigation-bar hover, audio-popover slider drag, backend update)

### After Validation (v1.x)

- [ ] Clean-subtree early-exit during structural reconciliation — trigger: profiling shows structural reconciliation dominating on large trees
- [ ] `TaffyRetainedLayout` / `TaffyStructuralReconcile` as distinct `ProfilingStage` variants — trigger: debug tooling needs to distinguish cache-hit from structural-edit cost
- [ ] Span reuse rate as a `DisplayListMetrics` field — trigger: investigation of frame-over-frame display list efficiency

### Future Consideration (v2+)

- [ ] GPU-backed paint commands using Skia or Vello — explicitly deferred per key decisions
- [ ] Parallel layout across independent subtrees — no evidence this is needed at current tree sizes

---

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Retain TaffyTree per surface | HIGH — eliminates per-frame full rebuild overhead | MEDIUM — requires restructuring layout engine ownership | P1 |
| In-place style mutation for paint-only dirty nodes | HIGH — hover/focus is the most common interaction path | LOW — `set_style` exists; routing change only | P1 |
| Structural reconciliation with cleanup | HIGH — prevents memory growth and enables list updates | MEDIUM — needs add/remove diff logic | P1 |
| Arc span sharing (no-copy clean subtrees) | HIGH — eliminates the primary display list copy overhead | HIGH — requires changing `append_child` semantics and flat-array assembly | P1 |
| Per-stage budget pins | MEDIUM — developer tooling only; no end-user impact | MEDIUM — workload design, threshold selection | P2 |
| `TaffyRetainedLayout` profiling sub-stage | LOW — diagnostic granularity | LOW — enum addition | P2 |
| Span reuse rate metric | LOW — debug metric | LOW — counter addition | P3 |
| Intrinsic text cache retention | MEDIUM — avoids remeasuring unchanged text nodes on retained layout passes | LOW — move existing `IntrinsicLayoutCache` to surface state | P2 |

---

## How Reference Toolkits Handle These Problems

### TaffyTree Retention (Taffy API)

Taffy is designed for retained use. `TaffyTree<C>` carries `NodeId`-stable layout caches across frames. The correct call sequence after initial construction is:

- Style change only: `tree.set_style(taffy_node, new_style)` — calls `mark_dirty` internally, does not rebuild tree structure
- Child added: `tree.add_child(parent, new_leaf)` — calls `mark_dirty` on parent automatically
- Child removed: `tree.remove_child(parent, child)` then optionally `tree.remove(child)` if the node is gone from the widget tree
- Leaf with changed measurement context (text content changed): `tree.mark_dirty(leaf)` explicitly
- Layout recompute: `tree.compute_layout_with_measure(root, available_space, measure_fn)` — only recomputes dirty subtrees; clean subtrees are cache hits

Confidence: HIGH — verified via Context7 Taffy documentation.

### Flutter Layer / Display List Model

Flutter's `RenderObject` tree maintains a `Layer` tree alongside it. Clean subtrees retain their `Layer` references across frames. The `Layer` object holds an `EngineLayer` reference for GPU-side retention. `markNeedsPaint()` propagates up to the nearest `RepaintBoundary` ancestor, so only the dirty layer region is repainted. The `SceneBuilder` API accepts an `oldLayer` argument to enable GPU-side layer reuse.

The MESH equivalent is: `RetainedPaintSubtree` acts like a Flutter `Layer`. A clean subtree's `Arc<[DisplayPaintCommand]>` is the MESH equivalent of a retained `EngineLayer`. The `_mesh_key` identity is the MESH equivalent of Flutter's `GlobalKey` / element identity.

### Qt Quick Scene Graph

Qt Quick's `QSGNode` tree is retained. `QQuickItem::updatePaintNode()` is called only on items that have `QQuickItem::update()` pending. Most `QSGNode` mutations (geometry, material) automatically call `markDirty()` with the appropriate flags (`DirtyGeometry`, `DirtyMaterial`, `DirtyNodeAdded`, `DirtyNodeRemoved`). The renderer traverses only dirty branches.

The `DirtyNodeAdded` / `DirtyNodeRemoved` flags on `QSGNode` correspond to MESH's `TREE_REBUILD` dirty bit. Qt's approach is to mark nodes rather than diff a shadow tree — MESH already follows this with its per-node dirty bits.

### Mozilla Firefox Retained Display Lists

Firefox's Gecko retained display list works by building the display list only for changed subtrees and merging new segments into the retained list at the appropriate positions (identified by stable frame IDs similar to `_mesh_key`). The merge step replaces only the dirty segments, leaving clean segments in place. This is architecturally identical to the MESH `build_paint_subtree` reuse logic, with the difference that Firefox uses tree-position-indexed segments while MESH uses `NodeId`-keyed `RetainedPaintSubtree` entries.

The Firefox approach does not use a rope data structure in the strict functional-persistence sense — it uses a mutable display list with in-place segment replacement, which is closer to the existing MESH `HashMap<NodeId, RetainedPaintSubtree>` approach. The "rope-style" framing in the MESH milestone context means sharing `Arc` slices between subtrees, not persistent vector semantics.

---

## Dirty-Flag Integration for Each Feature

| Feature | Dirty Bit that Triggers It | What It Must NOT Do |
|---------|---------------------------|---------------------|
| In-place `set_style` on Taffy node | `PAINT` or `STYLE_ONLY` | Must NOT call `mark_dirty` for pure paint changes (color, opacity, shadow) that have no geometry effect |
| In-place `set_style` + `mark_dirty` | `LAYOUT` | Must NOT rebuild Taffy tree structure; only updates style and propagates dirty up to ancestors |
| Structural reconciliation (add/remove) | `TREE_REBUILD` | Must NOT leave orphaned `TaffyNodeId` entries in the map |
| Arc span reuse | No dirty bit — clean subtree has no dirty bit set | Must NOT reuse a span if any node in the subtree has `PAINT`, `LAYOUT`, or `TEXT` dirty bit |
| Full TaffyTree rebuild fallback | Root `TREE_REBUILD` with ID change (component reload) | Must still clean up the old `NodeId` map before discarding it |

---

## Profiling Granularity Required

The milestone asks for "per-stage performance budget profiling for canonical shell workloads." The existing `ProfilingStage` enum already covers the necessary stages. What is missing is:

1. **Budget thresholds per workload**: concrete numbers that define what "good" means for each canonical scenario. These numbers should be derived from measurement, not assumed. The milestone work is: measure current per-stage costs on canonical workloads, record them as baselines, then re-measure after retention improvements and confirm improvement.

2. **Disambiguation within the `Layout` stage**: Currently `ProfilingStage::Layout` covers both TaffyTree construction and `compute_layout`. With retention, the construction cost disappears for cache-hit frames, making the two paths unequal. A sub-stage distinction helps future profiling.

3. **Display list rebuild vs reuse framing**: `ProfilingStage::RetainedDisplayListUpdate` already exists. The metric that needs to be added is which fraction of subtrees hit the reuse path. This is `DisplayListMetrics::subtree_segments_reused` — already tracked. The budget work is to establish what the target reuse ratio should be under each canonical workload.

**Canonical workloads to budget** (all already defined in existing benchmark scenarios):
- Navigation-bar hover: expect Layout = cache hit (no geometry change), only PAINT dirty
- Navigation-bar service update (audio backend emit): expect Layout = one or two LAYOUT-dirty nodes
- Audio-popover slider drag: expect Layout = LAYOUT dirty on slider node chain only
- Surface open (popover appears): expect TREE_REBUILD on the new popover subtree only
- Backend-driven text update (clock tick): expect TEXT-dirty on the text leaf node only

---

## Sources

- Taffy `TaffyTree` API — Context7 `/dioxuslabs/taffy` documentation (HIGH confidence)
- MESH `crates/core/ui/elements/src/layout.rs` — current per-frame TaffyTree construction (direct codebase inspection)
- MESH `crates/core/frontend/render/src/display_list.rs` — current `RetainedPaintSubtree`, `PaintSubtreeBuilder::append_child`, `RetainedCommandSpan` (direct codebase inspection)
- MESH `crates/core/foundation/debug/src/lib.rs` — `ProfilingStage` enum (direct codebase inspection)
- Flutter rendering architecture — [Inside Flutter](https://docs.flutter.dev/resources/inside-flutter), [Layer class Dart API](https://api.flutter.dev/flutter/rendering/Layer-class.html) (MEDIUM confidence — general architecture, not MESH-specific)
- Qt Quick scene graph — [Qt Quick Scene Graph docs](https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html), [QSGNode Class](https://doc.qt.io/qt-6/qsgnode.html) (HIGH confidence — official Qt docs)
- Mozilla retained display lists — [Retained Display Lists Mozilla Gfx Blog](https://mozillagfx.wordpress.com/2018/01/09/retained-display-lists/) (MEDIUM confidence — 2018 article; approach aligns with MESH current design)

---

*Feature research for: MESH v1.21 Retained Layout & Display List*
*Researched: 2026-06-18*
