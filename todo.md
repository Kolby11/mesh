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

- [ ] Interaction frames still re-apply string style declarations per node (`apply_declaration_no_diagnostics` + theme defaults maps dominate the post-2026-06-10 toggle profile); folds into the typed/compiled declaration work → v1.23 and narrower invalidation → v1.18

- [ ] Avoid flattening retained display-list subtrees into a new flat command buffer on each update; move toward segment/rope-style command storage → v1.21
- [ ] `StyleNodeAttrs::from_node` re-splits class strings per restyle; cache split classes on the retained `WidgetNode` once attribute mutation goes through an invalidating API → v1.23
- [ ] Replace per-node string/hash-heavy style matching with interned/typed node keys; remaining after first pass: interned tags, classes, attribute keys → v1.23
- [ ] Improve text ellipsis clipping: compute truncation from shaped glyph advances instead of measuring substrings on first miss
- [ ] Retain Taffy node state across layout passes; `build_taffy_tree` rebuilds a fresh TaffyTree every layout → v1.21
- [ ] Affected-subtree template re-evaluation: `narrow_script_update` rebuilds the full tree (full template eval) then diffs; use `NodeServiceFieldDependencies` to re-evaluate only nodes whose tracked fields changed → v1.27
- [ ] Generation-aware retained-tree diff: `RetainedWidgetTree::update` FNV-hashes every node's style + attribute strings per paint; skip clean subtrees using dirty bits → v1.27
- [ ] Fuse the five per-frame `finalize_tree` annotation walks into one traversal; move hot annotations from string attributes to typed `WidgetNode` fields → v1.27

### P1 — backend modules

- [ ] Investigate `pw-dump --monitor` as a real volume event source for the pipewire-audio backend — `pw-mon` emits no `changed:` block for volume changes (verified with and without `--hide-params`), so the stream currently only signals client/object lifecycle, and volume detection leans on the safety poll
- [ ] Audit the other exec-polling backends (pulseaudio-audio still polls 2× `pactl` at 100ms) for the same exec-storm pattern fixed in pipewire-audio on 2026-06-10

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

- [x] Per-frame clone/parse batch from 2026-06-10 deep dive: (1) template expressions were re-parsed by the string interpreter on every evaluation — `eval_expr` now compiles to a memoized AST per expression string (thread-local cache, parse once per source string); (2) full `Theme` (token/defaults maps) was deep-cloned into `active_theme` on every tree build and retained restyle — now `Arc<Theme>` refreshed only when `theme_changed()` marks it stale, and the child-build/animation readers clone the Arc instead of the maps; (3) `runtime_state()` deep-cloned the whole script variable map per tree build — now an `Arc<ScriptState>` snapshot cached by a new `mutation_generation` counter (bumps on `set`, `set_host_value`, proxy register/unregister; safe because `Clone` drops proxies), `ScriptState` made `Sync` (Mutex snapshot cache, `Send + Sync` proxy closures); (4) added `[profile.release]` thin LTO + `codegen-units = 1` — the workspace had no release tuning at all — 2026-06-10
- [x] Interaction CPU burst (~50% spikes on click): perf-profiled a popover toggle storm; three per-frame full-tree passes dominated — `parse_transition_shorthand` re-parsed the same shorthand strings per node per frame (14.5%, now memoized thread-locally), `record_runtime_style_diagnostics` re-resolved every node's style a second time per restyle frame (9%, now runs only on tree rebuild), and `publish_element_metrics` built a full per-element JSON snapshot into Luau state every paint even though no shipped module reads `refs`/`elements` (11%, now gated on script usage detected at compile/reload). Measured ~87ms CPU/toggle after vs ~418ms before on the same hardware state — 2026-06-10
- [x] pipewire-audio backend exec storm: each `wpctl` run registered a PipeWire client, whose pw-mon Client added/removed event re-triggered `refresh_state()` → another `wpctl` — a self-sustaining loop (~22 spawns/sec, ~90% of a core in child CPU, plus ~6% in mesh-shell from constant wakeups). Fixed with a self-noise counter + batch classifier in `on_stream_batch` (refresh only on `changed:`/non-Client `added:`/unaccounted external client connects) and safety poll relaxed 100ms → 1000ms (250ms when pw-mon unavailable) — 2026-06-10
- [x] `surface_id.clone()` on every render frame for LayerSurfaceConfig namespace; now only clones when config actually changes — 2026-06-02
- [x] `format!("{:.2}")` allocated new String for slider value and scroll offsets every annotation; now writes into retained entry buffer — 2026-06-02
- [x] All P0/P1 items from the 2026-05-27 shell performance audit and 2026-05-28 Skia canvas pass — see git log
