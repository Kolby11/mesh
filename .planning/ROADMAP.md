# Roadmap: MESH

## Milestones

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

---

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 96. Selector Dependency Tracking | v1.18 | 3/3 | Complete | 2026-06-07 |
| 97. Service Field Dependency Tracking | v1.18 | 3/3 | Complete | 2026-06-09 |
| 98. Narrow Invalidation & Event Routing | v1.18 | 3/3 | Complete | 2026-06-09 |

---

## Backlog

### v1.19 — Performance: Event-Driven Frame Scheduler

**Goal:** Replace the fixed 16 ms shell loop sleep with a deadline-driven
scheduler that blocks on real Wayland/frame-callback wakeups.

**Scope:**

- Replace the unconditional `sleep` in `crates/core/shell/src/shell/runtime/mod.rs`
  with a runtime deadline calculation using shell-message backlog, pending
  Wayland events, render needs, reload deadlines, and throttled commands.

- Block on real Wayland events and `wl_surface::frame` callbacks as the
  primary render permit rather than bounded polling — eliminates idle CPU burn.

- Send `wl_surface::set_opaque_region` from the present path (compute union of
  fully-opaque background rects from the retained display list; lets the
  compositor skip alpha-blending under opaque surfaces).

**Priority:** high — current idle loop burns CPU even when nothing changes.

---

### v1.20 — Compositor Integration

**Goal:** Use Wayland compositor protocols to offload work and support HiDPI
displays without upscaling.

**Scope:**

- HiDPI / fractional scale: plumb `wl_output::scale` / `wp_fractional_scale_v1`
  to each surface and render at native pixel density; pair with `wp_viewporter`
  for non-integer ratios.

- Compositor blur offload: wire `wp_blur_v1` / `org_kde_kwin_blur_v1` for
  backdrop-filter blur regions so the compositor handles blur on supported
  compositors instead of Skia on the CPU.

- Track damage as multiple rects deeper into the retained renderer so
  presentation can commit per-region damage instead of whole-surface damage.

**Priority:** medium — HiDPI is a correctness issue on 2× displays; blur
offload and fine-grained damage are performance polish.

---

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

### Future: i18n Configuration UI

Full locale picker and per-language catalog management surface for end users.
The manifest i18n contract (v1.13) and scripting runtime (v1.14) are
prerequisites; the settings module (v1.22) provides the host surface.
