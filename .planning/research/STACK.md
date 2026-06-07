# Technology Stack: Typed Dependency Tracking (v1.18)

**Domain:** Smart invalidation for retained-mode shell framework
**Researched:** 2026-06-07
**Overall confidence:** HIGH

## Recommended Stack

MESH's typed dependency tracking is an internal pipeline upgrade. **No new crate dependencies are required.** All data structures and algorithms live within existing crates.

### Core Framework

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| Rust | 1.85 (current MESH baseline) | All dependency tracking data structures | Already in use; `HashMap`, `HashSet`, `Vec`, `u32` suffice |
| mlua (Luau mode) | 0.11+ | Script execution — read tracking via `__index` metatable | Existing; proxy metatable is the field-read tracking mechanism |

### Existing Crates Used (No New Dependencies)

| Crate | Role in v1.18 | Changes |
|-------|--------------|---------|
| `mesh-core-elements` | `StyleRuleIndex` extension — `RuleDependencyMask`, `state_to_rules` | Modified: `StyleRuleIndex` struct, `index_selector()`, `restyle_nodes_cached()` |
| `mesh-core-scripting` | Per-node read capture — `ServiceFieldReadSnapshot` | Modified: `ScriptContext`, proxy metatable (read tracking) |
| `mesh-core-shell` | Narrow invalidation, field-aware routing, retained tree dirty methods | Modified: `FrontendSurfaceComponent`, `RetainedWidgetTree`, `Shell` |
| `mesh-core-frontend` | Expression evaluator — per-node read snapshot | Modified: template expression evaluation in `build_tree_with_state` |

### New Data Structures

| Structure | Underlying Types | Crate |
|-----------|-----------------|-------|
| `RuleDependencyMask` | `u32` bitmask + `Option<String>` (tag, class) | `mesh-core-elements` |
| `NodeServiceFieldDependencies` | `HashMap<NodeId, HashSet<(String, String)>>` + reverse index | `mesh-core-shell` |
| `ServiceFieldReadSnapshot` | `HashMap<NodeId, HashSet<(String, String)>>` (per-frame temp) | `mesh-core-scripting` |

### Modified Structures

| Structure | Change | Rationale |
|-----------|--------|-----------|
| `StyleRuleIndex` | `+rule_dep_masks: Vec<RuleDependencyMask>` | Per-rule state dependency masks |
| `StyleRuleIndex` | `+state_to_rules: [Vec<usize>; 13]` | O(1) reverse lookup: state_bit → affected rules |
| `RetainedNodeDirtyFlags` | `+SERVICE_STATE = 1 << 6` | Distinct from interaction STATE |
| `RetainedWidgetTree` | `+mark_nodes_dirty()`, `mark_layout_ancestors_dirty()`, `nodes_with_flag()` | Per-node dirty marking without snapshot diff |
| `ScriptContext` | `+field_read_snapshot()` method | Capture per-node reads after render |
| `ScriptContext` | `+any_tracked_field_changed(service, fields)` | Fast check for field-aware routing |
| `StyleResolver` | `+restyle_nodes_cached(root, rules, ctx, index, node_ids)` | Per-node selective restyle |
| `FrontendSurfaceComponent` | `+node_service_deps: NodeServiceFieldDependencies` | Per-node field dependency cache |
| `ComponentDirtyFlags` | No change | Narrow invalidation uses per-node dirty, not component flags |

### Infrastructure (Unchanged)

| Component | Status | Notes |
|-----------|--------|-------|
| Profiling (`mesh_core_debug::ProfilingStage`) | Extended with `narrow_service`, `narrow_interaction` counts | Existing snapshots retain all fields |
| Diagnostics (`mesh_core_diagnostics`) | Unchanged — still runs, but on fewer nodes | Per-node restyle reduces diagnostic workload |
| Style rule cache (`cached_restyle_rules`, `cached_style_rule_index`) | Extended `StyleRuleIndex` still passes `is_for()` check | Pointer verification unchanged |
| Retained rendering pipeline (`RetainedDisplayList`, `RenderObjectTree`) | Narrow dirty nodes feed into existing incremental paint | Downstream unchanged |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| `fixedbitset` crate | Adds a dependency for ~15 lines of `Vec<u64>` bit operations | Hand-rolled `Vec<u64>` with `set_bit(idx)`, `get_bit(idx)`, `union_assign(&other)` — MESH's node count is small enough |
| `petgraph` for dependency graph | General-purpose graph is overengineered for "which nodes match which rules" | `HashMap` + `Vec` — purpose-built reverse index |
| `salsa` crate | Forces immutable database pattern incompatible with MESH's mutable widget tree | Salsa red-green *concept*, hand-rolled for MESH's types |
| `dashmap` / concurrent maps | All invalidation on single render thread | Standard `HashMap` with `RefCell` borrow (existing pattern) |
| Luau debug hooks for field tracking | Fires on every instruction — prohibitive cost | `__index` metatable proxy (already in use) |

## Sources

- MESH codebase: `crates/core/ui/elements/src/style/resolve.rs` — state bit constants (L1038-1050), `StyleRuleIndex` (L187-296)
- MESH codebase: `crates/core/runtime/scripting/src/context/proxy.rs` — service read tracking (L152-159)
- MESH codebase: `crates/core/shell/src/shell/component/runtime_tree.rs` — `RetainedWidgetTree`, `RetainedNodeDirtyFlags`
- MESH codebase: `crates/core/shell/src/shell/component.rs` — `ComponentDirtyFlags`
- Rust stdlib: `HashMap`, `HashSet` — all already in MESH Cargo.toml
