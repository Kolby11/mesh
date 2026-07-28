# Phase 98: Narrow Invalidation & Event Routing - Research

**Researched:** 2026-06-09
**Domain:** Rust invalidation pipeline, dirty-flag routing, retained widget tree diff
**Confidence:** HIGH

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Script State Narrow Invalidation**
- Use **tree diff** approach: rebuild the WidgetNode tree normally, then compare old (retained tree) vs new tree values per node. Mark only nodes whose evaluated values changed as dirty leaf nodes — no new `script_var_reads` field on `WidgetNode` needed.
- Narrow path applies to **leaf text/value nodes only** — nodes where a changed variable maps to a text or attribute binding. Structural changes (conditionals, for-loops, component refs) continue to use TREE_REBUILD.
- Introduce a new **`SCRIPT_NARROW` `ComponentDirtyFlags` bit** that bypasses TREE_REBUILD. Set when tree diff reveals only leaf-level changes. Existing SCRIPT/TREE_REBUILD path used for structural changes.
- **Full ancestor chain to root** dirtied for any changed leaf node (layout/paint dirty propagated upward). Safe, no missed reflows.

**Service Event Fan-out Architecture**
- Field-level filtering happens at **component level** inside `handle_service_event()`. Use the `NodeServiceFieldDependencies` reverse index to check whether changed fields intersect any node this component tracks.
- Changed fields extracted via **JSON key-level diff**: compare old cached payload vs new payload, collect changed keys as `(service, field)` pairs.
- The check uses **`NodeServiceFieldDependencies.nodes_reading_field()`** (per-node, Phase 97 reverse index) — more precise than component-level `tracked_service_fields`.
- When payload diff fields have **no entries in the reverse index**, skip `invalidate_script_state()` entirely. Component stays clean. (The Lua-side `tracked_service_fields` check in `tracked_service_fields_changed()` also remains.)

**Threshold Logic**
- Threshold calculated as **affected_nodes / total_nodes** in the retained tree (all nodes, not just leaves).
- Threshold checked **before committing to narrow path**: compute affected set size, check ratio against 0.5, fall back early to TREE_REBUILD before any partial work begins.
- **Hardcode 0.5** — no config field.

**Test Coverage**
- Pixel equivalence via **FNV hash of PixelBuffer output**: hash pixel bytes after each render, compare baseline (full rebuild) vs narrow invalidation path hash. Must be equal.
- Equivalence tests live in **`crates/core/shell/src/shell/component/tests/invalidation/`** alongside existing profiling tests.
- Cover all 5 benchmark scenarios: hover, open/close, slider, traversal, backend-update.

### Claude's Discretion
- Exact data structure for WidgetNode value comparison during tree diff (clone + compare vs fingerprint).
- Whether `SCRIPT_NARROW` shares the `restyle_retained_tree()` path or gets its own `narrow_script_update()` method.
- Profiling stage names and logging format for narrow invalidation events.

