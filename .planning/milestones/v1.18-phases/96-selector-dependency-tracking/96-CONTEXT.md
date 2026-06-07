# Phase 96: Selector Dependency Tracking - Context

**Gathered:** 2026-06-07
**Status:** Ready for planning

<domain>
## Phase Boundary

Interaction state changes (`:hover`, `:focus`, `:active`) on the widget tree should restyle only the state-changed node and its descendants — not rebuild or re-traverse the entire widget tree. This phase builds the data structures (per-rule dependency masks, reverse state-bit → rule index) and wires them into the existing `finalize_tree` restyle path so narrow restyling produces visually identical output to the current full-tree restyle.

**Scope:** Data structure changes to `StyleRuleIndex`, `RetainedWidgetTree`/`RetainedNodeSnapshot`, and the `finalize_tree` restyle path. No new crate dependencies.

</domain>

<decisions>
## Implementation Decisions

### Data Design
- Add a new `state_to_rules: HashMap<u32, Vec<usize>>` field to `StyleRuleIndex` mapping individual state bits (e.g. `STATE_HOVERED = 1`, `STATE_FOCUSED = 2`, `STATE_ACTIVE = 4`) to the indices of rules that depend on that state.
- The existing `state: Vec<(u32, Vec<usize>)>` field stays for forward lookup (bitmask → rules, used in `for_each_candidate_rule`).
- Populate the reverse index during `StyleRuleIndex::new()` via `index_state_selector()`, storing each state bit's rules separately instead of only the combined bitmask.
- Provide a method `rules_for_state_bit(bit: u32) -> &[usize]` for O(1) reverse lookup.

### Change Tracking
- Augment `RetainedNodeSnapshot` to store `ElementState` directly instead of just a `state_hash: u64`.
- During `RetainedWidgetTree::update()`, diff old and new `ElementState` per-node, track which specific bits flipped (hover, focus, active).
- Publish changed state bits via `RetainedTreeDirtySummary` as a `changed_state_bits: u32` interaction bitmask.

### Propagation Scope
- When a node changes interaction state, restyle the changed node and all its descendants (entire subtree).
- Children inherit color, font-family, font-size, font-weight, line-height from their parent. If the parent's style changes due to state, children must recompute inherited values regardless of whether they have state-dependent rules themselves.
- Sibling nodes and cousins are NOT restyled — that's the optimization vs. the current full-tree approach.

### Integration
- Keep `finalize_tree()` as the single entry point for restyle.
- When `trigger_kind = "restyle"` and the dirty flags contain only `STATE` (no `SCRIPT | TEXT`), compute the set of affected NodeIds from: the state-changed node itself + its descendant keys.
- Call `restyle_subtree_for_keys_with_index()` with the computed target keys and the existing `StyleRuleIndex`.
- Fall back to full `restyle_subtree_cached()` when: `changed_state_bits` is empty (first frame), or dirty flags contain anything beyond `STATE`.

### OpenCode's Discretion
All implementation choices are at OpenCode's discretion within the boundaries defined above. Use existing codebase conventions for HashMap usage, error handling, and test patterns.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `StyleRuleIndex` (`crates/core/ui/elements/src/style/resolve.rs:187`) — existing struct with `tag`, `class`, `id`, `state`, `fallback` buckets. Methods: `new()`, `index_selector()`, `index_state_selector()`, `for_each_candidate_rule()`, `ensure_index()`.
- `StateSelector` enum and state bit constants (`STATE_HOVERED`, `STATE_FOCUSED`, `STATE_ACTIVE`, etc.) already defined at lines 1038-1050.
- `restyle_subtree_for_keys()` (`resolve.rs:675`) — existing targeted restyle that only recomputes nodes whose `_mesh_key` is in `target_keys`. Uses the cached `StyleRuleIndex`.
- `RetainedWidgetTree` / `RetainedNodeSnapshot` / `RetainedTreeDirtySummary` (`runtime_tree.rs`) — existing retained tree with diff-based dirty tracking.
- `restyle_retained_tree()` and `finalize_tree()` (`rendering.rs:156-276`) — the restyle pipeline with trigger_kind detection.

### Established Patterns
- State bitmasks use `u32` with constants like `STATE_HOVERED = 1 << 0`.
- `HashMap<String, Vec<usize>>` is the existing pattern for tag/class/id bucket indices in `StyleRuleIndex`.
- `RetainedNodeSnapshot` uses fingerprint hashing for equality; adding `ElementState` directly means dropping the `state_hash` field.
- `SecondaryMap<RetainedNodeKey, ...>` is used for per-node dirty flags.
- `restyle_subtree_for_keys()` accepts `&HashSet<String>` (mesh keys) for targeted restyle.

### Integration Points
- `ComponentDirtyFlags::INTERACTION_RESTYLE` = `STATE | STYLE | LAYOUT | PAINT | ACCESSIBILITY | METRICS` — the broad dirty flag set on interaction change. May need to be narrowed to just `STATE` initially, with downstream flags set per-node during targeted restyle.
- The decision point in `paint()` at `shell_component.rs:313-332` — determines whether to use retained restyle or full rebuild.
- `module_restyle_rules()` in `rendering.rs:38-59` — builds the ordered rule list for restyle; already used by the cached restyle path.

</code_context>

<specifics>
## Specific Ideas

No specific requirements beyond the ROADMAP success criteria:
1. `:hover` transitions restyle only hovered node + descendants
2. `:focus`/`:active` produce identical visual output to full-tree restyle
3. Inherited style values propagate correctly from partially-restyled parent to children
4. Navigation bar and audio popover regression tests pass

Requirements SEL-01 through SEL-05 as defined in REQUIREMENTS.md.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

- Unified bitmask index array was considered but adds complexity not needed for ~13 state bits.
- Dedicated `InteractionChangeTracker` was considered but the retained tree already performs per-node diffing.
- Building data structures without wiring narrow restyle until Phase 98 was considered but contradicts SEL-03 success criterion.

</deferred>
