# MESH — Active Backlog

Items marked `→ vX.Y` are tracked as GSD milestones in `.planning/ROADMAP.md`.

---

## Shell features

- [x] Icon rendering using icon packs — XDG resolution and SVG rasterization pipeline. Done: the full chain (`<icon>` → `DisplayPaintContent::Icon` → `render_display_icon_node` → registry/XDG/pack resolution → resvg/image raster + caches → blit, with built-in missing-icon fallback) was already implemented and unit-tested; added an end-to-end pixel-level proof on a real shipped surface (`shipped_navigation_icon_rasterizes_pixels_on_real_surface` in `real_surfaces.rs`) that paints `@mesh/navigation-bar` and asserts the volume `<icon>` rasterizes non-transparent pixels within its layout box. Note surfaced by the proof: the navigation bar's right-aligned control cluster overflows off-buffer when painted narrower than its intrinsic content width (icon landed at x≈1978 on a 960px paint); fine at full screen width but worth a follow-up for small outputs.
- [x] Layer system — specify which Wayland layer (background/bottom/top/overlay) a surface targets; needed for proper popover/overlay stacking. Backlog sync 2026-06-20: already wired through manifest/settings surface layout (`mesh.surface.layer`), shell surface config, and layer-shell presentation backend.
- [x] Positioning system — `position: relative / absolute / fixed` in layout and paint; needed for tooltips, context menus, dropdowns. Backlog sync 2026-06-20: style parsing/resolution, layout insets, fixed-position viewport anchoring, retained display list, and painter handling are all present with focused layout tests.
- [ ] Settings module — surface for managing installed modules, active providers, theme, i18n → v1.22
- [ ] Popups / overlays — transient surfaces with custom content and dismiss behavior → v1.22
- [ ] Clean up backend modules and interfaces — consider moving the interface contract declaration from the separate `modules/interfaces/` directory into the implementing backend module, or bundling it as core metadata; evaluate impact on multi-provider resolution before changing

### Module architecture friction redesign — 2026-06-19

Brainstorm + decision record in [`docs/design-architecture.md`](docs/design-architecture.md).
Attacks authoring friction on top of the shipped interface/provider/frontend spine
(easy / unified / configurable). Selected path: **A+B headline, C/D reframed, F follow-on, E deferred.**

- [x] **A — Base surface schema.** Core ships the canonical surface schema (anchor/layer/size/keyboard/visibility) for every `kind: "frontend"`; authors declare only deltas. Done 2026-06-19: compact `mesh.surface` block parses into the single typed `SurfaceLayoutSection`, `surface_layout_from_manifest` reads it (no more verbose `settings.schema.surface`), all 7 shipped frontend manifests migrated (~110-line blocks removed). Tests in `mesh-core-module` + `mesh-core-surface-config`.
- [x] **B — One config block tagged by audience.** Done with A: `mesh.surface` is the single block; `mesh.surfaceLayout` + `provides.settings.schema.surface` collapsed into it. Fields documented as user-editable vs renderer-policy in `module-system.md`. Remaining for full B: generated settings UI consuming the editable subset (tracked under "Settings UI generated from contributed schemas").
- [x] **C (reframed) — Prune redundant capabilities.** Done 2026-06-19: removed restated consumer capabilities (`service.*.read/control`) from all 4 shipped backends (pipewire/pulseaudio/upower/hyprland-wm). Inverted the graph check — the contract's `[capabilities]` are consumer-only; providers are no longer *required* to declare them, and declaring one now emits `provider_declares_consumer_capability` (replaces the old `missing_provider_required_capability`). Capabilities stay fully explicit; no inference. Test in `mesh-core-module`.
- [x] **D (reframed) — Cheapen the single interface path.** Done 2026-06-19. Part 1: the graph auto-selects a provider when exactly one enabled backend implements an interface (`InstalledModuleGraph::from_parts`); shipped `config/module.json` now only names `mesh.audio` (2 implementers) — `mesh.power`/`mesh.hyprland` resolve automatically. Part 2: `mesh.interface.file` is now optional (contract inferred from emitted state), and a backend can implement an interface with no separate interface module at all — no `missing_interface_contract_file`/`missing_provider_interface_module_dependency` for that path. Tests in `mesh-core-module`.
- [x] **F — Root-graph auto-discovery.** Auto-populate installed modules from `modulesDir`; `config/module.json` holds decisions only (active providers, disabled list, layout entrypoint, theme/locale/icon pack). Backlog sync 2026-06-20: `load_installed_module_graph()` now scans `modulesDir` when the root graph omits an explicit module inventory, and `InstalledModuleGraph::from_parts()` keeps provider/layout decisions in `config/module.json`.
- [ ] **E (deferred) — Unify the 4 contribution schemas.** Theme/icons/i18n/keybinds under one `contributes` shape — only where they share honest structure; revisit after A/B land.
- Rejected: capability inference (C original) and parallel inline-interface path (D original) — both trade conceptual-simplicity for typing-simplicity, the failure mode this redesign avoids.

