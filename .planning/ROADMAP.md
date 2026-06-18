# Roadmap: MESH

## Milestones

- 🔄 **v1.20 Compositor Integration** — Phases 101-103 in progress
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

### v1.20 Compositor Integration

- [x] **Phase 101: Per-Region Damage** — Thread `Vec<DamageRect>` from the retained renderer through to `wl_surface::damage_buffer` calls, replacing the single unioned rect commit (completed 2026-06-10)
- [x] **Phase 102: HiDPI / Fractional Scale** — Wire `wl_output::scale` and `wp_fractional_scale_v1` as authoritative scale sources; allocate `PixelBuffer` at physical pixels; pair with `wp_viewporter` for non-integer ratios (completed 2026-06-10)
- [x] **Phase 103: Compositor Blur Offload** — Bind `org_kde_kwin_blur` optionally; send `kde_blur.set_region` + `kde_blur.commit` per surface with `backdrop-filter` nodes before each `wl_surface.commit` (completed 2026-06-17)
- [x] **Phase 103.1: Audit Gap Closure** — Fix CR-01 (blur region not cleared on backdrop-filter removal), CR-02 (negative coord cast in compute_blur_region), and DMGE-03 (damage_rect_count reports binary not count); add Phase 103 VERIFICATION.md and VALIDATION.md (INSERTED) (completed 2026-06-18)

## Phase Details

### Phase 101: Per-Region Damage
**Goal**: The compositor receives accurate per-dirty-rect damage information instead of a single unioned bounding rect on every frame commit
**Depends on**: Phase 100 (opaque region hints — retained display list already in place)
**Requirements**: DMGE-01, DMGE-02, DMGE-03
**Success Criteria** (what must be TRUE):
  1. A frame where only one widget changes causes `wl_surface::damage_buffer` to be called once with the widget's rect, not the full surface bounds
  2. A frame with multiple dirty regions calls `damage_buffer` once per rect, capped at 16 calls total per commit
  3. Debug overlay shows a per-frame damage rect count alongside existing damage metrics
**Plans**: 1/1 done
**Plans**:
- [x] 101-01-PLAN.md — Per-region damage: Vec<DamageRect> end-to-end through wl_surface::damage_buffer

### Phase 102: HiDPI / Fractional Scale
**Goal**: Shell surfaces render at native physical pixel density on HiDPI displays; layout coordinates stay in logical CSS pixels throughout
**Depends on**: Phase 101 (damage rects must be correctly scaled before commit)
**Requirements**: HDPI-01, HDPI-02, HDPI-03, HDPI-04, HDPI-05
**Success Criteria** (what must be TRUE):
  1. On a 2× integer-scale display, text and icons appear sharp without upscaling artifacts
  2. On a 1.5× fractional-scale display, `wp_viewporter` sets the destination to logical dimensions and the buffer is allocated at `ceil(logical × 1.5)` physical pixels
  3. Plugging in or unplugging a HiDPI monitor (scale factor change) triggers a resize and full redraw with no stale pixels visible
  4. On a compositor without `wp_fractional_scale_v1`, the `wl_output::scale` integer fallback keeps rendering correct
 **Plans**:
 - [x] 102-01-PLAN.md — Scale acquisition: bind wp_viewporter + wp_fractional_scale_v1, store scale: f32 on SurfaceEntry, implement scale handlers
 - [x] 102-02-PLAN.md — Physical pixel pipeline: PixelBuffer at ceil(logical×scale), wp_viewporter integration, damage rect scaling
 **UI hint**: yes

