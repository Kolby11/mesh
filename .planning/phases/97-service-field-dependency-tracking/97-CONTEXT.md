# Phase 97: Service Field Dependency Tracking - Context

**Gathered:** 2026-06-09
**Status:** Ready for planning

<domain>
## Phase Boundary

During template rendering, the evaluator captures which `(service, field)` pairs each `WidgetNode` reads — e.g., node "root/0/2" reads `("audio", "percent")` from `{audio.percent}`. A bidirectional `NodeServiceFieldDependencies` index is built after each full tree pass, answering "which nodes read field X?" and "which fields does node Y read?" in O(1). Overhead must stay below 1% of total render pass time on shipped surfaces.

**Scope:** New `service_field_reads` field on `WidgetNode`, new `TrackingVariableStore` wrapper in `mesh-core-frontend` compiler, new `NodeServiceFieldDependencies` struct in `mesh-core-shell` component module, benchmark test in `crates/core/frontend/render/tests/paint_perf_scenarios.rs`. No new crate dependencies.

</domain>

<decisions>
## Implementation Decisions

### Tracking Hook Location
- Intercept per-node field reads via a `TrackingVariableStore` wrapper that wraps the existing `VariableStore` reference passed into `build_element_node()`. The wrapper records field accesses without changing `eval_path()` or any expression evaluation logic.
- Key recorded reads by `_mesh_key` path string (e.g. `"root/0/2"`) — the same key used by `stable_runtime_node_id()` to derive the stable `NodeId`, so the index can be built with correct keys after `annotate_runtime_tree()` runs.
- Record `(service_name, field_name)` as a split pair: for path `audio.percent`, record `("audio", "percent")`. Mirrors the `store.get(parts[0])` / `json_path(value, parts[1])` split already in `eval_path()`. Needed for field-level fan-out in Phase 98.
- Keep the existing component-level `tracked_service_fields` in `ScriptContext` in parallel — it covers Lua-side reads via proxy `__index`; the new per-node tracker covers template-side expression evaluation reads. They cover different access paths.

### Node Identity & Index Structure
- Add `service_field_reads: Vec<(String, String)>` as a new field on `WidgetNode` in `mesh-core-elements/src/tree.rs`. Populated during `build_element_node()` via the `TrackingVariableStore`; read during index construction after `annotate_runtime_tree()` sets stable NodeIds.
- `NodeServiceFieldDependencies` struct lives in `mesh-core-shell` component module — same crate as `RetainedWidgetTree` which it pairs with.
- Bidirectional index uses `NodeId` (u64) as the key type:
  - Forward: `HashMap<NodeId, HashSet<(String, String)>>` — "which (service, field) pairs does node Y read?"
  - Reverse: `HashMap<(String, String), HashSet<NodeId>>` — "which nodes read (service, field) X?"
- Rebuild from scratch after each full `build_tree()` pass. Incremental diffing is deferred to Phase 98 or later — full rebuild is correct, simple, and the hot path (targeted interaction restyle) skips it entirely.

