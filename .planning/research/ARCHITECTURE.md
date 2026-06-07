# Typed Dependency Tracking: Integration Architecture

**Domain:** Smart invalidation for retained-mode shell framework (v1.18)
**Researched:** 2026-06-07
**Confidence:** HIGH (primary sources: direct source reading of all 10+ relevant MESH crate files; live codebase)

---

## Integration Points

### 0. Executive Summary

MESH already has three foundations that make typed dependency tracking feasible:

1. **`tracked_service_fields`** in `ScriptContext` — per-component tracking of which service fields Luau read during render (e.g., `audio.percent`, `audio.muted`)
2. **`ComponentDirtyFlags` bitmask** — component-level dirty categories (SCRIPT, STATE, STYLE, LAYOUT, PAINT, TEXT, etc.)
3. **`StyleRuleIndex` with state-bit bucketing** — groups CSS rules by tag/class/id/state for candidate filtering

The current pipeline is **coarse**: any service state change → `TREE_REBUILD` (full rebuild + repaint), and any interaction restyle walks the entire tree even though only one node changed.

The upgrade splits into three integration points, each building on the last, with no circular dependencies:

| # | Integration Point | Ships | Depends On |
|---|-------------------|-------|------------|
| A | StyleRuleIndex: per-rule state dependency masks | `mesh-core-elements` | Nothing |
| B | Per-node service field dependency tracking | `mesh-core-scripting` + `mesh-core-shell` | Nothing (parallel with A) |
| C | Narrow invalidation + tracked-field routing | `mesh-core-shell` | A + B |

### 1. Integration Point A: Selector Dependency Sets at StyleRuleIndex

**Current state** (`crates/core/ui/elements/src/style/resolve.rs:187-296`):

```rust
pub struct StyleRuleIndex {
    rules_ptr: usize,
    rules_len: usize,
    tag: HashMap<String, Vec<usize>>,
    class: HashMap<String, Vec<usize>>,
    id: HashMap<String, Vec<usize>>,
    state: Vec<(u32, Vec<usize>)>,       // (state_bit, rule_indices) — primary key only
    fallback: Vec<usize>,
}
```

`index_state_selector` groups rules by their *primary* index key (the best single key from the compound selector, preferring id > class > tag > state > universal). This is used only for candidate filtering — reducing the candidate set — but every node still evaluates all candidates during restyle.

**What's missing:** The index knows which *single* state bit is the "best" index key for each rule, but:
- It does not know ALL state bits referenced in a compound selector (e.g., `Compound([Tag("button"), State("button", "hover")])` only indexes the Tag)
- It does not record which rules are affected when a specific state bit changes
- The restyle path in `finalize_tree()` (`rendering.rs:217`) detects "style-only dirty from interaction" and skips layout, but still calls `restyle_subtree_cached()` — a full tree walk

**Proposed extension — `RuleDependencyMask`:**

```rust
/// Per-rule: which state bits are referenced anywhere in this rule's selector.
/// Computed during StyleRuleIndex::new().
#[derive(Debug, Clone)]
struct RuleDependencyMask {
    /// Bitmask of all STATE_* bits referenced by this rule's selector (anywhere).
    depends_on_states: u32,
    /// If the selector targets a specific tag (e.g., button:hover → "button"),
    /// used to narrow which nodes need restyle when a state changes.
    constrained_tag: Option<String>,
    /// If the selector targets a specific class (e.g., .menu:active → "menu"),
    /// used to narrow which nodes need restyle.
    constrained_class: Option<String>,
}
```

**What gets added to StyleRuleIndex:**

```rust
pub struct StyleRuleIndex {
    // ... existing fields: tag, class, id, state, fallback ...

    /// Per-rule dependency masks, indexed by rule index.
    rule_dep_masks: Vec<RuleDependencyMask>,
    /// Reverse index: for each state bit, all rule indices that reference it.
    /// [STATE_HOVERED] → [rule_3, rule_12, rule_45]
    state_to_rules: [Vec<usize>; 13],  // one entry per STATE_* constant
}
```

The existing `state: Vec<(u32, Vec<usize>)>` is kept for backward compat — it records the *primary* index key for candidate filtering. The new `state_to_rules` records *all* rules referencing each state bit, enabling O(1) lookup: "which rules care about hover changes?"

**How this changes interaction restyle:**

