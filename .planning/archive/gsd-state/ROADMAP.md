# Roadmap: MESH

## Milestones

- ✅ **v1.20 Compositor Integration** — Phases 101-103.1 shipped 2026-06-18 ([archive](milestones/v1.20-ROADMAP.md), [audit](milestones/v1.20-MILESTONE-AUDIT.md))
- ✅ **v1.19 Performance: Event-Driven Frame Scheduler** — Phases 99-100 shipped 2026-06-09 ([archive](milestones/v1.19-ROADMAP.md), [audit](v1.19-MILESTONE-AUDIT.md))
- ✅ **v1.18 Performance: Smart Invalidation** — Phases 96-98 shipped 2026-06-09 ([archive](milestones/v1.18-ROADMAP.md), [audit](milestones/v1.18-MILESTONE-AUDIT.md))
- ✅ **v1.17 Performance: Scripting VM Consolidation** — Phases 92-95 shipped 2026-06-07 ([archive](milestones/v1.17-ROADMAP.md), [audit](v1.17-MILESTONE-AUDIT.md))
- ✅ **v1.16 Element Library** — Phases 86-91 shipped 2026-05-26 ([archive](milestones/v1.16-ROADMAP.md), [audit](milestones/v1.16-MILESTONE-AUDIT.md))
- ✅ **v1.15 Persistent Storage System** — Phases 81-85 shipped 2026-05-26 ([archive](milestones/v1.15-ROADMAP.md))
- ✅ **v1.14 Unified Luau Scripting Runtime** — Phases 74-80 shipped 2026-05-26 ([archive](milestones/v1.14-ROADMAP.md))
- ✅ **v1.13 Manifest I18n Contract** — Phases 70-73 shipped 2026-05-24 ([archive](milestones/v1.13-ROADMAP.md))
- ✅ **v1.12 Module Object Contract** — Phases 65-69 shipped 2026-05-23 ([archive](milestones/v1.12-ROADMAP.md))
- ✅ **v1.11 Surface Keybind Completion** — Phases 60-64 shipped 2026-05-23 ([archive](milestones/v1.11-ROADMAP.md))

---

## Phases

<details>
<summary>✅ v1.18 Performance: Smart Invalidation (Phases 96-98) — SHIPPED 2026-06-09</summary>

- [x] **Phase 96: Selector Dependency Tracking** — Per-rule state dependency masks and targeted interaction restyle at `StyleRuleIndex` (3/3 plans, completed 2026-06-07)
- [x] **Phase 97: Service Field Dependency Tracking** — Per-node service field read capture and bidirectional NodeId↔(service, field) index (3/3 plans, completed 2026-06-09)
- [x] **Phase 98: Narrow Invalidation & Event Routing** — Field-aware event routing, narrow dirty-node marking, and pixel-equivalence proof (3/3 plans, completed 2026-06-09)

</details>

<details>
<summary>✅ v1.19 Performance: Event-Driven Frame Scheduler (Phases 99-100) — SHIPPED 2026-06-09</summary>

- [x] **Phase 99: Event-Driven Wayland Dispatch** — Replace `std::thread::sleep` with `poll()` on Wayland fd, add eventfd IPC wakeup, and record scheduler profiling (4/4 plans, completed 2026-06-09)
- [x] **Phase 100: Opaque Region Hints** — Walk retained display list for opaque backgrounds and send `wl_surface::set_opaque_region` from the present path (2/2 plans, completed 2026-06-09)

</details>

<details>
<summary>✅ v1.20 Compositor Integration (Phases 101-103.1) — SHIPPED 2026-06-18</summary>

- [x] **Phase 101: Per-Region Damage** — Thread `Vec<DamageRect>` from the retained renderer through to `wl_surface::damage_buffer` calls, replacing the single unioned rect commit (completed 2026-06-10)
- [x] **Phase 102: HiDPI / Fractional Scale** — Wire `wl_output::scale` and `wp_fractional_scale_v1` as authoritative scale sources; allocate `PixelBuffer` at physical pixels; pair with `wp_viewporter` for non-integer ratios (completed 2026-06-10)
- [x] **Phase 103: Compositor Blur Offload** — Bind `org_kde_kwin_blur` optionally; send `kde_blur.set_region` + `kde_blur.commit` per surface with `backdrop-filter` nodes before each `wl_surface.commit` (completed 2026-06-17)
- [x] **Phase 103.1: Audit Gap Closure** — Fix CR-01 (blur region not cleared on backdrop-filter removal), CR-02 (negative coord cast in compute_blur_region), and DMGE-03 (damage_rect_count reports binary not count); add Phase 103 VERIFICATION.md and VALIDATION.md (INSERTED) (completed 2026-06-18)

</details>

### v1.21 Retained Layout & Display List (Phases 104-106)

