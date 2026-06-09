# Phase 97: Service Field Dependency Tracking - Research

**Researched:** 2026-06-09
**Domain:** Rust compiler/runtime — WidgetNode tree construction, VariableStore trait interception, bidirectional dependency indexing
**Confidence:** HIGH

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Tracking Hook Location**
- Intercept per-node field reads via a `TrackingVariableStore` wrapper that wraps the existing `VariableStore` reference passed into `build_element_node()`. The wrapper records field accesses without changing `eval_path()` or any expression evaluation logic.
- Key recorded reads by `_mesh_key` path string (e.g. `"root/0/2"`) — the same key used by `stable_runtime_node_id()` to derive the stable `NodeId`, so the index can be built with correct keys after `annotate_runtime_tree()` runs.
- Record `(service_name, field_name)` as a split pair: for path `audio.percent`, record `("audio", "percent")`. Mirrors the `store.get(parts[0])` / `json_path(value, parts[1])` split already in `eval_path()`. Needed for field-level fan-out in Phase 98.
- Keep the existing component-level `tracked_service_fields` in `ScriptContext` in parallel — it covers Lua-side reads via proxy `__index`; the new per-node tracker covers template-side expression evaluation reads. They cover different access paths.

**Node Identity & Index Structure**
- Add `service_field_reads: Vec<(String, String)>` as a new field on `WidgetNode` in `mesh-core-elements/src/tree.rs`. Populated during `build_element_node()` via the `TrackingVariableStore`; read during index construction after `annotate_runtime_tree()` sets stable NodeIds.
- `NodeServiceFieldDependencies` struct lives in `mesh-core-shell` component module — same crate as `RetainedWidgetTree` which it pairs with.
- Bidirectional index uses `NodeId` (u64) as the key type:
  - Forward: `HashMap<NodeId, HashSet<(String, String)>>` — "which (service, field) pairs does node Y read?"
  - Reverse: `HashMap<(String, String), HashSet<NodeId>>` — "which nodes read (service, field) X?"
- Rebuild from scratch after each full `build_tree()` pass. Incremental diffing is deferred.

**Integration & Lifecycle**
- `NodeServiceFieldDependencies` stored as a field on `FrontendSurfaceComponent` alongside `retained_tree`.
- Index is rebuilt only after full `build_tree()` calls. Skip during Phase 96 targeted interaction restyle.
- SRV-03 verified with a dedicated benchmark test in `paint_perf_scenarios.rs`.

### Claude's Discretion
- Exact field filtering in `TrackingVariableStore::get()`: only record reads where the key contains `.` (dotted path) and the root segment matches a known service name prefix, or record all dotted reads and let the consumer filter. Claude can choose the simpler option.
- Whether `service_field_reads` is `Vec` or `SmallVec` / `HashSet` on `WidgetNode` — Vec is fine for most nodes which read 0-2 service fields.

### Deferred Ideas (OUT OF SCOPE)
- Incremental diff of per-node reads — deferred to Phase 98 or later.
- Tracking Lua-side service reads at per-node granularity.
- `SmallVec` optimization for `service_field_reads`.
- Filtering `TrackingVariableStore` to only record known service prefixes.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| SRV-01 | During render, the template evaluator records per-node service field reads ((service, field) pairs per NodeId). | `TrackingVariableStore` wraps the `VariableStore` passed through `build_tree_with_state`; intercepts `get()` calls with dotted paths during `parse_attributes` / `eval_expr` / `eval_path`. |
| SRV-02 | A bidirectional `NodeServiceFieldDependencies` index supports both "which nodes read field X?" and "which fields does node Y read?" queries in O(1). | `HashMap<NodeId, HashSet<(String,String)>>` forward + `HashMap<(String,String), HashSet<NodeId>>` reverse, built after `annotate_runtime_tree()` assigns stable NodeIds. |
| SRV-03 | Per-node field tracking overhead is below 1% of total render pass time on shipped surfaces. | Index rebuild skipped on the hot `restyle_retained_tree()` path; benchmark test in `paint_perf_scenarios.rs` asserts ratio ≤ 1.01×. |
</phase_requirements>

---

## Summary