### Deferred Ideas (OUT OF SCOPE)
- Shell-level ServiceEvent routing (filtering before components even see the event).
- Per-node script variable read tracking (new WidgetNode field mirroring Phase 97's service_field_reads).
- Configurable threshold value in ShellSettings.
- Incremental tree diff (only diff changed subtrees).
- Unifying Lua-side and template-side service field read tracking.
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| INV-01 | Simple text/value script state changes dirty only the affected leaf nodes plus their layout ancestor chain, not `TREE_REBUILD`. | Tree diff using `RetainedNodeSnapshot.diff_flags()` after `build_tree()`; new `SCRIPT_NARROW` bit routes away from TREE_REBUILD path in `take_dirty_for_paint()`. |
| INV-02 | Service events fan out only to components whose tracked field sets intersect the changed fields. | JSON key-level diff of `cached_service_payloads` vs new payload; `NodeServiceFieldDependencies.nodes_reading_field()` reverse lookup to check intersection before `invalidate_script_state()`. |
| INV-03 | `TREE_REBUILD` fallback activates when >50% of nodes are affected, preserving correctness for bulk changes. | `WidgetNode::node_count()` exists; affected set size checked before committing to narrow path; fallback by not setting `SCRIPT_NARROW`. |
| INV-04 | Profiling payloads show reduced dirty-node counts and retained-tree churn across canonical benchmarks. | Extend existing `profiling.rs` test file; `ProfilingInvalidationSnapshot.full_rebuild` already tracked; add `narrow_path: bool` and `affected_node_count: u64` fields to `ProfilingInvalidationSnapshot`. |
| INV-05 | Pixel-identical output on all benchmark scenarios (equivalence testing against pre-invalidation baseline rendering). | `PixelBuffer.data: Vec<u8>` accessible; FNV hash over `buffer.data` bytes is deterministic; test harness follows existing patterns in `tests/invalidation/`. |
</phase_requirements>

---

## Summary

Phase 98 adds narrow invalidation routing to the MESH shell component pipeline. The current pipeline always sets `TREE_REBUILD` in `invalidate_script_state()`, which forces a full Luau tree rebuild even when only a leaf text node changed (e.g. a volume label update). Phase 98 introduces a `SCRIPT_NARROW` dirty flag that bypasses full rebuild for pure leaf-level value changes, and adds field-level filtering to `handle_service_event()` so components that do not read any changed service fields skip invalidation entirely.

The implementation has three interlocking pieces: (1) a new `SCRIPT_NARROW` bit in `ComponentDirtyFlags` and corresponding routing in `take_dirty_for_paint()` and `paint()`; (2) tree diff logic that runs after a normal `build_tree()` call to identify which nodes changed and classify the change as leaf-only or structural; (3) a JSON payload diff in `handle_service_event()` that uses the `NodeServiceFieldDependencies` reverse index from Phase 97 to skip `invalidate_script_state()` when no intersecting fields changed.

All three pieces are pure within-crate changes in `mesh-core-shell`. No new crate dependencies. The correctness guarantee is pixel equivalence: the same `PixelBuffer` bytes must result whether the narrow or full path is taken, verified by FNV-hashing buffer contents in the five canonical benchmark scenarios.

**Primary recommendation:** Implement `SCRIPT_NARROW` as a flag that is set _after_ a normal `build_tree()` when the diff result is leaf-only, then reuse the existing `restyle_retained_tree()` fast-path entry point or add a parallel `narrow_script_update()` method — the key difference being that the tree is already rebuilt and the diff result is available.

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Dirty flag management | `component.rs` (ComponentDirtyFlags) | — | All flag bits, compound constants, and routing predicates live here. |
| Tree diff (leaf vs structural) | `rendering.rs` (finalize_tree / new narrow path) | `runtime_tree.rs` (RetainedNodeSnapshot diff) | `finalize_tree` is the post-build hook; `RetainedNodeSnapshot.diff_flags()` already computes per-node diffs. |
| Service event field filtering | `shell_component.rs` (handle_service_event) | `runtime_tree.rs` (NodeServiceFieldDependencies) | Event arrives in `handle_service_event`; reverse index is in `NodeServiceFieldDependencies`. |
| Threshold guard | `rendering.rs` (inside narrow path entry) | `tree.rs` (WidgetNode::node_count) | Threshold computed at the point the narrow path is chosen; `node_count()` already exists. |
| Paint routing (SCRIPT_NARROW branch) | `shell_component.rs` (paint) | `rendering.rs` (restyle_retained_tree or new method) | `paint()` reads `take_dirty_for_paint()` result and decides which path to call. |
| Profiling instrumentation | `rendering.rs` (record_profiling_stage) | `debug/src/lib.rs` (ProfilingInvalidationSnapshot) | Profiling hooks are already called at every path boundary in `rendering.rs`; snapshot struct is in the debug crate. |
| Pixel equivalence tests | `tests/invalidation/` | — | Follows established pattern; test helpers in `common.rs`. |

---

## Standard Stack

### Core (all [VERIFIED: codebase inspection])

This phase introduces no new libraries. All implementation uses existing in-tree types.

| Type | Location | Purpose |
|------|----------|---------|
| `ComponentDirtyFlags` | `component.rs:69` | Bitflags controlling invalidation routing |
| `RetainedNodeSnapshot` / `diff_flags()` | `runtime_tree.rs:189` | Per-node diff used for tree comparison |
| `NodeServiceFieldDependencies` | `runtime_tree.rs:721` | Reverse index: (service,field) → HashSet<NodeId> |
| `WidgetNode::node_count()` | `tree.rs:111` | Total node count for threshold calculation |
| `cached_service_payloads` | `component.rs:294` | `HashMap<String, Arc<serde_json::Value>>` — old payload available for diff |
| `serde_json::Value` | already a dependency | JSON key-level diff by iterating `as_object()` entries |
| `ProfilingInvalidationSnapshot` | `debug/src/lib.rs:174` | Extended with narrow-path fields |

### No New Dependencies

The phase explicitly prohibits new crate dependencies (from CONTEXT.md). `serde_json` diff via `as_object()` is sufficient for field-level comparison. FNV hashing for pixel equivalence tests uses the existing `RuntimeTreeHasher` pattern in `runtime_tree.rs:224` (hand-rolled FNV) rather than any new hash crate.

---

## Package Legitimacy Audit

> Not applicable — this phase adds no external packages.

---

## Architecture Patterns

### System Architecture Diagram

```
handle_service_event()
  |
  ├─ JSON diff: old cached_service_payloads vs new payload
  |   └─ collect changed (service, field) pairs
  |
  ├─ NodeServiceFieldDependencies.nodes_reading_field(service, field)
  |   ├─ No intersecting nodes? → skip invalidate_script_state() [INV-02]
  |   └─ Intersecting nodes exist → proceed
  |
  └─ invalidate_script_state() → sets SCRIPT_NARROW (not TREE_REBUILD)
       |
       ↓
paint()
  ├─ take_dirty_for_paint()
  |   ├─ SCRIPT_NARROW set? → requires_tree_rebuild=false, new narrow flag
  |   └─ SCRIPT set (structural)? → requires_tree_rebuild=true (existing path)
  |
  ├─ [SCRIPT_NARROW path] build_tree() normally
  |   ├─ tree diff: compare new tree vs last_tree (RetainedNodeSnapshot)
  |   ├─ collect changed leaf NodeIds
  |   ├─ structural change detected? → fallback to full rebuild
  |   ├─ affected_nodes / total_nodes > 0.5? → fallback [INV-03]
  |   └─ mark ancestor chains dirty, skip full style+layout on unaffected subtrees
  |
  └─ [TREE_REBUILD path] existing full build (unchanged)
       |
       ↓
retained_tree.update() → RetainedDisplayList.update_with_dirty_nodes()
  → PixelBuffer (pixel-identical output) [INV-05]
```

### Recommended File Changes

```
crates/core/shell/src/shell/
├── component.rs                     # Add SCRIPT_NARROW bit; update requires_tree_rebuild(); add invalidate_script_state_narrow()
├── component/
│   ├── rendering.rs                 # Tree diff logic; narrow_script_update() or extend finalize_tree(); threshold guard
│   ├── shell_component.rs           # JSON key diff in handle_service_event(); conditional invalidation
│   └── tests/
│       └── invalidation/
│           ├── basic.rs             # Unit tests for SCRIPT_NARROW routing, threshold guard, field filtering
│           └── profiling.rs         # 5 benchmark scenarios with narrow_path assertion and pixel equivalence
crates/core/foundation/debug/src/lib.rs   # Add narrow_path: bool + affected_node_count: u64 to ProfilingInvalidationSnapshot
```

### Pattern 1: SCRIPT_NARROW Dirty Flag

**What:** A new `ComponentDirtyFlags` bit that signals leaf-only script change — bypasses TREE_REBUILD routing without disabling the tree build itself.

**When to use:** Set by `invalidate_script_state()` when called from the narrow service event path. Cleared by `take_dirty_for_paint()`.

```rust
// Source: component.rs (existing bitflags block)
bitflags::bitflags! {
    pub(super) struct ComponentDirtyFlags: u16 {
        const SCRIPT        = 1 << 0;
        const STATE         = 1 << 1;
        // ... existing bits ...
        const SCRIPT_NARROW = 1 << 9;  // new: leaf-only script change
    }
}

impl ComponentDirtyFlags {
    pub(super) fn requires_tree_rebuild(self) -> bool {
        // SCRIPT_NARROW does NOT trigger TREE_REBUILD
        self.intersects(Self::SCRIPT | Self::TEXT)
        // Note: SCRIPT_NARROW is intentionally excluded here
    }
}
```

### Pattern 2: JSON Field-Level Diff in handle_service_event()

**What:** Before calling `invalidate_script_state()`, diff old and new payloads to extract changed (service, field) pairs, then check the reverse index.

**When to use:** Every service event in the narrow path. Falls through to full invalidation if no previous payload cached.

```rust
// Source: shell_component.rs handle_service_event() — conceptual pattern
fn collect_changed_fields(
    service: &str,
    previous: Option<&serde_json::Value>,
    next: &serde_json::Value,
) -> Vec<(String, String)> {
    let Some(prev_obj) = previous.and_then(|v| v.as_object()) else {
        // No previous — treat all fields as changed (conservative)
        return next.as_object()
            .map(|obj| obj.keys().map(|k| (service.to_string(), k.clone())).collect())
            .unwrap_or_default();
    };
    let next_obj = next.as_object().unwrap_or_default();  // serde_json's Map
    let mut changed = Vec::new();
    // Fields in next that differ from previous
    for (key, next_val) in next_obj {
        if prev_obj.get(key.as_str()) != Some(next_val) {
            changed.push((service.to_string(), key.clone()));
        }
    }
    // Fields removed in next (was in prev, not in next)
    for key in prev_obj.keys() {
        if !next_obj.contains_key(key.as_str()) {
            changed.push((service.to_string(), key.clone()));
        }
    }
    changed
}
```

### Pattern 3: Threshold Guard Before Narrow Path

**What:** After tree diff collects affected node IDs, check ratio before committing. `WidgetNode::node_count()` already exists.

```rust
// Source: rendering.rs — inside narrow_script_update() or finalize_tree narrow branch
let total_nodes = tree.node_count();
let affected_count = changed_leaf_ids.len();
if total_nodes == 0 || affected_count * 2 > total_nodes {
    // >50% affected — fall back to full TREE_REBUILD path
    return self.build_tree(theme, width, height);
}
```

### Pattern 4: Pixel Equivalence Test via FNV Hash

**What:** Hash the entire `PixelBuffer.data` slice with FNV-1a, compare baseline (full rebuild) vs narrow path. Same inputs must produce same hash.

```rust
// Source: tests/invalidation/profiling.rs — test helper pattern
fn fnv_hash_buffer(buffer: &PixelBuffer) -> u64 {
    const OFFSET: u64 = 14695981039346656037;
    const PRIME: u64 = 1099511628211;
    let mut hash = OFFSET;
    for byte in &buffer.data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

// In test: render twice (full rebuild, then narrow path), assert equal hash
let baseline_hash = fnv_hash_buffer(&baseline_buffer);
let narrow_hash = fnv_hash_buffer(&narrow_buffer);
assert_eq!(baseline_hash, narrow_hash, "pixel equivalence: {scenario}");
```

### Pattern 5: Tree Diff Using Existing RetainedNodeSnapshot Infrastructure

**What:** `RetainedNodeSnapshot` and `diff_flags()` already compute per-node diffs in `retained_tree.update()`. The narrow path reuses this machinery or adds a parallel structural-change detector.

**Key insight:** A structural change is indicated by `RetainedNodeDirtyFlags::CHILDREN` or `RetainedNodeDirtyFlags::INSERTED`/`REMOVED` — these signal that the tree shape changed, not just leaf values. Attribute or layout-only changes on leaf nodes are the narrow-eligible case.

```rust
// Source: runtime_tree.rs RetainedNodeDirtyFlags — existing flags
bitflags::bitflags! {
    pub(super) struct RetainedNodeDirtyFlags: u16 {
        const LAYOUT     = 1 << 0;
        const STYLE      = 1 << 1;
        const ATTRIBUTES = 1 << 2;
        const CHILDREN   = 1 << 3;  // structural: narrow path falls back
        const STATE      = 1 << 4;
        const INSERTED   = 1 << 5;  // structural: narrow path falls back
    }
}
```

### Anti-Patterns to Avoid

- **Setting SCRIPT_NARROW without rebuilding the tree first:** The narrow path still calls `build_tree()` — it defers to a post-diff classification, not a pre-build skip. Skipping the Luau tree build is a deferred optimization.
- **Diffing raw `WidgetNode` trees directly:** Use `RetainedNodeSnapshot` (layout fingerprint + style hash + attributes hash + child_ids) rather than comparing full `ComputedStyle` or attribute maps — the fingerprint is already hashed and cheap to compare.
- **Forgetting to propagate dirty up the ancestor chain:** Narrow path must mark not just the changed leaf but every ancestor to root for layout/paint dirty — the render engine expects a continuous dirty chain from leaf to root.
- **Skipping service field filtering when `cached_service_payloads` is absent:** First event for a service has no previous payload; must treat all fields as changed (conservative) to avoid silent misses.
- **Mutating `node_service_field_deps` during the narrow path:** `NodeServiceFieldDependencies` is rebuilt only after full `build_tree()` calls (`finalize_tree()` line 200-202). The narrow path reads the existing index; it does not rebuild it.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Per-node diff computation | Custom deep WidgetNode comparison | `RetainedNodeSnapshot.diff_flags()` in `runtime_tree.rs:200` | Already hashes layout, style, attributes, child_ids; O(1) comparison per node |
| Node count for threshold | Manual tree traversal | `WidgetNode::node_count()` in `tree.rs:111` | Already recursive; returns total subtree count |
| FNV hashing | Use `std::collections::DefaultHasher` | Inline FNV-1a (matches `RuntimeTreeHasher` pattern in `runtime_tree.rs:224`) | Deterministic cross-run; `DefaultHasher` is NOT stable across runs |
| Reverse field index lookup | Scan all nodes per field | `NodeServiceFieldDependencies.nodes_reading_field()` in `runtime_tree.rs:738` | O(1) hash lookup; Phase 97 built this specifically for this purpose |
| JSON field diff | Parse and reconstruct payloads | `serde_json::Value::as_object()` key comparison | Payload is already `Arc<serde_json::Value>`; object key iteration is direct |

**Key insight:** The retained tree infrastructure in `runtime_tree.rs` was built precisely to support this phase. `RetainedNodeSnapshot` + `diff_flags()` already produces the "which nodes changed and how" answer — the narrow path reuses this diff result rather than computing a separate one.

---

## Common Pitfalls

### Pitfall 1: CHILDREN Flag Missed in Structural Change Detection
**What goes wrong:** Checking only `ATTRIBUTES` and `LAYOUT` dirty flags without checking `CHILDREN` misclassifies a structural change (added/removed child, conditional branch flip) as a leaf change. The narrow path then produces an incorrect tree.
**Why it happens:** `RetainedNodeDirtyFlags::CHILDREN` is distinct from `ATTRIBUTES`; it is easy to write an "is leaf-only" predicate that checks layout/style/attributes but forgets children.
**How to avoid:** The "is structural" check must be `flags.intersects(CHILDREN | INSERTED)`. Any node with `CHILDREN` dirty forces full TREE_REBUILD fallback. Add an explicit test with a conditional-rendering component.
**Warning signs:** Test output has missing/extra nodes; pixel hash mismatch on conditional-render scenario.

### Pitfall 2: Ancestor Chain Dirtying Skipped
**What goes wrong:** Narrow path marks only the changed leaf node's NodeId as dirty. The retained display list renderer does not re-composite the parent box, causing the leaf's new pixel content to not appear in the final buffer.
**Why it happens:** The retained display list walks dirty node IDs and their immediate contexts. Parent nodes must also be in the dirty set for the dirty segment to be included in the paint pass.
**How to avoid:** After collecting changed leaf NodeIds, walk each leaf's ancestor chain in `last_tree` and add each ancestor's NodeId to the dirty set. Use `WidgetNode::find()` or the retained node key map to navigate upward.
**Warning signs:** Pixel equivalence test passes in isolation but fails when composited (parent background differs).

### Pitfall 3: node_service_field_deps Out of Date After Narrow Path
**What goes wrong:** A narrow service event path skips `build_tree()` entirely. On the next full service event, `node_service_field_deps` reflects the previous full build's tree, not the current narrow-updated tree. Field reads added by the narrow update are not indexed.
**Why it happens:** `node_service_field_deps` is rebuilt only inside `finalize_tree()` with `trigger_kind == "rebuild"` (line 200-202). The narrow path by design does not call `finalize_tree()` in rebuild mode.
**How to avoid:** Per the locked decision, the narrow path still calls `build_tree()` (Luau tree rebuild); it only skips full restyle+layout. So `finalize_tree()` runs in rebuild mode and `node_service_field_deps` is always up-to-date. Document this invariant in code comments.
**Warning signs:** Test where second service event after a narrow update fails to trigger narrow invalidation.

### Pitfall 4: FNV Hash Instability Across Allocations
**What goes wrong:** Using `std::collections::DefaultHasher` or a non-deterministic hasher for pixel equivalence produces different hashes across test runs even when pixel content is identical.
**Why it happens:** Rust's default `SipHash` randomizes the seed per process.
**How to avoid:** Use inline FNV-1a (same pattern as `RuntimeTreeHasher` in `runtime_tree.rs`). This is already the project pattern — no new dependency needed.
**Warning signs:** Pixel equivalence test is flaky — sometimes passes, sometimes fails with same input.

### Pitfall 5: Threshold Computed After Partial Work
**What goes wrong:** The threshold check happens after some mutation has already been applied (e.g., after restyle of changed nodes), then the fallback to TREE_REBUILD produces a double-mutation.
**Why it happens:** Ordering the threshold guard after the narrow work begins instead of before.
**How to avoid:** The locked decision explicitly states: "Threshold checked before committing to narrow path: compute affected set size, check ratio against 0.5, fall back early to TREE_REBUILD before any partial work begins." Run `node_count()` and `len(affected_set)` before any mutation.
**Warning signs:** Spurious pixel differences or crashes on large trees.

---

## Code Examples

### Verified: ComponentDirtyFlags current state (requires_tree_rebuild)
```rust
// Source: crates/core/shell/src/shell/component.rs:118 [VERIFIED: codebase inspection]
pub(super) fn requires_tree_rebuild(self) -> bool {
    self.intersects(Self::SCRIPT | Self::TEXT)
}
// SCRIPT_NARROW must NOT be added to this predicate.
```

### Verified: finalize_tree trigger for node_service_field_deps rebuild
```rust
// Source: crates/core/shell/src/shell/component/rendering.rs:200 [VERIFIED: codebase inspection]
if trigger_kind == "rebuild" {
    self.node_service_field_deps = NodeServiceFieldDependencies::build(tree);
}
// Narrow path calls build_tree() -> finalize_tree(trigger="rebuild"), so index stays fresh.
```

### Verified: handle_service_event invalidation call site
```rust
// Source: crates/core/shell/src/shell/component/shell_component.rs:177 [VERIFIED: codebase inspection]
if needs_rebuild {
    self.render_hooks_pending = true;
    self.invalidate_script_state();  // ← Phase 98: replace with narrow variant when eligible
}
```

### Verified: WidgetNode::node_count() for threshold
```rust
// Source: crates/core/ui/elements/src/tree.rs:111 [VERIFIED: codebase inspection]
pub fn node_count(&self) -> usize {
    1 + self.children.iter().map(|c| c.node_count()).sum::<usize>()
}
```

### Verified: cached_service_payloads field for old payload access
```rust
// Source: crates/core/shell/src/shell/component.rs:294 [VERIFIED: codebase inspection]
cached_service_payloads: HashMap<String, std::sync::Arc<serde_json::Value>>,
// Previous payload: self.cached_service_payloads.get(service_name)
// Updated at: shell_component.rs:134 (insert after receiving new payload)
// Note: insert happens BEFORE the diff check — save previous BEFORE the insert.
```

### Verified: nodes_reading_field() reverse index lookup
```rust
// Source: crates/core/shell/src/shell/component/runtime_tree.rs:738 [VERIFIED: codebase inspection]
pub(super) fn nodes_reading_field(&self, service: &str, field: &str) -> &HashSet<NodeId> {
    // Returns empty set (not None) when no nodes read this field.
    static EMPTY: std::sync::OnceLock<HashSet<NodeId>> = std::sync::OnceLock::new();
    ...
}
```

### Verified: RetainedNodeDirtyFlags for structural change detection
```rust
// Source: crates/core/shell/src/shell/component/runtime_tree.rs:76 [VERIFIED: codebase inspection]
pub(super) struct RetainedNodeDirtyFlags: u16 {
    const LAYOUT     = 1 << 0;
    const STYLE      = 1 << 1;
    const ATTRIBUTES = 1 << 2;
    const CHILDREN   = 1 << 3;  // structural — triggers TREE_REBUILD fallback
    const STATE      = 1 << 4;
    const INSERTED   = 1 << 5;  // structural — triggers TREE_REBUILD fallback
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Always TREE_REBUILD on script change | `SCRIPT_NARROW` bypasses for leaf-only changes | Phase 98 | Eliminates Luau re-evaluation for pure data changes |
| Always invalidate on any service event | Field-level filtering via reverse index | Phase 98 | Components reading only `audio.muted` not dirtied when only `audio.percent` changes |
| Single `invalidate_script_state()` entrypoint | Two entrypoints: full (structural) and narrow (leaf-only) | Phase 98 | Clean separation at call sites |

**Deprecated/outdated:**
- `invalidate_script_state()` always calling `TREE_REBUILD`: remains for structural changes but no longer the only invalidation path for service events.

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `PixelBuffer.data` is a `Vec<u8>` accessible in tests for FNV hashing | Code Examples / Tests | Need alternative approach to pixel equivalence if field is private or differently typed |
| A2 | `cached_service_payloads` insert at `shell_component.rs:134` happens before the diff check in `handle_service_event()` | Code Examples | Must save previous payload before the insert, or reorder to diff before inserting |

> A1: `buffer.data` is used in `buffer_pixel()` in `tests/common.rs:321` as `buffer.data[offset]`, confirming it is a public `Vec<u8>` — risk is LOW. [VERIFIED: codebase inspection]
>
> A2: Line 134 inserts the new payload; the old payload must be saved (`let previous = self.cached_service_payloads.get(service_name).cloned()`) before line 134. The CONTEXT.md code context notes "old payload available for diff" implying this ordering is expected. [ASSUMED]

---

## Open Questions

1. **Where does the narrow path read `last_tree` for diff baseline?**
   - What we know: `last_tree: Option<WidgetNode>` stores the previous frame's tree; `build_tree()` always produces a fresh tree. The narrow path needs to diff the freshly rebuilt tree against `last_tree` before storing it as the new `last_tree`.
   - What's unclear: Should the diff happen inside `finalize_tree()` (after rebuild) or in `paint()` (after `build_tree()` returns)?
   - Recommendation: Diff in `paint()` after `build_tree()` returns, before `retained_tree.update()` — this is the cleanest insertion point and avoids threading diff results through `finalize_tree()`. The planner can choose `finalize_tree()` instead if it simplifies result propagation.

2. **Does `SCRIPT_NARROW` need its own `invalidate_*` method or does `invalidate_script_state()` get a parameter?**
   - What we know: Current `invalidate_script_state()` always sets `TREE_REBUILD`. The CONTEXT.md says "new callers pass SCRIPT_NARROW instead when appropriate."
   - What's unclear: Whether to add `invalidate_script_state_narrow()` as a separate method or add a `narrow: bool` parameter.
   - Recommendation: Separate method (`invalidate_script_state_narrow()`) for clarity — the call site in `handle_service_event()` will conditionally call one or the other. Claude's discretion per CONTEXT.md.

3. **Should the narrow path skip calling `render_hooks_pending = true`?**
   - What we know: `render_hooks_pending = true` triggers `call_render_hooks()` at the top of `build_tree()`. If a service field changed, render hooks likely need to run to update script state.
   - What's unclear: Whether setting render hooks pending is required for the narrow path to produce correct pixel output.
   - Recommendation: Keep `render_hooks_pending = true` in the narrow path — the hook call is needed for scripts to react to the field change, and skipping it risks stale script state. The narrow path optimization is in the tree diff post-build, not in skipping the Luau execution.

---

## Environment Availability

> Step 2.6: SKIPPED (no external dependencies identified — pure Rust codebase changes, no new tools or services required)

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` + `#[cfg(test)]` |
| Config file | none (Cargo test runner) |
| Quick run command | `cargo test -p mesh-core-shell shell::component::tests::invalidation -- --nocapture 2>&1` |
| Full suite command | `cargo test -p mesh-core-shell 2>&1` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| INV-01 | SCRIPT_NARROW set when tree diff shows leaf-only change; TREE_REBUILD not triggered | unit | `cargo test -p mesh-core-shell script_narrow_set_for_leaf_only_change` | ❌ Wave 0 |
| INV-01 | Ancestor chain dirtied for changed leaf node | unit | `cargo test -p mesh-core-shell narrow_path_dirties_ancestor_chain` | ❌ Wave 0 |
| INV-01 | TREE_REBUILD triggered when structural change detected | unit | `cargo test -p mesh-core-shell structural_change_falls_back_to_tree_rebuild` | ❌ Wave 0 |
| INV-02 | `invalidate_script_state()` skipped when no nodes read changed fields | unit | `cargo test -p mesh-core-shell service_event_skipped_when_no_intersecting_fields` | ❌ Wave 0 |
| INV-02 | `invalidate_script_state()` called when changed fields intersect index | unit | `cargo test -p mesh-core-shell service_event_triggers_narrow_when_fields_intersect` | ❌ Wave 0 |
| INV-03 | TREE_REBUILD fallback when >50% nodes affected | unit | `cargo test -p mesh-core-shell threshold_fallback_exceeds_half` | ❌ Wave 0 |
| INV-03 | Narrow path taken when affected < 50% | unit | `cargo test -p mesh-core-shell threshold_narrow_below_half` | ❌ Wave 0 |
| INV-04 | Profiling snapshot shows narrow_path=true and reduced dirty counts on backend-update scenario | integration | `cargo test -p mesh-core-shell phase98_narrow_invalidation_reduces_churn -- --nocapture` | ❌ Wave 0 |
| INV-05 | Pixel-identical output: hover scenario | integration | `cargo test -p mesh-core-shell phase98_pixel_equivalence_hover -- --nocapture` | ❌ Wave 0 |
| INV-05 | Pixel-identical output: open/close scenario | integration | `cargo test -p mesh-core-shell phase98_pixel_equivalence_open_close -- --nocapture` | ❌ Wave 0 |
| INV-05 | Pixel-identical output: slider scenario | integration | `cargo test -p mesh-core-shell phase98_pixel_equivalence_slider -- --nocapture` | ❌ Wave 0 |
| INV-05 | Pixel-identical output: traversal scenario | integration | `cargo test -p mesh-core-shell phase98_pixel_equivalence_traversal -- --nocapture` | ❌ Wave 0 |
| INV-05 | Pixel-identical output: backend-update scenario | integration | `cargo test -p mesh-core-shell phase98_pixel_equivalence_backend_update -- --nocapture` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p mesh-core-shell shell::component::tests::invalidation -- --nocapture 2>&1`
- **Per wave merge:** `cargo test -p mesh-core-shell 2>&1`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
All test functions above are new and do not exist yet. The test files exist:
- `crates/core/shell/src/shell/component/tests/invalidation/basic.rs` — covers unit tests (INV-01 through INV-03)
- `crates/core/shell/src/shell/component/tests/invalidation/profiling.rs` — covers integration/profiling tests (INV-04, INV-05)

Both files exist and should receive additional test functions. No new test files need to be created.

---

## Security Domain

> Not applicable — this phase makes no changes to authentication, session management, input validation, cryptography, access control, or external data ingestion. It modifies only internal dirty-flag routing and in-memory tree diffing within the shell process.

---

## Sources

### Primary (HIGH confidence)
- Codebase inspection: `crates/core/shell/src/shell/component.rs` — ComponentDirtyFlags, invalidate_script_state(), take_dirty_for_paint()
- Codebase inspection: `crates/core/shell/src/shell/component/rendering.rs` — build_tree(), restyle_retained_tree(), finalize_tree(), collect_interaction_changed_keys()
- Codebase inspection: `crates/core/shell/src/shell/component/shell_component.rs` — handle_service_event(), paint()
- Codebase inspection: `crates/core/shell/src/shell/component/runtime_tree.rs` — RetainedWidgetTree, RetainedNodeSnapshot, diff_flags(), NodeServiceFieldDependencies, nodes_reading_field()
- Codebase inspection: `crates/core/ui/elements/src/tree.rs` — WidgetNode, node_count()
- Codebase inspection: `crates/core/foundation/debug/src/lib.rs` — ProfilingInvalidationSnapshot, ProfilingStage
- Codebase inspection: `crates/core/shell/src/shell/component/tests/invalidation/` — existing test structure and helpers

### Secondary (MEDIUM confidence)
- CONTEXT.md: locked decisions for implementation approach

### Tertiary (LOW confidence)
- None

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all types verified by direct codebase inspection
- Architecture: HIGH — all call sites, field names, and function signatures verified
- Pitfalls: HIGH — derived from code reading structural constraints and existing patterns
- Test map: HIGH — existing test files confirmed; new function names are prescriptive

**Research date:** 2026-06-09
**Valid until:** 2026-07-09 (stable Rust codebase; no external dependencies)
