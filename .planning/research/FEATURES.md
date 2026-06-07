# Feature Landscape: Typed Dependency Tracking (v1.18)

**Domain:** Smart invalidation for retained-mode shell framework
**Researched:** 2026-06-07
**Confidence:** HIGH (all feature requirements mapped to existing codebase structures)

## Table Stakes (Must Have for v1.18)

| Feature | Why Expected | Implementation Approach | Complexity |
|---------|-------------|------------------------|------------|
| **Per-rule selector dependency masks at StyleRuleIndex** | `:hover/:focus/:active` changes currently restyle the entire component tree. Rules must know which state bits they depend on. | Add `RuleDependencyMask` to `StyleRuleIndex::new()`. Build `state_to_rules` reverse index. 13 state bits → 13 `Vec<usize>` arrays. | MEDIUM — extends existing index construction |
| **Per-node selective restyle for state changes** | Only nodes whose matched rules reference the changed state bit should be restyled. A `:hover` change on one button should restyle ~5 nodes, not 500. | Add `StyleResolver::restyle_nodes_cached()`. In finalize_tree(), replace full `restyle_subtree_cached()` with targeted `restyle_nodes_cached(affected_node_ids)`. | MEDIUM — new resolver method, reuse existing `ParentInheritedStyle` pattern |
| **Inherited style propagation on partial restyle** | When a node is restyled and its color/font changes, children that inherit text-style must also be updated, even if not directly restyled. | In `restyle_nodes_with_index()`, after restyling a node, propagate `inherit_retained_text_style()` to children. Track which 5 properties inherit. | LOW — relies on existing `inherit_retained_text_style()` |
| **Per-node service field dependency tracking** | Service state changes must dirty only nodes that actually render the changed fields. `audio.percent` change should dirty 2 nodes, not 200. | During template expression evaluation, snapshot per-node `tracked_service_fields` diffs. Build `NodeServiceFieldDependencies` with forward (node→fields) and reverse (field→nodes) indices. | MEDIUM — affects template evaluator hot path |
| **Narrow per-node invalidation in handle_service_event** | Service events must produce per-node dirty flags, not `TREE_REBUILD`. | Replace `invalidate_script_state()` call in `handle_service_event` with `invalidate_service_nodes(changed_fields)`. Lookup `NodeServiceFieldDependencies` → mark affected nodes dirty. >50% fallback to `TREE_REBUILD`. | HIGH — central invalidation path, must preserve correctness |
| **Layout ancestor propagation from narrow dirty nodes** | When a leaf node's style changes, its parent chain must be marked LAYOUT dirty for the Taffy pass. | Add `RetainedWidgetTree::mark_layout_ancestors_dirty(node_ids)`. Walk parent chain upward for each affected node. | MEDIUM — requires parent chain tracking |
| **Field-aware service event routing** | Components whose runtimes never read the changed service fields should skip the event entirely. | Add `affected_by_service_change()` check in `observes_service_event`. Use per-runtime `any_tracked_field_changed()` fast path. | LOW — gating logic in existing event loop |

## Differentiators (High Value, Moderate Cost — Defer to Future)

| Feature | Value Proposition | Complexity | Defer Reason |
|---------|------------------|------------|-------------|
| **Direct-mutation fast path for text/value changes** | Avoid `build_tree_with_state()` entirely for simple text/content changes. Volume label "65%"→"66%" is one field update. | MEDIUM | Requires structural stability detection (no added/removed children) |
| **Container-query re-evaluation on size changes** | Container-queries dependent on width/height must re-evaluate when container size changes from layout. | MEDIUM | MESH has few container queries today; defer until layout narrowing is stable |
| **Accessibility-only dirty flag narrowing** | Currently ACCESSIBILITY is in `INTERACTION_RESTYLE`, `VISUAL_REPAINT`, etc. — wasted work when only visual properties change. | LOW | Tactical dirty-flag composition fix; do early in milestone |

## Feature Dependencies

```
Phase A: Selector Dependency Tracking
   RuleDependencyMask → state_to_rules reverse index
      └── StyleResolver::restyle_nodes_cached()
         └── finalize_tree() targeted_interaction_restyle path

Phase B: Per-Node Service Dependency Tracking
   ServiceFieldReadSnapshot (per-frame capture)
      └── NodeServiceFieldDependencies (bidirectional index)
         └── FrontendSurfaceComponent.node_service_deps

Phase C: Narrow Invalidation + Routing
   (depends on A + B)
   RetainedWidgetTree::mark_nodes_dirty()
      └── handle_service_event() → invalidate_service_nodes()
         └── paint() → narrow restyle path (STATE only, not TREE_REBUILD)
   affected_by_service_change() → observes_service_event() gate
```

## Anti-Features (Explicitly NOT Building)

| Anti-Feature | Why Avoid | What to Do Instead |
|-------------|-----------|-------------------|
| **Per-property dirty categories** (font-size-dirty vs color-dirty vs opacity-dirty) | CSS engine level complexity. MESH supports ~50 properties. Browsers need this; shell UIs don't. | Use per-node STYLE dirty with backdating: if computed style equals previous, skip layout/paint cascade. |
| **Compile-time Luau dependency analysis** | Luau is dynamic — `self.audio["percent"]`, closures, `pairs()`. Static analysis cannot determine what a script reads. | Runtime read tracking via `__index` metatable proxy (already exists). |
| **Full Svelte-style signal system** (`$derived` auto-tracking) | Requires either a compiler (Svelte approach) or wrapping every value in reactive proxies (SolidJS approach). Luau doesn't support Proxies. | Per-node state-key bindings cover 90% of the value at 10% of the complexity. |

## MVP Recommendation (Phase Prioritization)

Prioritize:
1. **Phase A: Selector dependency tracking** — highest impact (60fps interaction events), lowest risk (extends existing index)
2. **Phase B: Per-node service field tracking** — builds the dependency map needed for Phase C
3. **Phase C: Narrow invalidation + field-aware routing** — combines both systems into final narrow pipeline

Defer: Direct-mutation fast path: valuable but requires structural stability detection that depends on per-node tracking being stable first.

## Sources

- MESH v1.18 milestone goal (PROJECT.md: "Replace coarse 'tree rebuild + full repaint' invalidation with typed dependency tracking")
- MESH codebase: `crates/core/shell/src/shell/component.rs` — current coarse invalidation paths
- MESH codebase: `crates/core/ui/elements/src/style/resolve.rs` — `StyleRuleIndex` with state-bit bucketing
- MESH codebase: `crates/core/runtime/scripting/src/context/proxy.rs` — existing field read tracking via `__index`
- Stylo (Firefox CSS engine): restyle hints approach for pseudo-class changes
- Svelte 5 runes: signal-based fine-grained reactivity model