Phase 97 instruments the template evaluator to capture which `(service, field)` pairs each `WidgetNode` reads during a full tree build pass. The mechanism is a `TrackingVariableStore` wrapper that intercepts `VariableStore::get()` calls with dotted paths (e.g., `audio.percent` → records `("audio", "percent")`). The accumulated reads are stored as `Vec<(String, String)>` on `WidgetNode.service_field_reads`, then a bidirectional `NodeServiceFieldDependencies` index is built after `annotate_runtime_tree()` assigns stable NodeIds.

The design is deliberately minimal: no new crate dependencies, no changes to expression evaluation logic, no incremental diffing. The hot path (`restyle_retained_tree()`) skips index rebuild entirely — it handles only interaction state changes, which do not alter which service fields nodes read. This is the key mechanism that satisfies SRV-03.

Three coordination points exist between the new tracking machinery and existing code: (1) `build_tree_with_state()` in `mesh-core-frontend` needs to accept a wrapping point for the `VariableStore`, (2) `WidgetNode` in `mesh-core-elements` gains a new field, and (3) `FrontendSurfaceComponent` in `mesh-core-shell` gains `node_service_field_deps` alongside `retained_tree`. The benchmark verifies <1% overhead.

**Primary recommendation:** Implement `TrackingVariableStore` with `RefCell<Vec<(String, String)>>` interior mutability in `mesh-core-frontend/compiler`; reset and harvest per-node reads inside `build_element_node()`; build the bidirectional index once after `finalize_tree()` runs `annotate_runtime_tree()`.

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Service field read interception | `mesh-core-frontend` compiler | — | `TrackingVariableStore` lives here; wraps `VariableStore` in `build_widget_tree_from_component`; closest to `eval_path` / `parse_attributes` |
| Per-node read storage (`service_field_reads`) | `mesh-core-elements` WidgetNode | — | WidgetNode is the shared IR; adding the field here keeps it visible to both compiler and shell without a new crate dep |
| Bidirectional dependency index | `mesh-core-shell` component module | — | Same crate as `RetainedWidgetTree`; both are per-surface runtime state; index rebuild belongs where `build_tree` is called |
| Index lifecycle (rebuild vs. skip) | `mesh-core-shell` rendering.rs | — | `build_tree()` triggers rebuild; `restyle_retained_tree()` skips it — both live in `rendering.rs` |
| Benchmark verification | `mesh-core-render` tests | — | `paint_perf_scenarios.rs` is the established perf harness for this milestone |

---

## Standard Stack

### Core

No new packages. All data structures use Rust stdlib types already in `Cargo.toml`. [VERIFIED: CONTEXT.md locked decision, confirmed by codebase inspection — no external deps needed beyond `std::collections::HashMap/HashSet`]

| Type | Purpose | Location |
|------|---------|----------|
| `HashMap<NodeId, HashSet<(String, String)>>` | Forward index: node → fields | stdlib |
| `HashMap<(String, String), HashSet<NodeId>>` | Reverse index: field → nodes | stdlib |
| `RefCell<Vec<(String, String)>>` | Interior mutability for per-node read accumulation | stdlib |
| `Vec<(String, String)>` | `service_field_reads` on `WidgetNode` | stdlib |

## Package Legitimacy Audit

No external packages are added in this phase. Section not applicable. [VERIFIED: CONTEXT.md — "No new crate dependencies."]

---

## Architecture Patterns

### System Architecture Diagram

```
build_tree() [rendering.rs]
  └─ compiled.build_tree_with_state(state: Option<&dyn VariableStore>)
       └─ build_widget_node(state: Some(&tracking_store))
            └─ build_element_node(state: Some(&tracking_store))
                 ├─ parse_attributes() → eval_expr() → eval_path()
                 │    └─ tracking_store.get("audio")       ← intercepts "audio.percent" root
                 │         records ("audio","percent") into tracking_store.pending_reads
                 ├─ node.service_field_reads = tracking_store.take_reads()
                 └─ return node

finalize_tree() [rendering.rs]
  └─ annotate_runtime_tree()   ← assigns node.id = stable_runtime_node_id(key)
  └─ build_node_service_field_deps(root)
       └─ walks tree, reads node.service_field_reads + node.id
            → populates NodeServiceFieldDependencies { forward, reverse }
  └─ self.node_service_field_deps = deps   ← stored on FrontendSurfaceComponent

restyle_retained_tree() [rendering.rs]
  └─ finalize_tree() WITHOUT tree rebuild
       └─ SKIPS build_node_service_field_deps (no index rebuild on this path)
```