### Integration & Lifecycle
- `NodeServiceFieldDependencies` stored as a field on `FrontendSurfaceComponent` alongside `retained_tree` — both are per-surface runtime state.
- Index is rebuilt only after full `build_tree()` calls. Skip during Phase 96 targeted interaction restyle (state changes don't alter which service fields nodes read, only their styles). This satisfies SRV-03: the targeted restyle path is the hot path.
- SRV-03 verified with a dedicated benchmark test asserting per-node tracking overhead is <1% of total render time, using the existing `paint_perf_scenarios.rs` test harness. Measure a baseline (build with no-op store) vs. tracking store, assert ratio ≤ 1.01×.

### Claude's Discretion
- Exact field filtering in `TrackingVariableStore::get()`: only record reads where the key contains `.` (dotted path) and the root segment matches a known service name prefix, or record all dotted reads and let the consumer filter. Claude can choose the simpler option.
- Whether `service_field_reads` is `Vec` or `SmallVec` / `HashSet` on `WidgetNode` — Vec is fine for most nodes which read 0-2 service fields.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `VariableStore` trait in `crates/core/ui/elements/src/lib.rs:48` — `fn get(&self, name: &str) -> Option<serde_json::Value>`. The `TrackingVariableStore` wraps this and intercepts calls.
- `eval_path()` in `crates/core/frontend/compiler/src/expr.rs:187` — calls `store.get(parts[0])` then `json_path(value, parts[1])` for dotted expressions. No changes needed; the tracker fires on the outer `get()` call.
- `build_widget_tree_from_component()` in `crates/core/frontend/compiler/src/render.rs:47` — top-level entry point; `state: Option<&dyn VariableStore>` is the parameter to wrap.
- `build_element_node()` in `render.rs:314` — where individual node attributes are evaluated from template bindings. Node `_mesh_key` is constructed here from `instance_key`.
- `annotate_runtime_tree()` in `runtime_tree.rs:527` — assigns `node.id = stable_runtime_node_id(&key)` and `node.attributes.insert("_mesh_key", key)`. This is where per-node reads get mapped to stable NodeIds for the index.
- `RetainedWidgetTree` in `runtime_tree.rs:89` — the model for `NodeServiceFieldDependencies` to follow (per-surface, stored on `FrontendSurfaceComponent`).
- `FrontendSurfaceComponent` struct in `component.rs:269` — owns `retained_tree: RetainedWidgetTree` at line 351; `node_service_field_deps` goes alongside it.
- `paint_perf_scenarios.rs` in `crates/core/frontend/render/tests/` — existing benchmark test file with `WidgetNode` tree construction helpers.

### Established Patterns
- `SecondaryMap<RetainedNodeKey, …>` pattern in retained tree for per-node data — `HashMap<NodeId, …>` is acceptable for the bidirectional index since NodeId is u64.
- `HashMap<String, HashSet<String>>` is the existing shape for `tracked_service_fields` (service → fields); the new reverse index uses `HashMap<(String, String), HashSet<NodeId>>`.
- `WidgetNode.attributes: BTreeMap<String, String>` is the existing per-node metadata store; the new `service_field_reads` is a typed Vec, not an attribute string.
- Tests in `#[cfg(test)]` modules at the bottom of each source file.

### Integration Points
- `build_tree()` call site in `shell_component.rs:383` via `build_widget_tree_from_component()` — wrap the `VariableStore` here, collect per-node reads, build the index after annotation.
- `restyle_retained_tree()` in `shell_component.rs:326` (targeted interaction restyle path) — does NOT call `build_widget_tree_from_component()`; skip index rebuild on this path.
- `annotate_runtime_tree()` call in `runtime_tree.rs:527` (within `update()`) — immediately after this, all NodeIds are stable; index construction runs here.

</code_context>

<specifics>
## Specific Ideas

No specific requirements beyond the ROADMAP success criteria:
1. Template evaluator records per-node (service, field) pairs during render (SRV-01)
2. Bidirectional `NodeServiceFieldDependencies` supports both query directions in O(1) (SRV-02)
3. Per-node field tracking overhead below 1% of total render pass time on shipped surfaces (SRV-03)

Requirements SRV-01 through SRV-03 as defined in REQUIREMENTS.md.

</specifics>

<deferred>
## Deferred Ideas

- Incremental diff of per-node reads (only update changed nodes) — deferred to Phase 98 or later; full rebuild is correct and sufficient since the hot path already skips it.
- Tracking Lua-side service reads at per-node granularity — the existing `tracked_service_fields` in `ScriptContext` handles Lua-side reads; unifying with template-side reads is deferred (requires threading node context into Lua execution).
- `SmallVec` optimization for `service_field_reads` — micro-optimization; revisit if profiling shows alloc pressure.
- Filtering `TrackingVariableStore` to only record known service prefixes — adds complexity; record all dotted reads and let the index consumer handle filtering.

</deferred>
