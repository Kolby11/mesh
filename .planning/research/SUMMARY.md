# Project Research Summary

**Project:** MESH v1.18 — Typed Dependency Tracking (Smart Invalidation)
**Domain:** Retained-mode shell framework — pipeline optimization
**Researched:** 2026-06-07
**Confidence:** HIGH

## Executive Summary

MESH v1.18 replaces the coarse "tree rebuild + full repaint" invalidation pipeline with typed dependency tracking. Today, a single `audio.percent` field change triggers `TREE_REBUILD` — 500 nodes rebuilt, restyled, laid out, and repainted. A `:hover` state change on one button walks the entire component tree revalidating every CSS selector. This is correct but wasteful, and it limits the number of components per surface.

The upgrade splits into three integration points, each building on the last with no circular dependencies. Phase A adds per-rule state dependency masks to `StyleRuleIndex` so that `:hover` restyles 3 nodes instead of 500. Phase B captures per-node service field reads during expression evaluation, building a bidirectional NodeId↔(service, field) index. Phase C connects both systems into narrow invalidation: service events mark only the nodes that care, and a `>50%` fallback threshold preserves correctness when too many nodes are affected.

**No new crate dependencies are required.** All data structures and algorithms live within existing crates (`mesh-core-elements`, `mesh-core-scripting`, `mesh-core-shell`, `mesh-core-frontend`). The key risks are inherited style values not propagating on partial restyle (Phase A), co-dirtying from script+interaction destroying narrow work (Phase C), and text-measure cascades into layout (Phase C). Each has a specific prevention strategy validated against live codebase analysis. Recommended approach: build Phase A and Phase B in parallel, merge into Phase C, gate on pixel-equivalence tests against full-rebuild baselines.

## Key Findings

### Stack Decisions

**No new crate dependencies.** All data structures use Rust stdlib (`HashMap`, `HashSet`, `Vec`, `u32`) already in MESH's `Cargo.toml`. `RuleDependencyMask` reuses the existing 13 `STATE_*` bit constants. `NodeServiceFieldDependencies` uses the existing `NodeId` type.