*Before (current):*
```
Interaction state change (hovered: false→true on node_5)
→ invalidate_interaction_restyle()
→ INTERACTION_RESTYLE (component-wide flags)
→ paint() → restyle_retained_tree() → finalize_tree()
  → targeted_interaction_restyle=true (skips layout, but full tree walk)
  → resolver.restyle_subtree_cached(ROOT, ...)
  → walks ALL 500 nodes, re-validates every selector
```

*After:*
```
Interaction state change (hovered: false→true on node_5)
→ compute changed_bits = prev_state ^ next_state → STATE_HOVERED
→ lookup state_to_rules[STATE_HOVERED] → [rule_3, rule_12]
→ collect_nodes: all nodes matching constrained_tag/class of rules 3,12
  → node_5 (button that was hovered) + node_5's children that match
→ resolver.restyle_nodes(affected_node_ids, ...)  // ~5 nodes instead of 500
→ mark layout ancestors dirty (from affected nodes up to root)
```

**New StyleResolver method needed:**

```rust
pub fn restyle_nodes_cached(
    &self,
    root: &mut WidgetNode,
    rules: &[StyleRule],
    context: StyleContext,
    index_cache: &mut Option<StyleRuleIndex>,
    target_node_ids: &HashSet<NodeId>,
) {
    let index = ensure_index(rules, index_cache);
    self.restyle_nodes_with_index(root, rules, index, context, target_node_ids);
}

fn restyle_nodes_with_index(
    &self,
    node: &mut WidgetNode,
    rules: &[StyleRule],
    index: &StyleRuleIndex,
    context: StyleContext,
    parent_style: Option<&ParentInheritedStyle>,
    target_node_ids: &HashSet<NodeId>,
) {
    if target_node_ids.contains(&node.id) || target_node_ids.is_empty() {
        // Restyle this node (re-run style resolution)
        let attrs = StyleNodeAttrs::from_node(node);
        node.computed_style = self.resolve_node_style_with_attrs_indexed_no_diagnostics(
            rules, index, &attrs, context,
        );
        if let Some(parent) = parent_style {
            inherit_retained_text_style(&mut node.computed_style, parent);
        }
    }
    let effective_parent = if target_node_ids.contains(&node.id) {
        // This node's computed style changed; children must inherit the new values
        Some(ParentInheritedStyle::from(&node.computed_style))
    } else {
        parent_style
    };
    for child in &mut node.children {
        self.restyle_nodes_with_index(child, rules, index, context, effective_parent, target_node_ids);
    }
}
```

**The constrained_tag/class filter matters.** When `button:hover` changes, only `<button>` elements are affected (and their descendants that inherit visual properties). A `column` element that happens to be hovered should not have its `box` children restyled just because hover state changed — unless rules reference them. The `constrained_tag` enables this: only nodes whose tag matches the rule's target tag are collected.

**Integration into `FrontendSurfaceComponent` rendering.rs:**

In `finalize_tree()`, the `targeted_interaction_restyle` code path (line 217) changes from:
```rust
// Current: global restyle with layout skip
resolver.restyle_subtree_cached(tree, restyle_rules, context, index_cache);
```
to:
```rust
// New: per-node selective restyle
let affected = self.collect_state_change_affected_nodes(tree, index_cache, changed_state_bits);
resolver.restyle_nodes_cached(tree, restyle_rules, context, index_cache, &affected);
```

### 2. Integration Point B: Per-Node Service Dependency Tracking

**Current state** (`crates/core/runtime/scripting/src/context/proxy.rs:14-159`):

The service proxy metatable `__index` already tracks reads at the **component level**:
```rust
// Line 152-157: When Luau reads audio.percent,
// records ("audio", "percent") in tracked_service_fields
tracked_service_fields
    .lock().unwrap()
    .entry(service_name.clone())
    .or_default()
    .insert(key.clone());
```

And `ScriptContext.tracked_service_fields_changed()` (`runtime.rs:336-351`) already compares previous vs. next payload values for tracked fields. This is used in `handle_service_event()` (`shell_component.rs:167`) — if tracked fields changed, `needs_rebuild = true`.

**What's missing:** The tracking doesn't know *which WidgetNode* read which field. When `audio.percent` changes, every component with the capability gets `TREE_REBUILD` — all nodes rebuild, restyle, and layout even if only 2/500 nodes render `audio.percent`.

**Proposed extension — Per-Node Service Dependency Index:**

