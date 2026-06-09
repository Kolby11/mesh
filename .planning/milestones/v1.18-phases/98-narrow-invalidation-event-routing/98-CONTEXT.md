# Phase 98: Narrow Invalidation & Event Routing - Context

**Gathered:** 2026-06-09
**Status:** Ready for planning

<domain>
## Phase Boundary

Script state changes and service events should mark only the nodes that actually depend on the changed data. When a script variable changes, a tree diff identifies affected leaf nodes and their ancestor chains — no full TREE_REBUILD. When a service event arrives, the `NodeServiceFieldDependencies` reverse index determines whether any node in this component actually reads the changed fields; if none do, no invalidation occurs. A correctness-preserving >50% fallback threshold falls back to TREE_REBUILD whenever the affected set exceeds half the tree.

**Scope:** New `SCRIPT_NARROW` dirty flag in `ComponentDirtyFlags`; tree-diff logic in the rebuild path; field-level check in `handle_service_event()` using `NodeServiceFieldDependencies`; threshold guard; pixel equivalence tests in `tests/invalidation/`. No new crate dependencies.

</domain>

<decisions>
## Implementation Decisions

### Script State Narrow Invalidation
- Use **tree diff** approach: rebuild the WidgetNode tree normally, then compare old (retained tree) vs new tree values per node. Mark only nodes whose evaluated values changed as dirty leaf nodes — no new `script_var_reads` field on `WidgetNode` needed.
- Narrow path applies to **leaf text/value nodes only** — nodes where a changed variable maps to a text or attribute binding. Structural changes (conditionals, for-loops, component refs) continue to use TREE_REBUILD.
- Introduce a new **`SCRIPT_NARROW` `ComponentDirtyFlags` bit** that bypasses TREE_REBUILD. Set when tree diff reveals only leaf-level changes. Existing SCRIPT/TREE_REBUILD path used for structural changes.
- **Full ancestor chain to root** dirtied for any changed leaf node (layout/paint dirty propagated upward). Safe, no missed reflows.

### Service Event Fan-out Architecture
- Field-level filtering happens at **component level** inside `handle_service_event()`. Use the `NodeServiceFieldDependencies` reverse index to check whether changed fields intersect any node this component tracks.
- Changed fields extracted via **JSON key-level diff**: compare old cached payload vs new payload, collect changed keys as `(service, field)` pairs.
- The check uses **`NodeServiceFieldDependencies.nodes_reading_field()`** (per-node, Phase 97 reverse index) — more precise than component-level `tracked_service_fields`.
- When payload diff fields have **no entries in the reverse index**, skip `invalidate_script_state()` entirely. Component stays clean. (The Lua-side `tracked_service_fields` check in `tracked_service_fields_changed()` also remains, so Lua-side reads not captured in template still trigger invalidation as before.)

### Threshold Logic
- Threshold calculated as **affected_nodes / total_nodes** in the retained tree (all nodes, not just leaves — simpler, consistent with success criterion language).
- Threshold checked **before committing to narrow path**: compute affected set size, check ratio against 0.5, fall back early to TREE_REBUILD before any partial work begins.
- **Hardcode 0.5** — no config field, no premature generalization.

### Test Coverage
- Pixel equivalence via **FNV hash of PixelBuffer output**: hash pixel bytes after each render, compare baseline (full rebuild) vs narrow invalidation path hash. Must be equal.
- Equivalence tests live in **`crates/core/shell/src/shell/component/tests/invalidation/`** alongside existing profiling tests.
- Cover all 5 benchmark scenarios from success criteria: hover, open/close, slider, traversal, backend-update.