**Explicitly rejected:** `fixedbitset` (adds dependency for ~15 lines of bit operations), `petgraph` (overengineered for static rule→node mapping), `salsa` (immutable database pattern incompatible with MESH's mutable widget tree), `dashmap` (all invalidation on single render thread), Luau debug hooks (instruction-level overhead prohibitive vs existing `__index` metatable).

**Modified crates:**

| Crate | Change |
|-------|--------|
| `mesh-core-elements` | `StyleRuleIndex` extended with `RuleDependencyMask` and `state_to_rules` reverse index. `StyleResolver` gains `restyle_nodes_cached()`. |
| `mesh-core-scripting` | `ScriptContext` gains `field_read_snapshot()` and `any_tracked_field_changed()`. |
| `mesh-core-shell` | `RetainedNodeDirtyFlags` adds `SERVICE_STATE`. `RetainedWidgetTree` gains `mark_nodes_dirty()`, `mark_layout_ancestors_dirty()`, `nodes_with_flag()`. `FrontendSurfaceComponent` gains `node_service_deps` and `invalidate_service_nodes()`. |
| `mesh-core-frontend` | Template expression evaluator snapshots per-node field reads during binding evaluation. |

**New data structures:** `RuleDependencyMask` (u32 bitmask + constrained tag/class), `NodeServiceFieldDependencies` (bidirectional HashMap: NodeId↔(service, field)), `ServiceFieldReadSnapshot` (per-frame per-node HashMap).

### Feature Scope

**Must-have (table stakes — 7 features):**
1. Per-rule selector dependency masks at `StyleRuleIndex` (MEDIUM complexity)
2. Per-node selective restyle for state changes (MEDIUM)
3. Inherited style propagation on partial restyle (LOW)
4. Per-node service field dependency tracking (MEDIUM)
5. Narrow per-node invalidation in `handle_service_event` (HIGH — central invalidation path)
6. Layout ancestor propagation from narrow dirty nodes (MEDIUM)
7. Field-aware service event routing (LOW — gating logic)

**Differentiators (deferred to future):**
- Direct-mutation fast path for text/value changes (requires structural stability detection)
- Container-query re-evaluation on size changes (few container queries today)
- Accessibility-only dirty flag narrowing (tactical fix, not scope blocker)

**Anti-features (explicitly excluded):**
- Per-property dirty categories (CSS-engine complexity; shell UIs don't need it)
- Compile-time Luau dependency analysis (Luau is dynamic; `__index` metatable covers this)
- Full Svelte-style signal system (requires compiler or Proxy wrapping; Luau doesn't support Proxies)

### Architecture Integration

Three integration points, A and B developable in parallel, C depends on both:

**Point A (mesh-core-elements):** `StyleRuleIndex` construction now computes `RuleDependencyMask` per rule (all referenced state bits + constrained tag/class for node collection). A `state_to_rules: [Vec<usize>; 13]` reverse index provides O(1) lookup from state bit → affected rules. When `:hover` changes, the engine collects only nodes matching constrained tags (~3 nodes) and restyles them. Children of restyled nodes get inherited text-style propagation via existing `inherit_retained_text_style()`.

**Point B (mesh-core-scripting + mesh-core-shell):** Template expression evaluator snapshots `tracked_service_fields` before/after each node's binding evaluation. The diff reveals which (service, field) pairs this node read. Results feed into `NodeServiceFieldDependencies` — a bidirectional index: `node→fields` (forward) and `(service,field)→nodes` (reverse).

**Point C (mesh-core-shell):** Service events check `affected_by_service_change()` (per-runtime gate: skip components whose scripts never read the changed fields), then `invalidate_service_nodes()` marks only affected nodes as `SERVICE_STATE` dirty. A `>50%` fallback preserves `TREE_REBUILD` behavior. The `paint()` method gains a narrow restyle path: restyle only dirty nodes, compute layout only on the ancestor chain, repaint only the damage region.

**Data flow, before:** `audio.percent` change → `TREE_REBUILD` → 500 nodes rebuilt, full restyle, full layout, full repaint.
**Data flow, after:** `audio.percent` change → `field_nodes[("audio","percent")]` → `{node_42, node_87}` → mark SERVICE_STATE dirty → restyle 2 nodes → layout ancestors only → repaint damage region.

### Critical Pitfalls

| # | Pitfall | Phase | Mitigation |
|---|---------|-------|------------|
| 1 | **Inherited style not propagated:** Children of a restyled node keep stale inherited color/font values. | A | Walk children after restyle, re-apply `inherit_retained_text_style()`. Track 5 properties: color, font-family, font-size, font-weight, line-height. |
| 2 | **Co-dirtying destroys narrow work:** `:hover` narrow restyle + backend service update in same frame → SCRIPT dirty triggers full `build_tree()`, discarding narrow work. | C | Respect dirty-type priority: SCRIPT > TEXT > STATE > STYLE > LAYOUT > PAINT. Only narrow when SCRIPT+TEXT are clean. Existing `requires_tree_rebuild` gate already enforces this. |
| 3 | **Text measure cascade:** Service field changes text node content ("65%"→"66%"). Only the text node is flagged dirty, but its parent flex container must recalculate layout. | C | Propagate LAYOUT dirty to ancestors when text intrinsic size changes. |
| 4 | **False negatives on first render:** Before first render, `tracked_service_fields` is empty. Service event arrives → component skips invalidation → stale display. | C | Fall back to `TREE_REBUILD` when tracked set is empty. Existing `invalidate_script_state()` handles this. |

**Testing strategy:** For every benchmark scenario (hover, open/close, slider, traversal, backend-update), run narrow invalidation → capture widget tree + pixel buffer. Force `TREE_REBUILD` → capture same. Assert pixel-identical output AND widget-tree equivalence (all `computed_style` fields, all `layout` bounds).

## Implications for Roadmap

### Phase 1: Selector Dependency Tracking (2-3 days)

**Rationale:** Highest impact (interaction events at 60fps, full-tree restyle is the most measurable waste), lowest risk (extends existing `StyleRuleIndex` construction). No upstream dependencies. Unblocks Phase C interaction narrowing.

**Delivers:** `RuleDependencyMask` struct, `state_to_rules` reverse index, `StyleResolver::restyle_nodes_cached()`, targeted interaction restyle in `finalize_tree()` — `:hover` restyles ~5 nodes instead of 500.

**Crate:** `mesh-core-elements`

**Pitfalls to avoid:** #1 (inherited style propagation — walk children and re-apply), #7 (state bitmask non-orthogonality — accept as overhead), #8 (fallback bucket audit — ensure <5% of rules land in fallback).

### Phase 2: Per-Node Service Field Tracking (3-4 days)

**Rationale:** Builds the dependency map needed for Phase C. No upstream dependencies. Can be developed in parallel with Phase A.

**Delivers:** `NodeServiceFieldDependencies` bidirectional index, `ServiceFieldReadSnapshot`, per-node snapshot diffing in template evaluator, `any_tracked_field_changed()` fast path, `field_read_snapshot()` on `ScriptContext`.

**Crates:** `mesh-core-scripting`, `mesh-core-shell`

**Pitfalls to avoid:** #6 (metatable overhead — cache tracked status on Lua side, profile to ensure <1% render time regression).

### Phase 3: Narrow Invalidation + Field-Aware Routing (4-5 days)

**Rationale:** Combines both dependency systems into the final narrow pipeline. The integration point — consumes `state_to_rules` from A and `field_nodes` from B to make routing decisions.

**Delivers:** `RetainedNodeDirtyFlags::SERVICE_STATE`, `mark_nodes_dirty()` + `mark_layout_ancestors_dirty()` + `nodes_with_flag()` on `RetainedWidgetTree`, `invalidate_service_nodes()` replacing `invalidate_script_state()`, `affected_by_service_change()` gating in `observes_service_event()`, `>50%` fallback threshold, narrow paint path in `paint()`.

**Crate:** `mesh-core-shell`

**Pitfalls to avoid:** #2 (co-dirtying — respect priority order; only narrow when SCRIPT+TEXT clean), #3 (text measure cascade — propagate LAYOUT to ancestors), #4 (first-render false negatives — fallback to `TREE_REBUILD`), #5 (nested JSON — deferred past MVP).

### Phase Ordering Rationale

- Phases A and B have zero overlapping crate modifications — can be developed simultaneously with no merge conflicts.
- Phase C is the integration point that consumes both A's and B's outputs.
- This phased approach builds each concern separately before wiring them together, matching the research goal of "ship narrow invalidation across selector, service, and event routing."
- All phases share a common testing baseline: pixel-equivalence against `TREE_REBUILD` output, making regression detection immediate.

### Research Flags

**Needs deeper research during planning:**
- **Phase C (narrow invalidation):** `mark_layout_ancestors_dirty()` requires parent chain access — stored in `WidgetNode` or derived from slotmap key→parent mapping. Exact API needs validation against current `RetainedWidgetTree` structure. Taffy's support for incremental layout re-computation on ancestor chains needs verification.
- **Phase C (co-dirtying edge cases):** Simultaneous service+interaction+script dirty states in one frame need explicit test coverage. Priority ordering must be verified against real compositor event patterns.

**Skip research-phase (well-documented patterns):**
- **Phase A:** Extends existing `index_state_selector()` and `restyle_subtree_cached()` patterns. Stylo (Firefox) restyle-hints approach provides validated reference architecture.
- **Phase B:** Tactical extension of existing `tracked_service_fields` mechanism. The metatable handler doesn't change — only the Rust-side capture granularity increases.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | **HIGH** | All data structures use Rust stdlib types already in MESH's `Cargo.toml`. No new crates. Every structure traced to a specific existing crate file and line number. |
| Features | **HIGH** | All 7 table-stakes features mapped to exact code locations in the current coarse invalidation pipeline. Each feature's complexity assessed against existing code patterns. |
| Architecture | **HIGH** | All 3 integration points have concrete before/after data flow diagrams, method signatures, and pseudo-code. Every modified struct traced to a live source file. 10+ crate files read in full. |
| Pitfalls | **HIGH** | All 8 pitfalls identified from direct codebase analysis — no speculative hazards. Each has a concrete detection test and mitigation strategy. Cascading failure modes traced through actual render pipeline sequencing. |

**Overall confidence:** HIGH — this is an internal pipeline optimization on a mature codebase. Every integration point extends existing infrastructure rather than introducing new architectural concepts.

### Gaps to Address

- **Parent chain access for `mark_layout_ancestors_dirty()`:** The `RetainedWidgetTree` stores nodes in a slotmap; parent relationships may need to be derived from `WidgetNode.parent_id` or tracked separately. Validate during Phase C planning.
- **Template evaluator hot-path overhead:** The per-node snapshot diff adds a HashMap clone+compare per expression evaluation. Profile during Phase B to ensure <1% render time regression.
- **Fallback threshold tuning:** The `>50%` threshold is a heuristic. Profile on real surfaces (navigation-bar: ~80 nodes, audio-popover: ~40 nodes) during Phase C to determine optimal threshold.
- **Nested JSON field tracking:** Deferred past MVP. Flat payloads (`audio.percent`, `audio.muted`) cover current use cases. Document as a known limitation for v1.18.

## Sources

### Primary — MESH codebase (live, read in full)
- `crates/core/ui/elements/src/style/resolve.rs` — `StyleRuleIndex` (L187-296), state bit constants (L1038-1050), `restyle_subtree_cached` (L615-623), `inherit_retained_text_style` (L992-1009), `selector_index_key` (L1011-1036)
- `crates/core/ui/elements/src/tree.rs` — `WidgetNode` (L44-62), `NodeId` (L30), `ElementState` (L13-27)
- `crates/core/shell/src/shell/component.rs` — `ComponentDirtyFlags` (L67-79), `TREE_REBUILD` (L83-90), invalidation methods (L477-521), `take_dirty_for_paint` (L523-543)
- `crates/core/shell/src/shell/component/shell_component.rs` — `handle_service_event` (L119-182), `observes_service_event` (L184-208), `paint()` (L289-489)
- `crates/core/shell/src/shell/component/rendering.rs` — `finalize_tree()` (L169-276), `build_tree()` (L109-149)
- `crates/core/shell/src/shell/component/runtime_tree.rs` — `RetainedWidgetTree` (L83-91), `RetainedNodeDirtyFlags` (L70-79), `RetainedNodeSnapshot` (L184-190)
- `crates/core/shell/src/shell/runtime/service_state.rs` — `broadcast_service_event` (L4-33), `deliver_service_event` (L167-184)
- `crates/core/runtime/scripting/src/context/runtime.rs` — `ScriptContext` (L42-74), `tracked_service_fields` (L59), `tracked_service_fields_changed` (L336-351)
- `crates/core/runtime/scripting/src/context/proxy.rs` — service proxy `__index` tracking (L152-159)

### Secondary — reference architectures
- Stylo (Firefox CSS engine) — restyle hints approach for pseudo-class changes
- Svelte 5 runes — signal-based fine-grained reactivity model (conceptual influence only)
- MESH v1.3 benchmark scenarios — hover, open/close, slider, traversal, backend-update
- CSS cascade specification — inheritance behavior for color, font-family, font-size, font-weight, line-height
- Luau performance notes — metatable dispatch cost ~50-100ns

---
*Research completed: 2026-06-07*
*Ready for roadmap: yes*
