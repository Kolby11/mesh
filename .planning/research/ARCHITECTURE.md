# Architecture Research: v1.21 Retained Layout & Display List

**Domain:** Retained rendering pipeline — Taffy layout retention, rope-style display list, per-stage budget profiling
**Researched:** 2026-06-18
**Confidence:** HIGH — based on direct codebase inspection of all touched files

---

## Current State (What Exists Today)

### Per-frame TaffyTree rebuild (the waste this milestone removes)

Every call to `FrontendSurfaceComponent::paint()` eventually reaches
`LayoutEngine::compute_taffy_layout_with_cache()` in
`crates/core/ui/elements/src/layout.rs`.  That function constructs a brand-new
`TaffyTree<NodeId>` and two fresh `HashMap`s (`node_map`, `text_nodes`), walks
the full `WidgetNode` tree to populate them via `build_taffy_tree()`, calls
`tree.compute_layout_with_measure()`, then writes the resulting `LayoutRect`s
back onto the `WidgetNode` tree via `write_taffy_layout()`.  After the function
returns the `TaffyTree` is dropped.  This happens every frame — even when only
a single leaf node changed.

### Retained structures that already exist on FrontendSurfaceComponent

`FrontendSurfaceComponent` (declared in `crates/core/shell/src/shell/component.rs`) 
already retains:

| Field | Type | Purpose |
|---|---|---|
| `retained_tree` | `RetainedWidgetTree` | Stable `SlotMap`-backed node store with per-node dirty flags (`RetainedNodeDirtyFlags`) |
| `retained_render_objects` | `RenderObjectTree` | Render-object synchronization |
| `retained_display_list` | `RetainedDisplayList` | Paint command store with subtree spans |
| `last_tree` | `Option<WidgetNode>` | Previous frame's full `WidgetNode` tree |
| `intrinsic_layout_cache` | `IntrinsicLayoutCache` | LRU cache for text measurements used by Taffy |

The `RetainedWidgetTree` records per-node dirty flags including `LAYOUT` and
`CHILDREN`, which is exactly the information needed to decide which Taffy nodes
to mark dirty rather than rebuilding the whole tree.

### Retained display list structure (what the rope replaces/extends)

`RetainedDisplayList` (in `crates/core/frontend/render/src/display_list.rs`) 
already has a subtree-oriented paint command store using `RetainedPaintSubtree`
structs containing `Arc<[DisplayPaintCommand]>` slices.  Paint commands are
assembled into a flat `Arc<[DisplayPaintCommand]>` by copying subtree slices
together.  `RetainedCommandSpan` records `(start, end)` index pairs into that
flat array so selection can skip clean spans.  This is already close to
rope-style; the goal is to go from "copy clean spans into new Vec" to
"reference clean spans directly through a rope node".

### Profiling infrastructure that exists

`ProfilingStage` enum in `crates/core/foundation/debug/src/lib.rs` already has:
`Layout`, `RenderObjectSync`, `RetainedDisplayListUpdate`, `PaintTraversal`,
`TextShaping`, `IconImageRaster`, `Paint`, `PresentCommit`,
`TotalSurfaceRender`, `SchedulerIdle`.  `FrontendSurfaceComponent` calls
`record_profiling_stage_with_elapsed()` at multiple points in `paint()`.

---

## Integration Architecture

### System Overview

```
FrontendSurfaceComponent (mesh-core-shell)
  │
  ├── RetainedWidgetTree         ← already exists; LAYOUT/CHILDREN dirty bits
  │                                drive retained Taffy mutations
  │
  ├── PerSurfaceLayoutState      ← NEW: retained TaffyTree + node-id map
  │     TaffyTree<NodeId>
  │     HashMap<NodeId, TaffyNodeId>
  │     surface_size: (u32, u32)
  │
  ├── retained_render_objects    ← unchanged
  │
  ├── RopeDisplayList            ← MODIFIED: RetainedDisplayList gains
  │     rope_nodes: Vec<RopeNode>  rope-node storage alongside existing
  │     (existing subtree fields)  span/subtree machinery
  │
  └── ProfilingBudget            ← NEW lightweight wrapper (optional struct)
        or inline via existing record_profiling_stage_with_elapsed()
```