### Module system — remaining open follow-ups

The 2026-06-18 redesign largely shipped (canonical `module.json` with `mesh.uses`/
`mesh.provides`/`mesh.implements`, graph as single source of truth, typed graph
diagnostics for interfaces/icons/i18n/keybinds/capabilities, library modules,
resource packs). Remaining open work:

- [ ] Make event channels typed and declared. Backend `mesh.service.emit_event("WorkspaceChanged", payload)` should be validated against the interface contract; frontend `audio.VolumeChanged:on(...)` should be known to the compiler/diagnostics. Progress: graph emits `undeclared_interface_event_emit` for backend Luau emitting event names absent from the implemented interface contract; frontend-side validation still open.
- [ ] Eliminate service-specific Rust branches where possible. Current audio optimistic state and some debug/profiling paths are pragmatic, but new module domains should route through interfaces/contracts/providers.
- [ ] Treat manifests as defaults and user config as overrides. Module authors provide settings schema/defaults; users choose active provider, layout entrypoint, theme, icon pack, locale, and per-module settings in the root graph/settings files.
- [ ] Support multiple instances of the same frontend module. Module identity should not be the only surface identity; root graph should support configured instances like two panels or repeated widgets with separate settings/storage scopes.
- [ ] Keep `self.storage` scoped to module/component/provider instance and use it for durable per-instance state, not installed graph state.
- [ ] Settings UI generated from contributed schemas by default, with optional custom `settings_ui` entrypoint for advanced modules.
- [ ] Settings/diagnostics UI should show each module's uses/provides graph: required interfaces, active provider, optional interfaces, required icons, native binaries, capabilities, settings namespace, i18n catalogs, keybinds, health. Progress: `mesh.debug.module_graph` payload exists and the debug-inspector Modules tab renders the first entries; remaining is a full settings UI with filtering, active-provider detail, native binary health, keybinds, and per-module customization controls.

---

## Performance — remaining open items

Items owned by a milestone are listed with their milestone reference.

### P0 — scheduling and invalidation (→ v1.18 / v1.19)

- [x] Replace fixed 16ms shell loop sleep with event/deadline-driven scheduler. Done 2026-06-20: the Wayland shell loop now sleeps until computed deadlines or fd wakeups instead of forcing a 16ms idle cadence; `wait_for_events` polls Wayland + backend/IPC eventfd together so service and IPC messages interrupt long waits; component ticks can publish precise deadlines, with tooltip delay/fade using that path; Linux config/theme/source reloads wake through inotify instead of fixed short polling. The dev-window fallback blocks on eventfd when no minifb windows are open, and uses a 16ms pump only while minifb windows exist because minifb exposes no blocking event fd.
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
- [x] `StyleNodeAttrs::from_node` re-splits class strings per restyle; cache split classes on the retained `WidgetNode` once attribute mutation goes through an invalidating API. Done 2026-06-20: `WidgetNode` now keeps a lazy `class` token cache refreshed from the raw attribute before style resolution, so restyles borrow cached class slices instead of re-splitting the class string on every pass.
- [ ] Replace per-node string/hash-heavy style matching with interned/typed node keys; remaining after first pass: interned tags, classes, attribute keys → v1.23
- [x] Improve text ellipsis clipping: compute truncation from shaped glyph advances instead of measuring substrings on first miss. Done 2026-06-20: `truncate_with_ellipsis` now uses a single shaped `cosmic-text` layout for the common single-line LTR case and falls back to the older substring-measurement path for multi-line / RTL cases.
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
- [x] Send `wl_surface::set_opaque_region` from the present path; compute union of fully-opaque background rects from retained display list. Backlog sync 2026-06-20: already wired — shell render computes the root opaque rect and presentation forwards it to `wl_surface::set_opaque_region`.
- [x] Wire `wp_blur_v1` / `org_kde_kwin_blur_v1` for backdrop-filter blur regions. Backlog sync 2026-06-20: already wired — shell render computes blur regions and the Wayland presentation backend stores/commits them through the compositor blur protocol when available.
- [x] HiDPI: plumb `wp_fractional_scale_v1` + `wp_viewporter`; render at native pixel density. Backlog sync 2026-06-20: already wired — Wayland surfaces bind fractional-scale + viewporter protocols, scale buffers to physical pixels, and set viewport destinations for fractional outputs.

### P2 — architecture

- [ ] Introduce interned `Symbol` / `TagId` types before further string-key cleanups → v1.23
- [ ] Add allocator-level profile mode (allocation counts per render pass) → v1.23
- [ ] Consider typed runtime node representation for hot paths (`WidgetNode` tag/attrs/content as strings today) → v1.23
- [ ] GPU rendering — after retained layout, smart invalidation, and damage tracking ship → v1.25
