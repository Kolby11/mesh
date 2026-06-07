# Roadmap: MESH

## Milestones

- 🔄 **v1.17 Performance: Scripting VM Consolidation** — Phases 92-95 active
- ✅ **v1.16 Element Library** — Phases 86-91 shipped 2026-05-26 ([archive](milestones/v1.16-ROADMAP.md), [audit](milestones/v1.16-MILESTONE-AUDIT.md))
- ✅ **v1.15 Persistent Storage System** — Phases 81-85 shipped 2026-05-26 ([archive](milestones/v1.15-ROADMAP.md))
- ✅ **v1.14 Unified Luau Scripting Runtime** — Phases 74-80 shipped 2026-05-26 ([archive](milestones/v1.14-ROADMAP.md))
- ✅ **v1.13 Manifest I18n Contract** — Phases 70-73 shipped 2026-05-24 ([archive](milestones/v1.13-ROADMAP.md))
- ✅ **v1.12 Module Object Contract** — Phases 65-69 shipped 2026-05-23 ([archive](milestones/v1.12-ROADMAP.md))
- ✅ **v1.11 Surface Keybind Completion** — Phases 60-64 shipped 2026-05-23 ([archive](milestones/v1.11-ROADMAP.md))

## Current Status

**Active milestone: v1.17 Performance: Scripting VM Consolidation** — Phase 92 complete, Phase 93 next.

## Phases

- [x] **Phase 92: VM Pool Foundation** — New `LuaVmPool`, `PooledVm` RAII types, and `ChunkCache`; no behavioral change to existing `ScriptContext`.
- [x] **Phase 93: Host API Re-targeting** — Refactor `install_host_api()` to accept a `&Table` target; pass `lua.globals()` temporarily to preserve existing behavior.
- [ ] **Phase 94: _ENV Isolation + Lazy-Init** — Replace `lua: Lua` with `vm: Option<PooledVm>` + `env: Option<Table>`; wire `ensure_initialized()`; per-component `_ENV` sandboxing; explicit checkin reset.
- [ ] **Phase 95: Integration + Validation** — Wire pool and cache into `FrontendSurfaceComponent`; `BackendScriptContext` lazy-init; hot-reload cache eviction; shipped surface regression proof.

## Phase Details

### Phase 92: VM Pool Foundation
**Goal**: A thread-local VM pool and compiled-source cache exist as isolated, independently testable types with no changes to existing component behavior.
**Depends on**: Nothing (first phase of v1.17)
**Requirements**: POOL-01, POOL-02, POOL-03, POOL-04, CACHE-01, CACHE-02
**Success Criteria** (what must be TRUE):
  1. A `LuaVmPool` can be created, and calling `checkout()` returns a `PooledVm` that returns its slot to the pool on drop without any assertion failure.
  2. The pool grows on-demand and never blocks when all existing slots are checked out simultaneously (minimum 4 VMs floor).
  3. A `PooledVm` dropped on a different thread than the one that checked it out triggers a detectable runtime assertion.
  4. A `ChunkCache` stores a source string under its FNV64 content hash and returns the same string on a second lookup without re-reading from disk.
  5. All existing shell surfaces (`navigation-bar`, `audio-popover`) continue to mount and render identically — pool and cache types exist but are not yet wired into `ScriptContext`.
**Plans**: 2 plans
- [ ] 92-01-PLAN.md — LuaVmPool + PooledVm RAII guard (POOL-01..04)
- [ ] 92-02-PLAN.md — ChunkCache (FNV64 source-string cache; CACHE-01, CACHE-02)

### Phase 93: Host API Re-targeting
**Goal**: Every per-component host API write targets a caller-supplied `&Table` rather than `lua.globals()`, validating the refactor against existing behavior before `_ENV` isolation is active.
**Depends on**: Phase 92
**Requirements**: ISO-02 (foundation work), ISO-04
**Success Criteria** (what must be TRUE):
  1. `install_host_api()` compiles and is called with `lua.globals()` as the target — all existing host API keys (`require`, `self`, `module`, `mesh.*`, `__mesh_svc_*`, `__mesh_request_redraw`, `__mesh_locale_current`) are set on that table.
  2. A `pool_baseline_globals` snapshot is captured once at pool VM construction and is available as an immutable shared reference before any per-component host API installation runs.
  3. All existing shell surface tests and component render paths pass without modification — the refactor is purely mechanical at this stage with no observable behavior change.
**Plans**: 2 plans
- [ ] 93-01-PLAN.md — LuaVmPool::baseline_globals + ScriptContext::install_host_api(&Table) (ISO-04, ISO-02)
- [ ] 93-02-PLAN.md — BackendScriptContext::install_host_api(&Table) (ISO-02)