### Where retained TaffyTree lives

The retained `TaffyTree` must live on `FrontendSurfaceComponent`.  It is
strictly per-surface state — one `TaffyTree` per mounted surface component,
never shared across surfaces.

**Do not create a new crate boundary for this.**  The `TaffyTree` belongs in a
new `PerSurfaceLayoutState` struct defined in
`crates/core/ui/elements/src/layout.rs` (the same file that owns `LayoutEngine`
and `IntrinsicLayoutCache`).  `FrontendSurfaceComponent` adds a field
`layout_state: PerSurfaceLayoutState` alongside `intrinsic_layout_cache`.

### PerSurfaceLayoutState — new struct

```rust
// crates/core/ui/elements/src/layout.rs

pub struct PerSurfaceLayoutState {
    tree: TaffyTree<NodeId>,
    node_map: HashMap<NodeId, TaffyNodeId>,   // MESH NodeId → Taffy NodeId
    text_nodes: HashMap<NodeId, TextMeasureData>,
    last_available: (f32, f32),               // width, height
    valid: bool,                              // false = must do full rebuild
}
```

`IntrinsicLayoutCache` stays a separate field on `FrontendSurfaceComponent` as
it is already — text measurement caching is orthogonal to layout state
retention.

### LayoutEngine API additions

Add two new public methods to `LayoutEngine`:

```rust
impl LayoutEngine {
    /// First-time or post-reset: build TaffyTree from scratch, populate
    /// node_map, run compute_layout_with_measure, write LayoutRects back.
    pub fn build_retained(
        root: &mut WidgetNode,
        available_width: f32,
        available_height: f32,
        state: &mut PerSurfaceLayoutState,
        intrinsic_cache: &mut IntrinsicLayoutCache,
        measurer: Option<&dyn TextMeasurer>,
    );

    /// Incremental update: apply dirty-node set to the existing TaffyTree,
    /// then call compute_layout_with_measure (Taffy only recomputes dirty
    /// subtrees), write changed LayoutRects back onto the WidgetNode tree.
    pub fn update_retained(
        root: &mut WidgetNode,
        available_width: f32,
        available_height: f32,
        dirty_node_ids: &HashSet<NodeId>,    // from RetainedWidgetTree
        dirty_flags: &HashMap<NodeId, RetainedNodeDirtyFlags>,
        state: &mut PerSurfaceLayoutState,
        intrinsic_cache: &mut IntrinsicLayoutCache,
        measurer: Option<&dyn TextMeasurer>,
    );
}
```

`update_retained` walks only the dirty set:
- `LAYOUT | STYLE` dirty → call `taffy.set_style(taffy_node, new_style)` (Taffy
  marks the node and its ancestors dirty internally)
- `CHILDREN` dirty → call `taffy.set_children(taffy_node, new_children)` with
  the new ordered child `TaffyNodeId` list, inserting or removing leaf nodes as
  needed
- `TREE_REBUILD` (>50% threshold already exists) → call `build_retained`
  instead

After mutations, call `tree.compute_layout_with_measure(root_taffy_id, ...)`.
Taffy's incremental solver only recomputes the dirty subtrees.

### How this hooks into the existing paint path

In `FrontendSurfaceComponent::paint()`, the current call sequence is:

```
build_tree / restyle_retained_tree
  → apply_style_animations_with_previous
  → retained_tree.update(&tree)           ← produces dirty flags
  → retained_render_objects.update(...)
  → retained_display_list.update_with_dirty_nodes(...)
```

Layout is currently implicit inside `build_tree` (calls
`LayoutEngine::compute_with_intrinsic_cache_and_measurer`).  After this
milestone it becomes explicit:

```
build_tree / restyle_retained_tree          ← unchanged; produces WidgetNode tree
                                               with ComputedStyle but no LayoutRect yet
  → retained_tree.update(&tree)             ← produces dirty NodeId set
  → if layout_state.valid && !requires_tree_rebuild {
        LayoutEngine::update_retained(...)  ← mutates TaffyTree in place;
                                               writes LayoutRects onto &mut tree
    } else {
        LayoutEngine::build_retained(...)   ← full rebuild into retained state
    }
  → apply_style_animations_with_previous    ← unchanged
  → retained_render_objects.update(...)     ← unchanged
  → retained_display_list.update_with_dirty_nodes(...)  ← unchanged
```

The `layout_state` field invalidation mirrors the existing pattern for other
retained structures.  It is reset to `valid: false` in `theme_changed()` and
`locale_changed()` exactly as `retained_tree`, `retained_render_objects`, and
`retained_display_list` are reset today.

### Rope-style display list — what changes

`RetainedDisplayList` already stores per-subtree `Arc<[DisplayPaintCommand]>`
slices and assembles a flat paint command vector by copying them together.  The
rope extension replaces that copy with a rope node reference so clean subtrees
are never copied.

**New type inside `display_list.rs`:**

```rust
enum RopeNode {
    /// Subtree whose commands are held as a shared arc slice. Reused when
    /// the subtree's retained generation matches.
    Retained {
        node_id: NodeId,
        commands: Arc<[DisplayPaintCommand]>,
        kinds: Arc<[DisplayPaintCommandKind]>,
        generation: u64,
    },
    /// Newly built or rebuilt commands for a dirty subtree.
    Rebuilt {
        node_id: NodeId,
        commands: Vec<DisplayPaintCommand>,
        kinds: Vec<DisplayPaintCommandKind>,
    },
}
```

`RetainedDisplayList` grows a `rope: Vec<RopeNode>` alongside the existing
`subtrees` and `command_spans` fields.  The paint phase iterates rope nodes in
order rather than copying into a flat Vec.

`SelectedDisplayListPaint` already has span-based iteration
(`SelectedDisplayListSelection::Spans`).  The iterator is extended to yield
from rope nodes directly without a flatten-copy step.

**No new crate boundary.**  The rope lives in `display_list.rs` in
`mesh-core-render`.

### Per-stage budget profiling — what changes

No new crate boundary and no new struct required.  The existing
`record_profiling_stage_with_elapsed()` call sites in `paint()` already cover
the stages.  What is missing is:

1. A `ProfilingStage::LayoutRetained` variant (alongside `Layout`) so retained
   vs full-rebuild layout time is separately attributed.
2. Budget thresholds as constants alongside the stage records — checked after
   each stage; if exceeded, emit a `tracing::warn!` (debug builds) or record an
   `invalidation_snapshot` flag (release builds).  No new public API required
   beyond the new variant.

Add `LayoutRetained` to `ProfilingStage` in `mesh-core-debug`.

---

## Modified Files

| File | Change |
|---|---|
| `crates/core/ui/elements/src/layout.rs` | Add `PerSurfaceLayoutState`; add `build_retained` and `update_retained` to `LayoutEngine` |
| `crates/core/shell/src/shell/component.rs` | Add `layout_state: PerSurfaceLayoutState` field to `FrontendSurfaceComponent`; remove `intrinsic_layout_cache` independence (it stays but is now passed alongside `layout_state`) |
| `crates/core/shell/src/shell/component/shell_component.rs` | `theme_changed` / `locale_changed`: reset `layout_state.valid = false` |
| `crates/core/shell/src/shell/component/rendering.rs` | Thread `layout_state` into the layout call; choose `update_retained` vs `build_retained` based on dirty flags |
| `crates/core/frontend/render/src/display_list.rs` | Add `RopeNode` enum and `rope: Vec<RopeNode>` to `RetainedDisplayList`; extend `SelectedDisplayListPaint` iterator to consume rope nodes without a flat-copy |
| `crates/core/foundation/debug/src/lib.rs` | Add `ProfilingStage::LayoutRetained` variant |