```rust
/// Bi-directional index: which WidgetNode reads which service fields,
/// and conversely, which nodes are affected by a specific field change.
#[derive(Debug, Clone, Default)]
pub struct NodeServiceFieldDependencies {
    /// Forward: NodeId → set of (service_name, field_name) read by this node.
    node_fields: HashMap<NodeId, HashSet<(String, String)>>,
    /// Reverse: (service_name, field_name) → set of NodeIds that read it.
    /// This is the primary lookup for service event invalidation.
    field_nodes: HashMap<(String, String), HashSet<NodeId>>,
}
```

**How reads become per-node — the approach:**

The template expression evaluator in `build_tree_with_state` (`mesh-core-frontend`) evaluates `{audio_label}` bindings for each node. The approach is **Rust-side snapshotting**, not Lua metatable threading:

1. Before evaluating a node's expression, snapshot `tracked_service_fields` (the existing HashMap)
2. Evaluate the expression (triggers service proxy `__index` → records field reads)
3. After evaluation, diff the before/after snapshots — any new entries are fields read by THIS node
4. Record: `node_id → (service, field)` in `NodeServiceFieldDependencies`
5. Clear the snapshot for the next node

This avoids threading `NodeId` into `mlua` Lua state, which would be fragile.

**Where it lives in FrontendSurfaceComponent:**

```rust
pub(super) struct FrontendSurfaceComponent {
    // ... existing fields ...

    /// Per-node service field dependencies, rebuilt each render cycle.
    /// Cleared before tree build, populated during expression evaluation.
    node_service_deps: NodeServiceFieldDependencies,

    /// Lookup cache: service field → dirty flags to apply.
    /// Populated from node_service_deps after render for fast event handling.
    service_field_impact: HashMap<(String, String), ComponentDirtyFlags>,
}
```

**Build sequence during paint():**

```
paint() → build_tree() or restyle_retained_tree()
→ tree built with expressions evaluated
→ node_service_deps populated (per-node field reads captured during expression eval)
→ service_field_impact computed: for each (service, field), collect all node_ids,
  determine minimal dirty flags needed (typically STYLE, but LAYOUT if geometry-dependent)
```

**Integration into ScriptContext:**

`ScriptContext` gains a `field_read_snapshot()` method that returns the current per-node reads. The template evaluator in `mesh-core-frontend` calls this after each node's bindings are evaluated.

No change to the service proxy itself — the Lua-side `__index` handler continues to record into `tracked_service_fields` at the component level. The Rust-side snapshot diff isolates per-node reads.

### 3. Integration Point C: Narrow Service Event Routing + Invalidation

**Current state** (`crates/core/shell/src/shell/runtime/service_state.rs:167-184`, `shell_component.rs:119-182`):

```rust
// deliver_service_event iterates ALL components
for component in &mut self.components {
    if !component.observes_service_event(event) {  // capability check only
        continue;
    }
    requests.extend(component.handle_service_event(event));
}

// handle_service_event per runtime:
let tracked_fields_changed = runtime.script_ctx.tracked_service_fields_changed(
    service_name, previous.as_ref(), payload,
);
if state_changed || tracked_fields_changed {
    needs_rebuild = true;
}
// ... later: invalidate_script_state() → TREE_REBUILD (component-wide)
```

The tracked-field check is already component-scope. But the outcome is still `TREE_REBUILD` — full tree rebuild + full repaint.

**What changes:**

**Layer 1: Component-level field-aware routing in `deliver_service_event`:**

The `observes_service_event` method currently checks only capabilities. With per-node tracking, it can also short-circuit when no runtime read any of the changed fields:

```rust
pub(super) fn affected_by_service_change(
    &self,
    service: &str,
    changed_fields: &HashSet<String>,
) -> bool {
    let Ok(runtimes) = self.runtimes.lock() else { return true; };
    runtimes.values().any(|runtime| {
        runtime.script_ctx.any_tracked_field_changed(service, changed_fields)
    })
}
```

If no runtime tracked any changed field, the component skips the event entirely.

**Layer 2: Per-node invalidation in `handle_service_event`:**

Replace `invalidate_script_state()` (TREE_REBUILD) with narrow = per-node dirty marking:

```rust
pub(super) fn invalidate_service_nodes(
    &mut self,
    service: &str,
    changed_fields: &[(String, String)],
) {
    let mut affected_nodes: HashSet<NodeId> = HashSet::new();
    for (svc, field) in changed_fields {
        if svc == service {
            if let Some(nodes) = self.node_service_deps.field_nodes.get(&(svc.clone(), field.clone())) {
                affected_nodes.extend(nodes);
            }
        }
    }

    if affected_nodes.is_empty() {
        return; // no node rendered any changed field — skip entirely
    }

    let total_nodes = self.node_count();
    if affected_nodes.len() > total_nodes / 2 || total_nodes == 0 {
        // Fallback: too many nodes affected → full rebuild
        self.invalidate_script_state();
        return;
    }

    // Mark affected nodes as STYLE dirty in retained tree
    self.retained_tree.mark_nodes_dirty(&affected_nodes, RetainedNodeDirtyFlags::SERVICE_STATE);
    // Mark layout ancestors dirty (props up from affected nodes to root)
    self.retained_tree.mark_layout_ancestors_dirty(&affected_nodes);
    self.dirty = true;
    self.render_hooks_pending = true;
    self.surface_pixels_invalid = true;
}
```

**Layer 3: Per-node restyle in paint():**

The `paint()` method currently checks `requires_tree_rebuild` (full rebuild) vs `can_use_retained_path` (restyle with layout skip). With narrow invalidation, a third path emerges:

```rust
// In paint(), after take_dirty_for_paint():
if requires_tree_rebuild {
    tree = self.build_tree(theme, content_width, content_height);
} else if self.has_narrow_service_dirty() {
    // New narrow path: restyle only nodes with SERVICE_STATE dirty
    tree = self.last_tree.take()?;
    let affected = self.retained_tree.nodes_with_flag(RetainedNodeDirtyFlags::SERVICE_STATE);
    resolver.restyle_nodes_cached(&mut tree, restyle_rules, context, index_cache, &affected);
    // Layout only the ancestor chain of affected nodes
    LayoutEngine::compute_ancestors(&mut tree, &affected, ...);
} else if can_use_retained_path {
    tree = self.restyle_retained_tree(theme, content_width, content_height, dirty_types)?;
} else {
    tree = self.build_tree(theme, content_width, content_height);
}
```

### New `RetainedNodeDirtyFlags`:

```rust
bitflags! {
    pub(super) struct RetainedNodeDirtyFlags: u16 {
        const INSERTED = 1 << 0;
        const LAYOUT = 1 << 1;
        const STYLE = 1 << 2;
        const ATTRIBUTES = 1 << 3;
        const CHILDREN = 1 << 4;
        const STATE = 1 << 5;
        const SERVICE_STATE = 1 << 6;   // NEW: dirty from service field change
    }
}
```

### New methods on `RetainedWidgetTree`:

```rust
impl RetainedWidgetTree {
    /// Mark specific nodes as dirty without requiring a full snapshot diff.
    pub fn mark_nodes_dirty(&mut self, node_ids: &HashSet<NodeId>, flags: RetainedNodeDirtyFlags) {
        for node_id in node_ids {
            if let Some(key) = self.node_keys.get(node_id) {
                self.dirty.insert(*key, self.dirty.get(*key).unwrap_or_default() | flags);
            }
        }
    }

    /// Propagate LAYOUT dirty from affected nodes up to the root.
    pub fn mark_layout_ancestors_dirty(&mut self, node_ids: &HashSet<NodeId>) {
        // Walk the tree structure and mark parent containers as LAYOUT dirty.
        // Requires access to the node's parent chain — stored in WidgetNode or derived
        // from the slotmap key → parent mapping.
    }

    pub fn nodes_with_flag(&self, flag: RetainedNodeDirtyFlags) -> HashSet<NodeId> {
        self.dirty.iter()
            .filter(|(_, flags)| flags.contains(flag))
            .filter_map(|(key, _)| self.node_keys.iter().find(|(_, v)| **v == key).map(|(k, _)| *k))
            .collect()
    }
}
```

---

## New Data Structures (Summary)

### Net-New

| Structure | Crate | Purpose |
|-----------|-------|---------|
| `RuleDependencyMask` | `mesh-core-elements::style::resolve` | Per-rule state dependency info |
| `NodeServiceFieldDependencies` | `mesh-core-shell::component` | Bidirectional map: NodeId↔(service, field) |
| `ServiceFieldReadSnapshot` | `mesh-core-scripting::context` | Per-node field reads captured during one render cycle |

### Modified / Extended