### Key Insight: Where Reads Actually Happen

`TrackingVariableStore::get()` is called from two paths:

1. **`eval_path()`** in `crates/core/frontend/compiler/src/expr.rs:194` — for dotted paths like `audio.percent`, calls `store.get("audio")` first, then `json_path()` on the result. The root segment `"audio"` is what gets intercepted. The field segment `"percent"` is extracted from the original expression by splitting on `.`.

2. **`parse_attributes()`** in `render.rs:563` — evaluates `AttributeValue::Binding` by calling `eval_expr(binding, store)` which eventually calls `eval_path`.

3. **`TemplateNode::Expr`** in `render.rs:164` — inline `{audio.percent}` expressions call `eval_expr` directly.

4. **`TemplateNode::For`** in `render.rs:226` — calls `store.get(&for_node.iterable)` to get the iterable array. This is NOT a service field read (no dot), so filtering on dotted paths naturally excludes it.

The `TrackingVariableStore` must accumulate reads across the entire `build_element_node` call, not just `eval_path`. This means `pending_reads` must be a `RefCell<Vec<...>>` on the wrapper struct, reset before `parse_attributes` / child builds.

### Critical Design Challenge: Per-Node Accumulation Scope

`build_element_node` calls `parse_attributes(state)` (node's own attributes) AND then recursively calls `build_widget_node` for each child with the same `state`. This means if `TrackingVariableStore` is shared across the entire tree, child reads would accumulate into the parent's slot.

**Solution:** `TrackingVariableStore` must be per-node — created fresh for each `build_element_node` call, wrapping the outer store. The reads captured are only those triggered by THIS node's attribute evaluation. Child nodes get their own `TrackingVariableStore` wrapping the same outer store.

This is directly supported by the architecture: `build_element_node` is called per-element, and `state` is passed by reference (`Option<&dyn VariableStore>`). Creating a new wrapper per call is zero-heap-cost beyond the `Vec` allocation (which starts empty for most nodes).

```rust
// In build_element_node:
let tracking = TrackingVariableStore::new(state);
let (classes, id, mut attributes, event_handlers) =
    parse_attributes(&element.attributes, Some(&tracking));
// ... resolve inline content with tracking too ...
node.service_field_reads = tracking.into_reads();

// Children use the OUTER state, not tracking (children own their own tracking)
node.children = element.children.iter().map(|child| {
    build_widget_node(child, ..., state, ...)  // original state, not tracking
}).collect();
```

### Recommended Project Structure

No new files needed beyond:
- `crates/core/ui/elements/src/tree.rs` — add `service_field_reads` field to `WidgetNode`
- `crates/core/frontend/compiler/src/render.rs` — add `TrackingVariableStore`, use per `build_element_node`
- `crates/core/shell/src/shell/component/runtime_tree.rs` — add `NodeServiceFieldDependencies` struct + `build_node_service_field_deps()` function
- `crates/core/shell/src/shell/component.rs` — add `node_service_field_deps` field to `FrontendSurfaceComponent`
- `crates/core/shell/src/shell/component/rendering.rs` — call `build_node_service_field_deps` after `annotate_runtime_tree` in `finalize_tree` (rebuild path only)
- `crates/core/frontend/render/tests/paint_perf_scenarios.rs` — add benchmark test

### Pattern 1: TrackingVariableStore

```rust
// Source: crates/core/frontend/compiler/src/render.rs (new code)
struct TrackingVariableStore<'a> {
    inner: &'a dyn mesh_core_elements::VariableStore,
    reads: RefCell<Vec<(String, String)>>,
}

impl<'a> TrackingVariableStore<'a> {
    fn new(inner: Option<&'a dyn mesh_core_elements::VariableStore>) -> Option<Self> {
        inner.map(|s| Self { inner: s, reads: RefCell::new(Vec::new()) })
    }

    fn into_reads(self) -> Vec<(String, String)> {
        self.reads.into_inner()
    }
}

impl mesh_core_elements::VariableStore for TrackingVariableStore<'_> {
    fn get(&self, name: &str) -> Option<serde_json::Value> {
        let result = self.inner.get(name);
        // Record only dotted reads — simple heuristic for service field detection.
        // "audio" is looked up as root; we record when the caller's context is a
        // dotted path. Since eval_path does splitn(2, '.') and calls get(parts[0]),
        // we record the root segment call unconditionally for dotted names only.
        // Filtering: only names that have already been split are the root calls.
        // We cannot know the original dotted expr here, so we track ALL get() calls
        // and let the index builder filter based on whether the name looks like a
        // service (present in the service payload). Claude's discretion: record all
        // get() calls unconditionally; filtering is the consumer's job (deferred).
        self.reads.borrow_mut().push((name.to_string(), String::new()));
        result
    }
    // ...delegate translate, keys
}
```

**Correction note:** The CONTEXT.md decision says to record `(service_name, field_name)` as a split pair, meaning `("audio", "percent")` not `("audio", "")`. But `eval_path()` calls `store.get("audio")` (the root), then separately calls `json_path(value, "percent")`. The `TrackingVariableStore` only sees `get("audio")`, not `get("audio.percent")`.

**Resolution:** The `TrackingVariableStore` must intercept at a higher level — either:
- (A) Wrap the entire `eval_path` call (requires changing `eval_path`'s signature — not allowed per decisions), OR
- (B) Use a different intercept point: intercept when `eval_expr` is called with a dotted expression, before `eval_path` splits it.

**Approach B is the correct one:** Create a public `eval_expr_tracked()` variant or make `TrackingVariableStore::get()` smart:

When `eval_path` is called with `"audio.percent"`, it first calls `store.get("audio.percent")` — returns `None` for most service structs, then falls through to the split path. The `TrackingVariableStore` can intercept this full-path call:

```rust
fn get(&self, name: &str) -> Option<serde_json::Value> {
    let result = self.inner.get(name);
    // When name contains '.', record it as (root, rest) pair.
    // This fires on the store.get(expr) attempt in eval_path:187 before the split.
    if let Some(dot) = name.find('.') {
        let service = &name[..dot];
        let field = &name[dot+1..];
        self.reads.borrow_mut().push((service.to_string(), field.to_string()));
    }
    result
}
```

BUT: `eval_path` first tries `store.get(expr)` with the full `"audio.percent"` string. If that returns `None`, it splits and tries `store.get("audio")`. The `TrackingVariableStore` intercepts the full-path call. This is the exact right intercept point — it fires once per dotted expression, and the name is the full path `"audio.percent"` allowing a clean split.

**This is the implementation approach.** [ASSUMED — based on code reading of eval_path:187-198; confirmed by tracing the get() call sequence]

### Pattern 2: NodeServiceFieldDependencies

```rust
// Source: crates/core/shell/src/shell/component/runtime_tree.rs (new code)
#[derive(Debug, Default)]
pub(super) struct NodeServiceFieldDependencies {
    /// Forward: node → set of (service, field) pairs it reads.
    forward: HashMap<NodeId, HashSet<(String, String)>>,
    /// Reverse: (service, field) → set of nodes that read it.
    reverse: HashMap<(String, String), HashSet<NodeId>>,
}

impl NodeServiceFieldDependencies {
    pub(super) fn build(root: &WidgetNode) -> Self {
        let mut deps = Self::default();
        collect_node_deps(root, &mut deps);
        deps
    }

    pub(super) fn nodes_reading_field(&self, service: &str, field: &str) -> &HashSet<NodeId> {
        static EMPTY: std::sync::OnceLock<HashSet<NodeId>> = std::sync::OnceLock::new();
        let key = (service.to_string(), field.to_string());
        self.reverse.get(&key).unwrap_or_else(|| EMPTY.get_or_init(HashSet::new))
    }

    pub(super) fn fields_read_by_node(&self, node_id: NodeId) -> Option<&HashSet<(String, String)>> {
        self.forward.get(&node_id)
    }
}

fn collect_node_deps(node: &WidgetNode, deps: &mut NodeServiceFieldDependencies) {
    if !node.service_field_reads.is_empty() {
        let entry = deps.forward.entry(node.id).or_default();
        for pair in &node.service_field_reads {
            entry.insert(pair.clone());
            deps.reverse.entry(pair.clone()).or_default().insert(node.id);
        }
    }
    for child in &node.children {
        collect_node_deps(child, deps);
    }
}
```

### Pattern 3: Integration in finalize_tree (rebuild path only)

```rust
// In rendering.rs finalize_tree(), after annotate_runtime_tree() runs:
// (Only on the rebuild path — restyle path skips this block)
if trigger_kind == "rebuild" {
    self.node_service_field_deps = NodeServiceFieldDependencies::build(tree);
}
```

### Anti-Patterns to Avoid

- **Sharing one `TrackingVariableStore` across the whole tree:** Reads from child nodes would accumulate in the parent's per-node record. Each `build_element_node` call must create its own wrapper.
- **Rebuilding the index on `restyle_retained_tree`:** Interaction state changes don't alter which service fields nodes read. Rebuilding on the hot path wastes CPU and violates SRV-03.
- **Modifying `eval_path()` directly:** The decision explicitly says the wrapper intercepts without changing eval logic.
- **Recording `get("audio")` (root segment only):** `eval_path` calls `get` twice for `"audio.percent"` — first `get("audio.percent")` (returns None), then `get("audio")` (returns service object). Intercepting only the second call loses the field name. Intercepting the first call (full dotted string) is the correct approach.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Per-node read accumulation with interior mutability | Lock-free atomic or channel | `RefCell<Vec<...>>` | Single-threaded render path — no contention, zero overhead |
| Bidirectional index | Custom tree structure | `HashMap` + `HashSet` | Stdlib; O(1) amortized; sufficient for ~200 node trees |
| Key hashing | Custom hasher | Default `HashMap` hasher (`SipHash`) | Already used by `RetainedWidgetTree`; consistent |

---

## Common Pitfalls

### Pitfall 1: Double-recording dotted reads

**What goes wrong:** `eval_path` calls `store.get("audio.percent")` first (returns None from the inner store), then `store.get("audio")` second (returns the service object). If `TrackingVariableStore` fires on both calls, `service_field_reads` gets duplicate/incorrect entries.

**Why it happens:** `eval_path:187` tries the full expression first before splitting. Both calls go through `TrackingVariableStore::get()`.

**How to avoid:** Record only when the `name` argument contains a `.` (full dotted path call). The second call `get("audio")` (root only, no dot) should not be recorded. This produces exactly one `("audio", "percent")` entry per expression.

**Warning signs:** Node has duplicate `(service, field)` entries; or entries with empty field name.

### Pitfall 2: NodeId not yet assigned at read-capture time

**What goes wrong:** `build_element_node` runs before `annotate_runtime_tree`, which is where `node.id = stable_runtime_node_id(key)` is assigned. If the index is built immediately after `build_element_node`, all NodeIds will be the transient auto-increment IDs from `next_node_id()`, not the stable FNV-hash ids.

**Why it happens:** `annotate_runtime_tree` runs in `finalize_tree`, AFTER `build_tree_with_state`. The `service_field_reads` Vec is stored on the node, but the index must be built AFTER annotation.

**How to avoid:** Call `NodeServiceFieldDependencies::build(tree)` in `finalize_tree()` AFTER `annotate_runtime_tree()` returns. [VERIFIED: CONTEXT.md code context confirms this ordering]

**Warning signs:** Index queries return wrong or empty node sets; NodeIds in the index don't match those in `RetainedWidgetTree`.

### Pitfall 3: For-loop iterable calls recorded as service reads

**What goes wrong:** `TemplateNode::For` in `render.rs:226` calls `store.get(&for_node.iterable)` to get an array. If the iterable is named `items` (no dot), it won't be recorded. But if a template uses `{#for item in audio.tracks}` or similar dotted iterable, the dotted get fires and gets recorded as a service field read for the `column` container node — incorrect.

**Why it happens:** `build_widget_node` for `TemplateNode::For` calls `store.get()` with the iterable name, which may be dotted.

**How to avoid:** The `TrackingVariableStore` is not passed to the `For` path's `store.get()` call — it's only used inside `build_element_node`. `TemplateNode::For` is handled in `build_widget_node`, which uses the outer `state` directly. Only `build_element_node` wraps with `TrackingVariableStore`.

**Warning signs:** `column` nodes generated by `{#for}` have unexpected `service_field_reads` entries.

### Pitfall 4: Reads from inline text expressions (TemplateNode::Expr) missing

**What goes wrong:** `TemplateNode::Expr` in `render.rs:160-176` creates a `text` node and calls `eval_expr(&expr.expression, store)` using the outer `state`, not a `TrackingVariableStore`. Reads from inline `{audio.percent}` text nodes are not captured.

**Why it happens:** `TemplateNode::Expr` is handled in `build_widget_node` (not `build_element_node`), so the per-element wrapper doesn't apply to it.

**How to avoid:** `TemplateNode::Expr` must also create a `TrackingVariableStore` and assign reads to the `text` node it produces. This requires the same `TrackingVariableStore` pattern in the `Expr` arm of `build_widget_node`.

**Warning signs:** `{audio.percent}` inline text nodes have empty `service_field_reads`; service event doesn't trigger update for nodes using inline expressions.

---

## Code Examples

### How eval_path intercept works

```rust
// Source: crates/core/frontend/compiler/src/expr.rs:187-202 (existing)
fn eval_path(expr: &str, store: &dyn VariableStore) -> String {
    // FIRST CALL: get("audio.percent") -- this is what TrackingVariableStore sees
    if let Some(value) = store.get(expr) {   // <-- intercept here when expr has '.'
        return json_value_to_string(value);
    }
    let parts: Vec<&str> = expr.splitn(2, '.').collect();
    if parts.len() == 2 {
        // SECOND CALL: get("audio") -- root only, no dot, not recorded
        if let Some(root) = store.get(parts[0]) {
            if let Some(nested) = json_path(root, parts[1]) {
                return json_value_to_string(nested);
            }
        }
    }
    expr.to_string()
}

// TrackingVariableStore::get() implementation
fn get(&self, name: &str) -> Option<serde_json::Value> {
    let result = self.inner.get(name);
    if let Some(dot_pos) = name.find('.') {
        // Full dotted path — record (root, rest) pair
        let service = name[..dot_pos].to_string();
        let field = name[dot_pos + 1..].to_string();
        self.reads.borrow_mut().push((service, field));
    }
    // name without '.' is a bare variable lookup — not a service field read
    result
}
```

### How to harvest reads per-node

```rust
// In build_element_node, ONLY for this node's own attribute evaluation:
let tracking = TrackingVariableStore {
    inner: state.unwrap_or(&NoopStore),  // or keep as Option
    reads: RefCell::new(Vec::new()),
};
let (classes, id, mut attributes, event_handlers) =
    parse_attributes(&element.attributes, Some(&tracking as &dyn VariableStore));
// Also handle inline content (if text node with inline children):
// resolve_inline_content also calls eval_expr — pass tracking too
node.service_field_reads = tracking.reads.into_inner();

// Children get the ORIGINAL outer state — each builds its own tracking wrapper
node.children = element.children.iter().map(|child| {
    build_widget_node(child, ..., state, ...)  // outer state, not tracking
}).collect();
```

### Bidirectional index query

```rust
// "Which nodes read audio.percent?" — O(1) HashSet lookup
let affected = deps.nodes_reading_field("audio", "percent");

// "What does node 12345 read?" — O(1) HashMap lookup
let fields = deps.fields_read_by_node(12345);
```

### Benchmark test skeleton

```rust
// In paint_perf_scenarios.rs (new test)
#[test]
fn service_field_tracking_overhead_under_one_percent() {
    // Build baseline tree with no-op store
    let noop_start = std::time::Instant::now();
    let _baseline = build_tree_with_noop_state(100 /*nodes*/);
    let baseline_ns = noop_start.elapsed().as_nanos();

    // Build tree with TrackingVariableStore wrapping the noop store
    let tracking_start = std::time::Instant::now();
    let tree = build_tree_with_tracking_state(100 /*nodes*/);
    let tracking_ns = tracking_start.elapsed().as_nanos();

    // Assert tracking overhead < 1% of baseline
    let overhead_ratio = tracking_ns as f64 / baseline_ns as f64;
    assert!(
        overhead_ratio <= 1.01,
        "Tracking overhead {:.3}x exceeds 1% threshold",
        overhead_ratio - 1.0
    );

    // Also build the NodeServiceFieldDependencies index and assert correctness
    let deps = NodeServiceFieldDependencies::build(&tree);
    // ...correctness checks...
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Component-level `tracked_service_fields` (Lua proxy `__index`) | Per-node template-side tracking | Phase 97 | Enables node-granular invalidation instead of component-granular in Phase 98 |
| Full tree rebuild on any service event | Index-driven narrow invalidation | Phase 98 (consuming this) | Reduces dirty node count and retained-tree churn |

**Existing pattern to be aware of:** `tracked_service_fields` in `ScriptContext` (`crates/core/runtime/scripting/src/context/runtime.rs:59`) already tracks `HashMap<String, HashSet<String>>` (service → fields) via Lua proxy `__index`. Phase 97 adds the TEMPLATE side. Both coexist. Phase 98 will use the template-side index (`NodeServiceFieldDependencies`) for node-level fan-out.

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `eval_path` calls `store.get(full_dotted_expr)` first (e.g. `"audio.percent"`) before splitting — so `TrackingVariableStore::get()` intercepts the full path. | Code Examples, Pitfall 1 | If inner store returns a value for dotted keys, the split path never fires — tracking still works, but behavior differs from assumption. Low risk: service stores return `None` for dotted keys. |
| A2 | `TemplateNode::For` iterable names are never dotted in shipped surfaces, so the pitfall about `column` nodes recording false service reads is theoretical. | Common Pitfalls | If a frontend module uses dotted iterables, the `For` container node gets incorrect reads. But since `TrackingVariableStore` is only used in `build_element_node`, NOT in the `For` arm, this is a non-issue by construction. |
| A3 | `resolve_inline_content()` (for text nodes with inline children) is called WITHIN `build_element_node` after the tracking wrapper is created — so its `eval_expr` calls flow through the same `TrackingVariableStore`. | Common Pitfalls (Pitfall 4) | If inline content evaluation path does NOT go through the tracking store, inline `{audio.percent}` expressions in text elements are missed. Mitigated: pass tracking store to `resolve_inline_content` too. |

**If this table is empty for your case:** A3 is the most important assumption to verify at implementation time by tracing the exact call path for `<text>{audio.percent}</text>` templates.

---

## Open Questions

1. **Does `resolve_inline_content` need to receive the `TrackingVariableStore`?**
   - What we know: `build_element_node` handles the "text with inline children" case at lines 360-371 by calling `resolve_inline_content`. That function calls `eval_expr` with the outer `state`.
   - What's unclear: Whether this call should go through the per-node `TrackingVariableStore` or the outer `state`.
   - Recommendation: Yes — pass the `TrackingVariableStore` to `resolve_inline_content`. The reads are attributed to the same text node, so they belong in that node's `service_field_reads`.

2. **What happens to `TemplateNode::Expr` nodes (bare inline expressions not inside `<text>`)?**
   - What we know: `TemplateNode::Expr` creates a fresh `text` WidgetNode in `build_widget_node` at line 160-176. It calls `eval_expr` with the outer `state` — no tracking wrapper.
   - What's unclear: Whether these nodes need per-node tracking.
   - Recommendation: Yes — the `Expr` arm should also create a `TrackingVariableStore` and store reads in the produced `text` node's `service_field_reads`. This is a small, contained change in the `Expr` arm.

---

## Environment Availability

Step 2.6: SKIPPED (no external tool dependencies — pure Rust code/data structure changes within existing crates).

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` (cargo test) |
| Config file | none — inline `#[cfg(test)]` modules |
| Quick run command | `cargo test -p mesh-core-elements -- service_field` |
| Full suite command | `cargo test -p mesh-core-elements -p mesh-core-frontend -p mesh-core-shell -p mesh-core-render` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| SRV-01 | TrackingVariableStore records (service, field) pairs for dotted reads | unit | `cargo test -p mesh-core-frontend -- tracking` | No — Wave 0 |
| SRV-01 | WidgetNode.service_field_reads populated after build_element_node | unit | `cargo test -p mesh-core-frontend -- service_field_reads` | No — Wave 0 |
| SRV-01 | Expr nodes also get reads captured | unit | `cargo test -p mesh-core-frontend -- expr_node_tracking` | No — Wave 0 |
| SRV-02 | NodeServiceFieldDependencies forward query O(1) | unit | `cargo test -p mesh-core-shell -- node_service_field_deps` | No — Wave 0 |
| SRV-02 | NodeServiceFieldDependencies reverse query O(1) | unit | `cargo test -p mesh-core-shell -- node_service_field_deps` | No — Wave 0 |
| SRV-02 | Index correctly built after annotate_runtime_tree assigns stable ids | integration | `cargo test -p mesh-core-shell -- deps_stable_ids` | No — Wave 0 |
| SRV-03 | Tracking overhead < 1% of baseline build time | bench (test) | `cargo test -p mesh-core-render -- service_field_tracking_overhead` | No — Wave 0 |

### Sampling Rate

- **Per task commit:** `cargo test -p mesh-core-elements -p mesh-core-frontend`
- **Per wave merge:** `cargo test -p mesh-core-elements -p mesh-core-frontend -p mesh-core-shell -p mesh-core-render`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps

- [ ] Unit tests for `TrackingVariableStore` — covers SRV-01 (in `render.rs` `#[cfg(test)]`)
- [ ] Unit tests for `NodeServiceFieldDependencies` — covers SRV-02 (in `runtime_tree.rs` `#[cfg(test)]`)
- [ ] Benchmark test in `paint_perf_scenarios.rs` — covers SRV-03

---

## Security Domain

No security-sensitive surfaces introduced. This phase adds read-only instrumentation to template evaluation and a data structure for dependency tracking. No user input, auth, encryption, or external service calls involved. ASVS categories not applicable to this internal infrastructure change.

---

## Sources

### Primary (HIGH confidence)

- `crates/core/ui/elements/src/lib.rs` — `VariableStore` trait definition (line 48); `WidgetNode` struct in `tree.rs` (lines 44-62)
- `crates/core/frontend/compiler/src/expr.rs` — `eval_path()` implementation (lines 187-202); confirms `get(full_dotted_expr)` fires before split
- `crates/core/frontend/compiler/src/render.rs` — `build_element_node()` (line 306); `parse_attributes()` (line 538); `build_widget_node()` Expr arm (lines 160-176); For arm (lines 219-249)
- `crates/core/frontend/compiler/src/lib.rs` — `LayeredStore` pattern (lines 25-51); `build_tree_with_state()` signature (line 125)
- `crates/core/shell/src/shell/component/runtime_tree.rs` — `annotate_runtime_tree()` (line 527); key composition `format!("{key}/{index}")` (line 693); `stable_runtime_node_id()` (line 249)
- `crates/core/shell/src/shell/component.rs` — `FrontendSurfaceComponent` struct fields (lines 269-388); `retained_tree: RetainedWidgetTree` at line 351
- `crates/core/shell/src/shell/component/rendering.rs` — `build_tree()` (line 109); `restyle_retained_tree()` (line 156); `finalize_tree()` (line 169); `annotate_runtime_tree` call (line 178)
- `crates/core/runtime/scripting/src/context/runtime.rs` — `tracked_service_fields: Arc<Mutex<HashMap<String, HashSet<String>>>>` (line 59); confirms existing Lua-side tracking shape
- `crates/core/frontend/render/tests/paint_perf_scenarios.rs` — benchmark harness structure (lines 1-80)
- `.planning/phases/97-service-field-dependency-tracking/97-CONTEXT.md` — all locked decisions

### Secondary (MEDIUM confidence)

- Codebase pattern: `LayeredStore` wrapping `VariableStore` — confirms per-call wrapper struct pattern is established in this codebase

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — pure stdlib Rust types, all locked in CONTEXT.md
- Architecture: HIGH — verified against actual source files; key intercept mechanism confirmed by reading `eval_path` source
- Pitfalls: HIGH — directly derived from reading the implementation; not speculative
- Benchmark approach: HIGH — `paint_perf_scenarios.rs` exists and has a clear harness pattern

**Research date:** 2026-06-09
**Valid until:** 2026-07-09 (stable Rust code, no external dependencies)