## New Files

None required.  No new crate boundaries needed.

---

## Component Boundaries and Ownership

| Component | Owns | Does Not Own |
|---|---|---|
| `mesh-core-elements` (layout.rs) | `PerSurfaceLayoutState`, `LayoutEngine::build_retained`, `LayoutEngine::update_retained`, `TaffyTree<NodeId>` | Surface lifecycle, dirty-flag collection |
| `mesh-core-shell` (component.rs) | `FrontendSurfaceComponent.layout_state`, reset logic on theme/locale change, dirty-flag dispatch to `update_retained` | Taffy internals |
| `mesh-core-render` (display_list.rs) | `RopeNode`, rope assembly, `SelectedDisplayListPaint` rope iteration | Layout computation |
| `mesh-core-debug` (lib.rs) | `ProfilingStage::LayoutRetained` | Budget enforcement logic |

---

## Data Flow: Retained Layout Pass

```
RetainedWidgetTree::update(&tree)
    produces: dirty_node_ids: HashSet<NodeId>
              dirty_flags: HashMap<NodeId, RetainedNodeDirtyFlags>
                  │
                  ▼
    if layout_state.valid && !requires_tree_rebuild
        LayoutEngine::update_retained(
            root: &mut WidgetNode,
            dirty_node_ids,
            dirty_flags,
            state: &mut PerSurfaceLayoutState,   ← mutated in place
            intrinsic_cache,
            measurer,
        )
        for each node_id in dirty_node_ids:
            if LAYOUT | STYLE dirty:
                taffy.set_style(taffy_node, recomputed_style)
            if CHILDREN dirty:
                taffy.set_children(taffy_node, new_children)
        taffy.compute_layout_with_measure(root_taffy_id, available_space, ...)
        write_changed_taffy_layout(root, &state.tree, &state.node_map)
    else
        LayoutEngine::build_retained(...)       ← full rebuild, sets state.valid = true
                  │
                  ▼
    WidgetNode tree now has updated LayoutRects
                  │
                  ▼
    retained_render_objects.update(&tree)       ← unchanged
                  │
                  ▼
    retained_display_list.update_with_dirty_nodes(...)
        for each dirty subtree: emit RopeNode::Rebuilt(...)
        for each clean subtree: emit RopeNode::Retained { commands: Arc::clone(...) }
                  │
                  ▼
    SelectedDisplayListPaint iterates rope nodes
    → yields &DisplayPaintCommand without flat-copy
```

---

## Phase Build Order

### Phase 1 — Retained TaffyTree

**Why first:** Layout is the dependency for both the display list and for any
profiling numbers to be meaningful.  The dirty-bit infrastructure from v1.18 is
complete.  `RetainedWidgetTree` already has `LAYOUT | CHILDREN` per-node flags.
This phase slots between the existing `retained_tree.update()` call and the
display list update.

Deliverables:
- `PerSurfaceLayoutState` struct in `layout.rs`
- `LayoutEngine::build_retained` and `LayoutEngine::update_retained`
- `layout_state` field on `FrontendSurfaceComponent`; `valid = false` resets in
  `theme_changed`, `locale_changed`, `reload_source`
- `rendering.rs` wired to choose retained vs full path
- Pixel-equivalence tests proving layout output is unchanged

### Phase 2 — Rope-style display list

**Why second:** Requires correct `LayoutRect` values in the `WidgetNode` tree
before subtree span bounds can be computed and retained.  Phase 1 provides
that.  The existing `Arc<[DisplayPaintCommand]>` subtree machinery in
`RetainedDisplayList` is the direct ancestor — this phase replaces the
flatten-copy assembly step.

Deliverables:
- `RopeNode` enum in `display_list.rs`
- `rope: Vec<RopeNode>` on `RetainedDisplayList`
- `SelectedDisplayListPaint` iterator extended to consume rope nodes
- Remove the `Vec::extend_from_slice` copy path for clean subtrees
- Existing `subtree_segments_reused` / `subtree_commands_rebuilt` metrics
  updated to reflect rope semantics

