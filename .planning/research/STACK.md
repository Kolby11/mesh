# Stack Research

**Domain:** Retained Taffy layout tree + rope-style display list for Wayland shell framework
**Researched:** 2026-06-18
**Confidence:** HIGH

## Context

MESH already has: `taffy 0.10.1` in `workspace.dependencies`, `slotmap 1.1.1`, `Arc<[T]>` spans in
`RetainedPaintSubtree`, `ProfilingStage` enum with `std::time::Instant` in `mesh-core-debug`, and
per-node dirty bits in `RetainedWidgetTree`. This research covers only the NEW crate additions
needed for v1.21.

---

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `taffy` (existing) | `0.10.1` | Retained layout tree | Already installed. `TaffyTree::mark_dirty`, `set_style`, `add_child`, `remove_child`, `remove`, `insert_child_at_index` are all present in 0.10 and support in-place mutation without a rebuild. No version change needed — just retain the `TaffyTree` across frames instead of creating a new one on line 208 of `layout.rs`. |
| `rpds` | `1.2.0` | Persistent vector for rope-style display list spans | Trie-based persistent vector with O(log n) push/index and O(1) structural-sharing clone. The `Vector<Arc<[DisplayPaintCommand]>, RcK>` pattern lets clean subtree spans be referenced (not copied) by both old and new generations of the command buffer. Use `RcK` (not `ArcK`) since the display list is single-threaded. |
| `profiling` | `1.0.17` | Per-stage instrumentation with zero release overhead | Thin proc-macro façade over puffin/tracy/optick. With no backend feature flag enabled (the default), all `profiling::scope!` and `#[profiling::function]` calls compile to nothing. Adding named scopes to the five layout/paint stages measures wall time in debug/profiling builds without touching the release binary. |

### Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `slotmap` (existing) | `1.1.1` | Stable `TaffyNodeId` → `MeshNodeId` bidirectional map | Already used for `RetainedNodeKey`. Extend the existing `SecondaryMap` pattern to store `TaffyNodeId` per `RetainedNodeKey` so the retained tree can look up and mutate Taffy nodes by MESH key without rebuilding the map. |
| `Arc<[T]>` (std) | stdlib | Immutable command buffer sharing | Already used in `RetainedPaintSubtree.commands`. With rpds providing structural sharing for the index structure, `Arc<[DisplayPaintCommand]>` slices remain the leaf storage — no change needed there. |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| `puffin` (optional, dev only) | Frame-scoped visual profiler for `profiling` backend | Enable with `--features profile-with-puffin` in dev builds. `puffin::GlobalProfiler::lock().new_frame()` must be called once per shell frame. View with `puffin_viewer` over HTTP. Do NOT add to `workspace.dependencies` — keep it an opt-in dev dependency on `mesh-core-shell` only. |
| `cargo flamegraph` (external) | Sampling profiler for release builds | No code changes. Use `cargo flamegraph --bin mesh` to identify the hot path before and after the retained-tree change. Sampling is unaffected by whether `profiling` is enabled. |

---

## Installation

```toml
# workspace Cargo.toml — [workspace.dependencies]
rpds       = { version = "1.2.0", default-features = false, features = ["std"] }
profiling  = { version = "1.0.17", default-features = false }

# crates/core/ui/elements/Cargo.toml — [dependencies]
# (add alongside existing taffy dep)
rpds = { workspace = true }

# crates/core/shell/Cargo.toml — [dependencies]
profiling = { workspace = true }

# crates/core/shell/Cargo.toml — [dev-dependencies]  (optional, profiling backend)
# puffin = "0.21"   ← only when experimenting; do not commit enabled
```

**rpds feature notes:** The `std` feature is required for `HashMap`-backed hash maps. The `serde`
feature is optional and not needed here. `default-features = false` drops the `serde` dep.

**profiling notes:** With `default-features = false` and no `profile-with-*` feature flag, the
crate compiles to empty macros. The v1.21 milestone should NOT enable any profiling backend by
default — the goal is to instrument the code so that budget measurement is possible in a profiling
build, not to ship a profiler binary.

---

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| `rpds::Vector` (persistent trie) | `im::Vector` (RRB-tree) | `im` has slightly better random-index access (O(log₃₂ n) vs O(log₃₂ n) but better constants), but adds ~130 KB to binary size and pulls in `rand` as a dev-dependency. For a display list that only ever appends and clones spans (`push_back` + `clone`), rpds is sufficient and lighter. |
| `rpds::Vector` | Hand-rolled `Arc<Vec<T>>` + copy-on-write via `Arc::make_mut` | Simpler but forces a full `Vec` clone when any span in the list is dirty. The whole point of the rope structure is to avoid copying clean spans. `Arc<Vec<T>>` only helps if the entire list is clean. |
| `rpds::Vector` | `im-rc::Vector` (non-thread-safe) | `im-rc` uses `Rc` so it cannot cross an async boundary even if wrapped. MESH shell is single-threaded per surface but uses `tokio` for the event loop. `rpds` with `RcK` is equivalent performance without the `im-rc` thread-safety gap. |
| `profiling` (façade crate) | Direct `puffin` / direct `tracing` spans | Direct puffin couples all instrumentation to one backend. Direct `tracing` spans are already present for warn/debug logging; adding layout span timing on top of the existing `tracing` subscriber would mix profiling and logging concerns. `profiling` keeps them separate and costs nothing in the default build. |
| Retaining `TaffyTree` across frames (mutation) | Fresh `TaffyTree` per frame (current) | The current per-frame rebuild is the bottleneck. A fresh tree is simpler but rebuilds all node styles and re-runs full layout every frame even when zero nodes changed. Taffy's `mark_dirty` + `compute_layout` correctly skips already-clean subtrees when the tree is retained. |