| Structure | Change | Purpose |
|-----------|--------|---------|
| `StyleRuleIndex` | `+rule_dep_masks: Vec<RuleDependencyMask>` | Per-rule state dependency masks |
| `StyleRuleIndex` | `+state_to_rules: [Vec<usize>; 13]` | Reverse: state_bit → affected rule indices |
| `StyleRuleIndex::index_state_selector` | Also record in `state_to_rules` | Builds reverse index |
| `ScriptContext` | `+field_read_snapshot() → ServiceFieldReadSnapshot` | Capture per-node reads |
| `ScriptContext` | `+any_tracked_field_changed(service, fields) → bool` | Fast check for field-aware routing |
| `RetainedNodeDirtyFlags` | `+SERVICE_STATE = 1 << 6` | Distinct from interaction STATE |
| `RetainedWidgetTree` | `+mark_nodes_dirty()` | Set dirty per-node without snapshot diff |
| `RetainedWidgetTree` | `+mark_layout_ancestors_dirty()` | Propagate LAYOUT up parent chain |
| `RetainedWidgetTree` | `+nodes_with_flag()` | Query nodes with specific dirty flag |
| `FrontendSurfaceComponent` | `+node_service_deps: NodeServiceFieldDependencies` | Cache per-component |
| `StyleResolver` | `+restyle_nodes_cached(root, rules, ctx, index, node_ids)` | Per-node restyle |
| `ComponentDirtyFlags` | No change | Narrow invalidation uses per-node dirty, not component |

---

## Data Flow Changes

### Current Coarse Invalidation

```
Service state change (audio.percent: 42→75)
  broadcast_service_event()
    deliver_service_event()
      for each component:
        observes_service_event? (capability check: has service.audio.read?)
          YES → handle_service_event()
            tracked_fields_changed? (per-component: did any field change?)
              YES → invalidate_script_state()
                → TREE_REBUILD (ALL 500 nodes rebuild + full restyle + full layout + full repaint)
```

### After Narrow Invalidation

```
Service state change (audio.percent: 42→75)
  broadcast_service_event()
    deliver_service_event()
      for each component:
        affected_by_service_change? (capability + field-track check)
          YES → handle_service_event()
            compute changed_fields: [("audio", "percent")]
            lookup node_service_deps.field_nodes[("audio", "percent")]
              → {node_42, node_87}
            affected > 50%? → fallback: TREE_REBUILD
            affected ≤ 2 nodes → mark_nodes_dirty(node_42, node_87, SERVICE_STATE)
              → mark_layout_ancestors_dirty(node_42, node_87)
              → dirty=true, surface_pixels_invalid=true
              → paint() restyles only node_42 + node_87 + layout-only ancestors
```

### Current Interaction Restyle

```
Interaction state change (button hovered: false→true)
  invalidate_interaction_restyle()
    → INTERACTION_RESTYLE (STATE|STYLE|LAYOUT|PAINT|ACCESSIBILITY|METRICS)
    → paint() → restyle_retained_tree()
      → finalize_tree() with targeted_interaction_restyle=true
        → resolver.restyle_subtree_cached(ROOT, ...)  // walks ALL 500 nodes
```

### After Selector Dependency Tracking

```
Interaction state change (button hovered: false→true)
  invalidate_interaction_restyle()
    → compute changed_state_bits: STATE_HOVERED
    → lookup StyleRuleIndex.state_to_rules[STATE_HOVERED]
      → affected rules: [rule_3, rule_12]
    → collect nodes matching constrained_tag/class: [node_5, node_5's children matching rule constraints]
      → affected_node_ids: {1005, 1006, 1007}  (3 nodes out of 500)
    → resolver.restyle_nodes_cached(tree, rules, ctx, index, affected_node_ids)
      → restyles 3 nodes + propagates inherited style to children
    → layout: only propagate up from changed nodes
```

---

## Build Order

```
Phase A: Selector Dependency Tracking
  → Crate: mesh-core-elements (style/resolve.rs)
  → No upstream dependencies
  → Unblocks: Phase C interaction narrowing

Phase B: Per-Node Service Dependency Tracking
  → Crates: mesh-core-scripting (context/), mesh-core-shell (component/)
  → No upstream dependencies
  → Can be developed in parallel with Phase A
  → Unblocks: Phase C service narrowing

Phase C: Narrow Invalidation + Routing
  → Crate: mesh-core-shell (component/shell_component.rs, runtime/service_state.rs)
  → Depends on: Phase A (selector deps) + Phase B (per-node field deps)
```

### Phase A Tasks (mesh-core-elements, 2-3 days)