### Phase 3 — Per-stage budget profiling

**Why third:** Profiling numbers are only meaningful once the retained paths
from phases 1 and 2 are in place.  This phase adds the `LayoutRetained` stage
variant, wires it into the existing `record_profiling_stage_with_elapsed()`
call sites, and establishes the per-stage budget constants that can be used to
flag regressions in CI.

Deliverables:
- `ProfilingStage::LayoutRetained` in `mesh-core-debug`
- Budget constant table alongside the stage records
- `tracing::warn!` when a stage exceeds its budget (debug builds)
- Debug inspector surface updated to show `layout_retained` separately from
  `layout`

---

## What Stays Unchanged

- `RetainedWidgetTree` — no changes needed; its dirty flags drive Phase 1
- `RenderObjectTree` — unchanged
- `FrontendRenderEngine`, `PixelBuffer`, `PaintProfilingMetrics` — unchanged
- `Shell::render_components()` in `render.rs` — unchanged; all work stays
  inside `FrontendSurfaceComponent::paint()`
- `build_tree`, `restyle_retained_tree`, `narrow_script_update` — unchanged;
  they produce the `WidgetNode` tree; layout is a downstream consumer
- Damage rect accumulation and present path — unchanged
- `IntrinsicLayoutCache` — stays as a separate field; no behavioral change

---

## Anti-Patterns to Avoid

### Putting TaffyTree on LayoutEngine as a global or thread-local

`LayoutEngine` is a zero-sized stateless struct by design.  Global or
thread-local `TaffyTree` state would couple surfaces and make the retained path
non-deterministic when surfaces share a thread.  The retained state must be
per-surface, owned by `FrontendSurfaceComponent`.

### Creating a new crate for retained layout state

The `TaffyTree` dependency already exists in `mesh-core-elements`.  Adding a
`mesh-core-layout` or `mesh-core-retained-layout` crate just to house
`PerSurfaceLayoutState` would create an unnecessary dependency edge and split
the layout API across two crates.  Keep it in `layout.rs`.

### Eager TaffyNodeId invalidation on every dirty bit

Only `LAYOUT`, `STYLE`, and `CHILDREN` dirty flags require Taffy mutations.
`PAINT`, `TEXT` (text content only), and `STATE` dirty bits change display list
content but not layout geometry — they must not trigger `taffy.set_style()` or
`taffy.set_children()`, which would incorrectly mark the layout dirty.

### Copying rope segments on select

`SelectedDisplayListPaint` must yield `&DisplayPaintCommand` references from
rope nodes' `Arc` slices without copying into an intermediate `Vec`.  Any path
that copies the full command list during `select_paint_commands` defeats the
purpose of the rope.

### Splitting the rope assembly across crate boundaries

`RopeNode` must stay in `display_list.rs` inside `mesh-core-render` so it can
borrow `Arc<[DisplayPaintCommand]>` slices without lifetime complexity or
cross-crate `Arc` re-wrapping.

---

## Sources

- Direct inspection: `crates/core/ui/elements/src/layout.rs`
- Direct inspection: `crates/core/shell/src/shell/component.rs` (field declarations)
- Direct inspection: `crates/core/shell/src/shell/component/shell_component.rs` (paint path)
- Direct inspection: `crates/core/shell/src/shell/component/runtime_tree.rs` (dirty flags)
- Direct inspection: `crates/core/frontend/render/src/display_list.rs` (RetainedDisplayList, RetainedPaintSubtree, RopeNode hooks)
- Direct inspection: `crates/core/shell/src/shell/runtime/render.rs` (Shell::render_components)
- Direct inspection: `crates/core/foundation/debug/src/lib.rs` (ProfilingStage)
- Taffy documentation: incremental layout via `set_style` / `set_children` marks only the subtree dirty; `compute_layout_with_measure` is safe to call every frame on a retained tree

---
*Architecture research for: v1.21 Retained Layout & Display List*
*Researched: 2026-06-18*
