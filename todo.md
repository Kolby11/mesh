# MESH — Active Backlog

Items marked `→ vX.Y` are tracked as GSD milestones in `.planning/ROADMAP.md`.

---

## Shell features

- [ ] Icon rendering using icon packs — XDG resolution and SVG rasterization pipeline needs end-to-end proof on a real module surface
- [ ] Layer system — specify which Wayland layer (background/bottom/top/overlay) a surface targets; needed for proper popover/overlay stacking
- [ ] Positioning system — `position: relative / absolute / fixed` in layout and paint; needed for tooltips, context menus, dropdowns → v1.22
- [ ] Settings module — surface for managing installed modules, active providers, theme, i18n → v1.22
- [ ] Popups / overlays — transient surfaces with custom content and dismiss behavior → v1.22
- [ ] Clean up backend modules and interfaces — consider moving the interface contract declaration from the separate `modules/interfaces/` directory into the implementing backend module, or bundling it as core metadata; evaluate impact on multi-provider resolution before changing

---

## Performance — remaining open items

Items owned by a milestone are listed with their milestone reference.

### P0 — scheduling and invalidation (→ v1.18 / v1.19)

- [ ] Replace fixed 16ms shell loop sleep with event/deadline-driven scheduler; remaining work: block on real Wayland/frame-callback wakeups instead of bounded polling → v1.19
- [ ] Stop broadcasting every backend service event to every component; first pass (observes_service_event) done; remaining: route by tracked fields / module dependencies → v1.18
- [ ] Narrow script/service invalidation below tree-rebuild + pixel repaint; add typed state dependencies → v1.18
- [ ] Avoid full-tree restyle for safe interaction changes; use selector-dependency analysis → v1.18

### P0 — scripting (→ v1.17)

- [ ] One `mlua::Lua` VM per ScriptContext (`runtime.rs:92`); move to per-thread VM with `_ENV` isolation → v1.17
- [ ] Bound instance proxy deep-clones full snapshot Value per component mount (`runtime.rs:284`); use Arc<Value> or metatable proxy → v1.17
- [ ] Tracked-fields and side-channel maps still cloned per state sync (`runtime.rs:202-203, 1021`); remaining: wrap in Arc and use copy-on-write → v1.17

### P1 — renderer hot paths

- [ ] Avoid flattening retained display-list subtrees into a new flat command buffer on each update; move toward segment/rope-style command storage → v1.21
- [ ] `StyleNodeAttrs::from_node` re-splits class strings per restyle; cache split classes on the retained `WidgetNode` once attribute mutation goes through an invalidating API → v1.23
- [ ] Replace per-node string/hash-heavy style matching with interned/typed node keys; remaining after first pass: interned tags, classes, attribute keys → v1.23
- [ ] Improve text ellipsis clipping: compute truncation from shaped glyph advances instead of measuring substrings on first miss
- [ ] Retain Taffy node state across layout passes; `build_taffy_tree` rebuilds a fresh TaffyTree every layout → v1.21

### P1 — presentation and memory (→ v1.20)

- [ ] Preserve surface configuration state: remaining dirty-bit work so unchanged size/options skip config construction entirely → v1.20 (surface_id clone now skipped on stable frames — 2026-06-02)
- [ ] Track damage as multiple rects deeper into the retained renderer → v1.20
- [ ] Add performance profiles for canonical shell workloads (idle, pointer move, text update, scroll, icon grid, animation, theme reload, resize) → v1.21
- [ ] Send `wl_surface::set_opaque_region` from the present path; compute union of fully-opaque background rects from retained display list → v1.19
- [ ] Wire `wp_blur_v1` / `org_kde_kwin_blur_v1` for backdrop-filter blur regions → v1.20
- [ ] HiDPI: plumb `wp_fractional_scale_v1` + `wp_viewporter`; render at native pixel density → v1.20

### P2 — architecture

- [ ] Introduce interned `Symbol` / `TagId` types before further string-key cleanups → v1.23
- [ ] Add allocator-level profile mode (allocation counts per render pass) → v1.23
- [ ] Consider typed runtime node representation for hot paths (`WidgetNode` tag/attrs/content as strings today) → v1.23
- [ ] GPU rendering — after retained layout, smart invalidation, and damage tracking ship → v1.25

---

## Completed (recent)

- [x] `restyle_subtree_with_index` cloned full `ComputedStyle` (~60 fields) per node; replaced with 5-field `ParentInheritedStyle` — 2026-06-02
- [x] `surface_id.clone()` on every render frame for LayerSurfaceConfig namespace; now only clones when config actually changes — 2026-06-02
- [x] `format!("{:.2}")` allocated new String for slider value and scroll offsets every annotation; now writes into retained entry buffer — 2026-06-02
- [x] All P0/P1 items from the 2026-05-27 shell performance audit and 2026-05-28 Skia canvas pass — see git log