### Phase 103: Compositor Blur Offload
**Goal**: Surfaces with `backdrop-filter: blur(...)` nodes delegate blur rendering to the compositor on KDE; non-KDE compositors render a flat background without error
**Depends on**: Phase 102 (scale factor must be established before blur region coordinates are correct)
**Requirements**: BLUR-01, BLUR-02, BLUR-03, BLUR-04
**Success Criteria** (what must be TRUE):
  1. On KDE Plasma, a surface with `backdrop-filter: blur(8px)` shows compositor-driven background blur behind the affected region
  2. On a non-KDE compositor (e.g., Sway), the same surface starts and renders normally with a flat background and no Wayland protocol errors in logs
  3. A surface with no `backdrop-filter` nodes produces no `kde_blur` protocol calls during its commit sequence
  4. Removing the CPU software blur path does not regress any existing test or visual output
**Plans**: 3 plans

Plans:
- [x] 103-01-PLAN.md — Protocol infrastructure: wayland-protocols-plasma dep, KdeBlur state, optional global binding, commit-time protocol calls
- [x] 103-02-PLAN.md — Blur region computation: walk display list for backdrop-filter nodes, compute logical-coordinate union rect, wire through present path
- [x] 103-03-PLAN.md — CPU blur removal: make apply_backdrop_filter and push_backdrop_filter_command no-ops; keep function structure for future effects

### Phase 103.1: Audit Gap Closure (INSERTED)
**Goal**: Close the three gaps found by the v1.20 milestone audit: fix the blur region clear path (CR-01/BLUR-04), fix negative coordinate saturation in compute_blur_region (CR-02), fix damage_rect_count to report actual count not binary (DMGE-03), and produce Phase 103 VERIFICATION.md and VALIDATION.md
**Depends on**: Phase 103
**Requirements**: DMGE-03, BLUR-04 (re-verify BLUR-02)
**Success Criteria** (what must be TRUE):
  1. Removing `backdrop-filter` from a surface causes `kde_blur.set_region(None); kde_blur.commit()` to be emitted — the compositor stops blurring after the next commit
  2. A `backdrop-filter` node with negative layout x/y coordinates produces a blur rect that is clipped to `x=0, y=0` with width/height reduced by the clipped amount, not a rect shifted to the surface origin
  3. The debug overlay's damage rect count field shows the actual number of `DamageRect` entries passed to the present path (e.g., "3" for a 3-rect frame), not "0" or "1"
  4. Phase 103 VERIFICATION.md exists with `status: passed`
**Plans**: 1 plan

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 96. Selector Dependency Tracking | v1.18 | 3/3 | Complete | 2026-06-07 |
| 97. Service Field Dependency Tracking | v1.18 | 3/3 | Complete | 2026-06-09 |
| 98. Narrow Invalidation & Event Routing | v1.18 | 3/3 | Complete | 2026-06-09 |
| 99. Event-Driven Wayland Dispatch | v1.19 | 4/4 | Complete | 2026-06-09 |
| 100. Opaque Region Hints | v1.19 | 2/2 | Complete | 2026-06-09 |
| 101. Per-Region Damage | v1.20 | 1/1 | Complete   | 2026-06-10 |
| 102. HiDPI / Fractional Scale | v1.20 | 2/2 | Complete   | 2026-06-10 |
| 103. Compositor Blur Offload | v1.20 | 3/3 | Complete   | 2026-06-17 |
| 103.1. Audit Gap Closure (INSERTED) | v1.20 | 1/1 | Complete   | 2026-06-18 |

---

## Backlog

### v1.21 — Retained Layout & Display List

**Goal:** Retain Taffy layout state across passes and move toward rope-style
display-list storage so unchanged subtrees are referenced rather than copied.

**Scope:**

- Retain Taffy `TaffyTree` and node-id maps per surface; mutate in place on
  structural changes instead of rebuilding a fresh tree every layout pass.

- Replace flattened display-list command-vector rebuilds with immutable command
  segments or a rope-style retained command store so clean child spans are
  referenced, not copied into parent vectors on each dirty update.

- Add performance profiles for canonical shell workloads (idle shell, pointer
  move, text update from backend, scrolling, large icon grid, animation, theme
  reload, resize) to pin per-stage budgets.

**Priority:** medium — Taffy retention and display-list rope are the next
major layout/render throughput gains after invalidation is narrowed.

---

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