- [ ] **Phase 104: Retained TaffyTree** — Per-surface `TaffyTree` and `_mesh_key → TaffyNodeId` map retained across frames; STYLE-only and LAYOUT-dirty nodes mutate in place; structural changes use `_mesh_key` as the stable identity key
- [ ] **Phase 105: Rope Display List** — `RopeNode` enum references existing `Arc<[DisplayPaintCommand]>` slices for clean subtrees; final flat-array assembly preserved; scroll offsets stored layout-relative
- [ ] **Phase 106: Per-Stage Budget Profiling** — `ProfilingStage::LayoutRetained` variant with profiling-gated acquisition; per-stage budget constants with `tracing::warn!` on overrun; baseline measurements across canonical workloads

## Phase Details

### Phase 104: Retained TaffyTree
**Goal**: Layout geometry is computed by mutating a per-surface `TaffyTree` in place rather than rebuilding a fresh tree on every pass, so only dirty layout nodes pay geometry recomputation cost
**Depends on**: Phase 103.1 (v1.20 fully shipped; no intra-v1.21 dependency)
**Requirements**: LAYOUT-01, LAYOUT-02, LAYOUT-03, LAYOUT-04, LAYOUT-05
**Success Criteria** (what must be TRUE):
  1. A frame that changes only a node's color (STYLE-only dirty) calls `taffy.set_style` on the affected node and skips `taffy.compute_layout` for all siblings — the full tree is not rebuilt
  2. A frame that adds or removes a widget uses `_mesh_key` (not `TaffyNodeId`) as the stable identity so retained geometry for unchanged siblings survives the structural change
  3. A subtree removal triggers a post-order descendant walk that removes all children before the parent node, leaving the `TaffyTree` in a valid state
  4. Layout output (x, y, width, height per node) is pixel-equivalent to the previous per-frame-rebuild approach across style-only, layout-dirty, and full tree-rebuild scenarios
  5. A `TREE_REBUILD` frame does not produce stale geometry — all layout rects reflect the current widget tree after the pass
**Plans**: 3 plans (2 waves)

Plans:
**Wave 1**
- [x] 104-01-PLAN.md — Foundation: PerSurfaceLayoutState struct, remove_taffy_subtree post-order helper, compute_incremental entry point (fresh-build / VISUAL_REPAINT / LAYOUT paths)

**Wave 2** *(blocked on Wave 1 completion)*
- [x] 104-02-PLAN.md — TREE_REBUILD structural diff keyed on _mesh_key plus 5 LAYOUT-05 parity tests
- [x] 104-03-PLAN.md — Shell wiring: layout_state field, compute_incremental call site in finalize_tree, theme/locale/source reset sites

### Phase 105: Rope Display List
**Goal**: The display list stores clean subtree command spans as shared references rather than copying bytes into parent vectors on each dirty update, reducing allocations on partial-dirty frames
**Depends on**: Phase 104 (correct `LayoutRect` values from retained Taffy are required for layout-relative scroll offset storage)
**Requirements**: ROPE-01, ROPE-02, ROPE-03
**Success Criteria** (what must be TRUE):
  1. A frame where a scrollable container's child is dirty reuses the unchanged sibling subtrees' `Arc<[DisplayPaintCommand]>` slices without cloning command bytes into the parent vector
  2. The final assembled contiguous paint command array is structurally identical to the pre-rope output — damage-rect queries and painter iteration produce the same results as before
  3. Scrolling a container with partially dirty children does not produce stale absolute coordinates — scroll offset is stored layout-relative in each rope segment and resolved at assembly time
**Plans**: TBD

### Phase 106: Per-Stage Budget Profiling
**Goal**: The profiling system tracks retained layout time per surface and emits observable warnings when individual stages exceed their defined budgets, giving concrete baseline measurements before and after the v1.21 retention changes
**Depends on**: Phase 105 (retained paths must exist before meaningful per-stage budget data can be captured)
**Requirements**: PERF-01, PERF-02, PERF-03
**Success Criteria** (what must be TRUE):
  1. The debug overlay shows a `LayoutRetained` stage timing entry alongside existing profiling stages when profiling is enabled
  2. In a debug build, a frame where layout retention exceeds its defined budget causes a `tracing::warn!` log line naming the stage, the measured duration, and the budget threshold — release builds emit nothing
  3. Profiling timer acquisition (`Instant::now()`) is guarded by the `profiling_enabled` flag so frames with profiling disabled pay zero clock cost for layout stage timing
  4. A captured baseline table shows measured durations for hover, backend update, slider drag, surface open, and clock tick workloads both before and after the Phase 104-105 retention changes