### Phase 94: _ENV Isolation + Lazy-Init
**Goal**: Each component gets a private `_ENV` table on checkout so writes from one component are invisible to any other component sharing the same pool VM; components that are never mounted hold no pool slot.
**Depends on**: Phase 93
**Requirements**: ISO-01, ISO-02 (completion), ISO-03, INIT-01, INIT-02, CACHE-03
**Success Criteria** (what must be TRUE):
  1. Two components compiled from the same `.mesh` source and mounted simultaneously each have their own reactive global namespace — a write to a public field in component A does not appear in component B's render output.
  2. A component that is declared but never shown or mounted allocates no pool VM slot; the pool slot count is zero for unused components.
  3. On VM checkin, the component's `env_table` and all registry key handles are dropped and the thread is reset before the slot is returned — a subsequent component checkout on the same VM slot starts with a clean environment.
  4. Changing a `.mesh` source file while the shell is running invalidates its chunk cache entry so the next component activation re-reads and re-caches the updated source.
  5. The shipped `navigation-bar` and `audio-popover` surfaces continue to display correct reactive state after the isolation migration — service fields, locale, and theme tokens resolve through the new per-component `_ENV`.
 **Plans**: 2 plans

Plans:
- [x] 94-01-PLAN.md — ScriptContext struct change: vm+env_table, ensure_initialized(), all method migration to per-component _ENV (INIT-01, INIT-02, ISO-01, ISO-02)
- [ ] 94-02-PLAN.md — Checkin cleanup (Thread::reset in return_slot), uninit() wiring to Drop, compile_and_execute with ChunkCache (ISO-03, CACHE-03)

### Phase 95: Integration + Validation
**Goal**: The pool and cache are live on the production path; `FrontendSurfaceComponent` and `BackendScriptContext` use them; shipped surfaces prove the full system works end-to-end with no regressions.
**Depends on**: Phase 94
**Requirements**: INIT-03, INT-01, INT-02
**Success Criteria** (what must be TRUE):
  1. `FrontendSurfaceComponent::create_runtime_for_component` calls `ScriptContext::new_lazy()` with pool and cache references — the old direct `Lua::new()` constructor call is gone from that path.
  2. `BackendScriptContext` defers its `Lua::new()` call until the first `init()` or poll invocation — a backend provider that is registered but never polled allocates no VM.
  3. The shipped `navigation-bar` surface renders its language selector, responds to pointer hover and keyboard traversal, and reflects audio service state correctly after the full pool migration.
  4. The shipped `audio-popover` surface shows correct volume level, responds to slider drag, and fires the mute keybind correctly after the full pool migration.
**Plans**: TBD

## Progress Table

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 92. VM Pool Foundation | 2/2 | Complete | 2026-06-07 |
| 93. Host API Re-targeting | 2/2 | Complete | 2026-06-07 |
| 94. _ENV Isolation + Lazy-Init | 1/2 | In Progress | - |
| 95. Integration + Validation | 0/? | Not started | - |

---

## Backlog

### v1.17 — Performance: Scripting VM Consolidation

**Goal:** Eliminate the per-component `mlua::Lua` VM allocation — the largest
per-component startup and memory cost — and fix related hot-path cloning in
script state delivery.

**Scope:**
- Replace per-`ScriptContext` `Lua::new()` with a per-thread VM pool using
  `_ENV`-based environment isolation so each component gets a private namespace
  without paying the full stdlib + metatable cost.
- Lazy-init for inactive components; shared compiled chunks across runtimes
  once VM ownership is stable.
- Fix bound instance proxy deep-cloning the full snapshot `Value` into Lua
  tables on every component mount — replace with a metatable proxy or
  `Arc<Value>` view.
- Fix remaining tracked-fields and side-channel maps cloned per state sync;
  wrap in `Arc` and use copy-on-write, or return borrowed references.

**Priority:** high — scales with component count; every new module pays the
full VM cost today.

---

### v1.18 — Performance: Smart Invalidation

**Goal:** Replace coarse "tree rebuild + full repaint" invalidation with typed
dependency tracking so interaction state, service events, and script state
changes only dirty the affected nodes and paint slots.

**Scope:**
- Selector-dependency restyle: build per-rule selector-dependency sets at
  `StyleRuleIndex` construction time; for `:hover`/`:focus`/`:active` changes
  only restyle nodes whose dependency set intersects the changed state.
- Narrow script/service invalidation: add typed state dependencies so simple
  text/value changes dirty only dependent leaf nodes, style slots, layout
  slots, and paint slots rather than flagging `TREE_REBUILD`.
- Service event routing by tracked-field set: restrict service event fan-out
  to components whose runtimes actually read the changed fields, not all
  components with the service capability.

**Priority:** high — eliminates the dominant source of unnecessary CPU work on
pointer moves, backend updates, and focus transitions.

---

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