1. Add `RuleDependencyMask` struct with `depends_on_states`, `constrained_tag`, `constrained_class`
2. Extend `StyleRuleIndex::new()` to compute `rule_dep_masks` for each rule and build `state_to_rules` reverse index
3. Add `StyleRuleIndex::rules_affected_by_state_change(changed_bits: u32) → &[usize]` lookup
4. Add `collect_affected_nodes_for_rules(root, rules, affected_rule_indices) → HashSet<NodeId>` helper
5. Add `StyleResolver::restyle_nodes_cached(root, rules, context, index_cache, node_ids)`
6. Wire into `finalize_tree()` `targeted_interaction_restyle` path in `rendering.rs`
7. Test: 500-node tree, hover one button → only that button + style-descendants restyled
8. Preserve: `ProfilingStage::StyleRestyle` timing should show reduction

### Phase B Tasks (mesh-core-scripting + mesh-core-shell, 3-4 days)

1. Add `NodeServiceFieldDependencies` + `ServiceFieldReadSnapshot` types
2. Extend `ScriptContext` with per-node `field_read_snapshot()` method
3. Modify template expression evaluator in `mesh-core-frontend` to snapshot per-node
4. Build `service_field_impact` cache after render in `FrontendSurfaceComponent`
5. Add `any_tracked_field_changed()` fast-path check
6. Test: `audio.percent` read in expression → node_42 tracked
7. Test: `audio.percent` + `audio.muted` → two tracked fields for same node
8. Test: node that doesn't read `audio.*` → not in dependency set
9. Preserve: overhead <1% of render time

### Phase C Tasks (mesh-core-shell, 4-5 days)

1. Add `RetainedNodeDirtyFlags::SERVICE_STATE`
2. Add `RetainedWidgetTree::mark_nodes_dirty()`, `mark_layout_ancestors_dirty()`, `nodes_with_flag()`
3. Add `FrontendSurfaceComponent::invalidate_service_nodes()` — per-node dirty instead of TREE_REBUILD
4. Add `FrontendSurfaceComponent::affected_by_service_change()` — field-aware component routing
5. Wire `invalidate_service_nodes()` into `handle_service_event()`
6. Wire `affected_by_service_change()` into `observes_service_event()`
7. Add `>50% fallback` threshold: preserves existing TREE_REBUILD behavior when most nodes affected
8. Add narrow paint path in `paint()`: restyle only SERVICE_STATE-dirty nodes
9. Test: `audio.percent` change dirties 2/500 nodes → only 2 restyled
10. Test: `audio.muted` change (not tracked) → no invalidation
11. Test: >50% nodes affected → fallback to TREE_REBUILD
12. Preserve: `ComponentInvalidationCounts` adds `narrow_service: u64`, `narrow_interaction: u64`
13. Preserve: `RetainedInvalidationCounts` reflects per-node dirty counts
14. Preserve: all existing profiling (`ProfilingStage`, `RetainedPaintSnapshot`) works unchanged

---

## References

**Primary sources (MESH codebase — live, read in full):**
- `crates/core/ui/elements/src/style/resolve.rs` — `StyleRuleIndex` (L187-296), `StyleNodeAttrs` (L52-61), state bits (L1038-1050), `restyle_subtree_cached` (L615-623), `selector_index_key` (L1011-1036)
- `crates/core/ui/elements/src/tree.rs` — `WidgetNode` (L44-62), `NodeId` (L30), `ElementState` (L13-27)
- `crates/core/shell/src/shell/component.rs` — `ComponentDirtyFlags` (L67-79), `TREE_REBUILD` (L83-90), `invalidate*` methods (L477-521), `take_dirty_for_paint` (L523-543)
- `crates/core/shell/src/shell/component/shell_component.rs` — `handle_service_event` (L119-182), `observes_service_event` (L184-208), `paint()` (L289-489)
- `crates/core/shell/src/shell/component/rendering.rs` — `finalize_tree()` (L169-276), `build_tree()` (L109-149)
- `crates/core/shell/src/shell/component/runtime_tree.rs` — `RetainedWidgetTree` (L83-91), `RetainedNodeDirtyFlags` (L70-79), `RetainedNodeSnapshot` (L184-190)
- `crates/core/shell/src/shell/runtime/service_state.rs` — `broadcast_service_event` (L4-33), `deliver_service_event` (L167-184)
- `crates/core/runtime/scripting/src/context/runtime.rs` — `ScriptContext` (L42-74), `tracked_service_fields` (L59), `tracked_service_fields_changed` (L336-351)
- `crates/core/runtime/scripting/src/context/proxy.rs` — service proxy `__index` tracking (L152-159)