**Plans**: TBD

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 96. Selector Dependency Tracking | v1.18 | 3/3 | Complete | 2026-06-07 |
| 97. Service Field Dependency Tracking | v1.18 | 3/3 | Complete | 2026-06-09 |
| 98. Narrow Invalidation & Event Routing | v1.18 | 3/3 | Complete | 2026-06-09 |
| 99. Event-Driven Wayland Dispatch | v1.19 | 4/4 | Complete | 2026-06-09 |
| 100. Opaque Region Hints | v1.19 | 2/2 | Complete | 2026-06-09 |
| 101. Per-Region Damage | v1.20 | 1/1 | Complete | 2026-06-10 |
| 102. HiDPI / Fractional Scale | v1.20 | 2/2 | Complete | 2026-06-10 |
| 103. Compositor Blur Offload | v1.20 | 3/3 | Complete | 2026-06-17 |
| 103.1. Audit Gap Closure (INSERTED) | v1.20 | 1/1 | Complete | 2026-06-18 |
| 104. Retained TaffyTree | v1.21 | 3/3 | Gaps found | - |
| 105. Rope Display List | v1.21 | 0/? | Not started | - |
| 106. Per-Stage Budget Profiling | v1.21 | 0/? | Not started | - |

---

## Backlog

### v1.22 — Shell Features: Settings, Positioning, Popups

**Goal:** Fill the three remaining core shell interaction gaps.

**Scope:**

- **Settings module**: surface for managing installed modules, active providers,
  theme, and i18n — driven by manifest-declared settings schemas, not
  hardcoded Rust.

- **Positioning system**: make `position: relative / absolute / fixed` work in
  the layout and paint pipeline; needed for tooltips, dropdowns, and overlays.

- **Popup/overlay system**: provide a host API for frontend modules to create
  transient surfaces (tooltips, context menus, popovers) with custom content
  and dismiss behavior, backed by the layer-shell `overlay` layer.

**Priority:** medium — each blocks real authoring patterns today.

---

### v1.23 — Interned Identifier Types (Symbol/TagId)

**Goal:** Replace pervasive `String`/`Arc<str>` tag, class, attribute name,
interface name, service name, and module ID keys with a single global interner
so repeated allocations and per-node string comparisons become integer ops.

**Scope:**

- Introduce `Symbol` (or `TagId`) as the canonical interned identifier type
  using `lasso` or a hand-rolled interner.

- Apply intern types to tag matching, class lookups, attribute keys, service
  event names, interface names, and module IDs on hot paths.

- Add an allocator-level profile mode that counts allocations per render pass
  so remaining allocation hotspots can be ranked by actual frame cost.

**Priority:** medium — prerequisite for the broader typed node representation
and completes remaining "clone string" findings.

---

### v1.24 — Package Manager

Remote package registry, download, signature verification, dependency
resolution, and `mesh install / update / remove` CLI. Deferred until the
local module graph, interface contract, and LSP import semantics are stable.

---

### v1.25 — GPU Rendering

Replace the Skia CPU raster backend with a GPU-accelerated path (wgpu/Vulkan
via dmabuf linux-dmabuf-v1) for full Qt-parity performance on dense surfaces.
Deferred until retained layout, smart invalidation, and damage tracking are
shipped — uploading brand-new paint data every frame would waste most of the
GPU benefit.

---

### v1.26 — LSP / Extension Tooling

Language server protocol support for `.mesh` authoring: element/component
completion, hover, diagnostics, `require(...)` resolution, `ref.field`
completions derived from the element model, and IDE integration. Depends on
stable module-graph semantics.

---

### v1.27 — Performance: Incremental Tree Build & Retained Diff

**Goal:** Eliminate the remaining O(whole-tree) work the component runtime
does on every frame: rebuild only affected template subtrees, diff only dirty
nodes, and collapse the per-frame annotation passes into one walk.

Continuation of v1.18 (smart invalidation) and v1.19 (frame scheduler);
identified in the 2026-06-10 performance deep dive alongside the shipped
per-frame clone/parse batch (expression AST cache, `Arc<Theme>`,
`Arc<ScriptState>` snapshots, release LTO).

**Scope:**

- Affected-subtree template re-evaluation: `narrow_script_update` still
  rebuilds the full widget tree (full template evaluation) and diffs
  afterward, so every script/service change pays O(tree) tree-build cost.
  Use the `NodeServiceFieldDependencies` index from v1.18 to re-evaluate only
  template nodes whose tracked fields changed.

- Generation-aware retained-tree diff: `RetainedWidgetTree::update` walks the
  whole tree and FNV-hashes every node's computed style and attribute strings
  on every paint. Skip snapshot collection/hashing for subtrees whose dirty
  bits prove them clean since the last generation.

- Fuse the `finalize_tree` annotation walks (`annotate_runtime_tree`,
  `annotate_surface_shortcuts`, `annotate_overflow_tree`,
  `merge_runtime_primitive_defaults`, `annotate_selection_tree`) into a
  single traversal, and move hot annotations (`_mesh_key`, slider/scroll
  values, exiting flags) from string attributes toward typed `WidgetNode`
  fields — pairs with the v1.23 typed-node work.

**Priority:** medium-high — these are the last per-frame full-tree passes in
the component runtime; sequencing before v1.21/v1.25 keeps layout and GPU
work from being measured against an inflated baseline.

---

### Future: i18n Configuration UI

Full locale picker and per-language catalog management surface for end users.
The manifest i18n contract (v1.13) and scripting runtime (v1.14) are
prerequisites; the settings module (v1.22) provides the host surface.