---

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| `im::Vector` (the `im` crate) | Heavier binary (+130 KB), transitive `rand` dependency, and the RRB-tree advantage only matters for mid-vector insert/remove, which the display list never does. | `rpds::Vector` |
| `puffin` as a direct workspace dependency | Pulls in TCP server code into the binary. Must stay opt-in dev-only. | `profiling` façade with puffin as an optional dev feature |
| `anyrender` / `vello_encoding` for the display list store | These are existing feature-gated stubs for a future GPU backend. The display list store must stay backend-neutral; using Vello types in the span store would couple the retained structure to the GPU path prematurely. | `Arc<[DisplayPaintCommand]>` slices with `rpds::Vector` index |
| Expanding `ProfilingStage` enum for per-substage breakdown at this milestone | `ProfilingStage` already has `Layout`, `RetainedDisplayListUpdate`, `PaintTraversal`. Adding sub-stages now (e.g. `TaffyMutate`, `SpanDiff`) would bloat the debug UI without a budget baseline. Pin budgets first, sub-stage later. | New `ProfilingStage` variants only after baseline numbers exist |

---

## Integration Notes

### Retained TaffyTree per surface

The current bottleneck is in `mesh-core-elements/src/layout.rs` line 208:

```rust
let mut tree = TaffyTree::<NodeId>::new();  // rebuilt every frame
```

The retained tree should live on `FrontendSurfaceComponent` (in `mesh-core-shell`), not inside
`mesh-core-elements`. The layout function signature changes from a pure function to a method that
mutates the retained `TaffyTree`. The `RetainedWidgetTree` dirty summary already carries
`inserted`, `removed`, `children`, and `layout` counts — these map directly to which Taffy
operations to call:

- `dirty.inserted > 0` → `tree.new_leaf(style)` + `tree.add_child` for new nodes
- `dirty.removed > 0` → `tree.remove(taffy_node_id)` for removed MESH nodes
- `dirty.children > 0` → `tree.set_children` or `tree.insert_child_at_index`
- `dirty.layout > 0` → `tree.set_style(taffy_node_id, new_style)` (auto-calls `mark_dirty`)
- Structural clean frame → `tree.compute_layout` only (Taffy skips already-clean subtrees internally)

The `NodeId → TaffyNodeId` map must persist alongside the tree. The existing `slotmap`
`SecondaryMap<RetainedNodeKey, …>` pattern is the right place for this.

### Rope-style display list command store

`RetainedPaintSubtree` already stores `commands: Arc<[DisplayPaintCommand]>`. The "rope" upgrade
means the top-level `paint_commands: Arc<[DisplayPaintCommand]>` on `RetainedDisplayList` becomes
an `rpds::Vector<Arc<[DisplayPaintCommand]>>` where each element is one subtree's command slice.

On a dirty update, only the spans belonging to dirty subtrees are replaced; clean span `Arc`
pointers are shared by clone (zero-copy). The painter iterates the vector of slices instead of a
single flat slice, which changes the iteration API slightly but avoids the `extend_from_slice` copy
that `PaintSubtreeBuilder::append_child` currently performs on every frame.

### Per-stage budget profiling

The five stages to instrument are the ones already timed with `std::time::Instant` in
`shell_component.rs`: `Layout`, `RenderObjectSync`, `RetainedDisplayListUpdate`,
`PaintTraversal`, and `Paint`. Adding `profiling::scope!("layout")` inside each timed block lets
a developer attach puffin or Tracy to get a flame graph without changing the existing
`ProfilingStage` duration reporting.

---

## Version Compatibility

| Package | Compatible With | Notes |
|---------|-----------------|-------|
| `taffy 0.10.1` | Rust 1.85 | Already locked. `mark_dirty`, `set_style`, `add_child`, `remove_child`, `remove`, `insert_child_at_index` are all present. |
| `rpds 1.2.0` | Rust 1.65+ | No conflicts with existing deps. `RcK` reference type is safe for single-threaded use; switch to `ArcK` only if display list spans need to cross thread boundaries (not currently required). |
| `profiling 1.0.17` | Rust edition 2021+, MESH uses 2024 | Fully compatible. Zero transitive dependencies when no backend feature is enabled. |

---

## Sources

- `/dioxuslabs/taffy` (Context7) — `TaffyTree::mark_dirty`, `set_style`, `add_child`, `remove_child`, `remove`, `insert_child_at_index` verified against Taffy 0.10 API
- [rpds on lib.rs](https://lib.rs/crates/rpds) — version 1.2.0, persistent Vector with structural sharing confirmed
- [profiling on crates.io](https://crates.io/crates/profiling) — version 1.0.17, zero-overhead with no backend feature confirmed
- [profiling on GitHub](https://github.com/aclysma/profiling) — puffin/tracy backend activation model verified
- Codebase: `crates/core/ui/elements/src/layout.rs:208` — per-frame `TaffyTree::new()` bottleneck confirmed
- Codebase: `crates/core/frontend/render/src/display_list.rs` — `Arc<[DisplayPaintCommand]>` span storage confirmed
- Codebase: `crates/core/foundation/debug/src/lib.rs:357` — existing `ProfilingStage` enum confirmed
- Codebase: `crates/core/shell/src/shell/component/runtime_tree.rs` — `RetainedTreeDirtySummary` dirty categories confirmed

---
*Stack research for: v1.21 Retained Layout & Display List*
*Researched: 2026-06-18*