### Claude's Discretion
- Exact data structure for WidgetNode value comparison during tree diff (clone + compare vs fingerprint).
- Whether `SCRIPT_NARROW` shares the `restyle_retained_tree()` path or gets its own `narrow_script_update()` method.
- Profiling stage names and logging format for narrow invalidation events.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ComponentDirtyFlags` bitflags in `component.rs:67` — `TREE_REBUILD`, `INTERACTION_RESTYLE`, `SCRIPT_NARROW` will be added here.
- `NodeServiceFieldDependencies` in `runtime_tree.rs:721` — `nodes_reading_field(service, field) -> &HashSet<NodeId>` is the key reverse lookup method.
- `handle_service_event()` in `shell_component.rs:119` — add JSON key-level diff + NodeServiceFieldDependencies check here before calling `invalidate_script_state()`.
- `collect_interaction_changed_keys()` in `rendering.rs:329` — pattern for collecting affected key sets; tree diff for script changes follows the same pattern.
- `restyle_retained_tree()` in `rendering.rs:156` — retained-tree mutation entry point; `SCRIPT_NARROW` path can reuse or parallel this.
- `invalidate_script_state()` in `component.rs:503` — currently unconditionally sets TREE_REBUILD; new callers pass SCRIPT_NARROW instead when appropriate.
- `take_dirty_for_paint()` in `component.rs:533` — reads flags and routes to rebuild vs style-only path; needs updating for SCRIPT_NARROW routing.
- `paint_widget_tree()` / `build_tree()` in `rendering.rs` — where TREE_REBUILD is invoked; SCRIPT_NARROW bypasses this.
- Existing profiling tests in `crates/core/shell/src/shell/component/tests/invalidation/profiling.rs` — benchmark harness to extend for Phase 98 profiling payloads.

### Established Patterns
- `ComponentDirtyFlags` bits combine via bitflags; new `SCRIPT_NARROW` bit follows existing pattern.
- `affected_keys: HashSet<String>` used in interaction restyle path — same data type for script diff result.
- `cached_service_payloads: HashMap<String, serde_json::Value>` on component — old payload available for diff.
- `full_rebuild: bool` field in profiling snapshots — used in profiling output lines.
- Tests in `#[cfg(test)]` at bottom of source files; test helpers in `tests/` subdirectory.

### Integration Points
- `handle_service_event()` — where fan-out filtering is added (field-level check before invalidate call).
- `take_dirty_for_paint()` — where `SCRIPT_NARROW` is read to choose narrow vs rebuild path.
- `finalize_tree()` in `rendering.rs:169` — check point after rebuild where tree diff runs to determine SCRIPT_NARROW vs confirm full rebuild was needed.
- `annotate_runtime_tree()` — must complete before tree diff uses stable NodeIds.
- `RetainedWidgetTree::update()` — stores previous node snapshots; used as diff baseline.

</code_context>

<specifics>
## Specific Ideas

No specific requirements beyond the ROADMAP success criteria:
1. Simple text/value script state changes dirty only affected leaf nodes and their layout ancestor chain — not triggering TREE_REBUILD (INV-01)
2. Service events fan out only to components whose tracked field sets intersect the changed fields (INV-02)
3. >50% threshold activates TREE_REBUILD as correctness-preserving fallback (INV-03)
4. Profiling payloads show reduced dirty-node counts and retained-tree churn across all canonical benchmarks (INV-04)
5. All benchmark scenarios produce pixel-identical output compared to pre-invalidation baseline (INV-05)

Requirements INV-01 through INV-05 as defined in REQUIREMENTS.md.

</specifics>

<deferred>
## Deferred Ideas

- Shell-level ServiceEvent routing (filtering before components even see the event) — deferred; component-level filtering is sufficient for v1.18 scope.
- Per-node script variable read tracking (new WidgetNode field mirroring Phase 97's service_field_reads) — deferred; tree diff is simpler and correct.
- Configurable threshold value in ShellSettings — deferred; hardcode 0.5 per success criterion.
- Incremental tree diff (only diff changed subtrees) — deferred; full diff is correct and fast enough given the retained tree is shallow in practice.
- Unifying Lua-side and template-side service field read tracking — deferred to a future phase.

</deferred>
