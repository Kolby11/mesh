# MESH — Active Backlog

Items marked `→ vX.Y` are tracked as GSD milestones in `.planning/ROADMAP.md`.

---

## Shell features

- [x] **TOP PRIORITY — finish promoted nav popover polish and remove manifest surface geometry.** Investigated 2026-07-01: `language-popover`/`theme-selector` were already migrated to `mesh.kind: "component"` with no `mesh.surface` block (anchor/layer/width/height/keyboard_mode/display_transition) — that part of this item was already done by an earlier pass (`9305df00`, `35a045a1`) and is verified by `shipped_tiny_nav_popovers_are_embeddable_components_without_surface_geometry`. No `offset-y`/`offset-x` markup nudges exist anywhere in the module tree — placement is already anchor-rect + CSS only (`anchor-ref` resolves the trigger's real measured layout box in `collect_child_surface_requests`/`popover_anchor_bounds`, `shell/component/shell_component.rs:1750-1884`). Horizontal anchoring: `anchor="bottom" gravity="bottom"` maps 1:1 to `xdg_positioner::{Anchor,Gravity}::Bottom` with no `Left`/`Right` bit (`shell/runtime/render.rs:953-978`, `presentation/src/wayland_surface/popup.rs:126-155`), which per the `xdg_positioner` protocol centers the popup horizontally on the anchor point — code-reviewed as spec-correct; not independently re-verified against a live compositor in this pass (no Wayland session in this environment). The real, confirmed gap was close/dismiss + exit-animation: closing child popovers were torn down (`destroy_child_surface_at`) the instant `open` flipped false, before their own CSS `.mesh-surface-exiting` transition (already authored in both components' `<style>`) ever got a chance to apply or run. Fixed: `ChildSurface` now carries a `closing_until` grace deadline sized from the popover's own resolved transition duration (`child_hide_transition_ms`); the shell keeps repainting/presenting the closing popup and calls a new `ShellComponent::set_closing_child_keys` so `finalize_tree` scopes `mesh-surface-exiting` to just that popover's subtree (not the whole tree) before style resolution runs, so the existing per-node CSS transition engine actually animates it — then tears the popup down once the deadline passes (or cancels cleanly if the popover reopens first). Tests: `child_surface_reconcile_plays_exit_transition_before_teardown`, `child_surface_reopen_cancels_pending_exit_transition`, `set_closing_child_keys_scopes_exit_transition_to_popover_subtree_only` (real `@mesh/theme-selector` component, asserts the class is applied/removed and a real transition starts). Also removed `theme-selector/src/components/bubble-burst.mesh`, a decorative burst animation that was never wired into `theme-selector/src/main.mesh` or referenced anywhere (dead code, not the actual bubble launch animation — that's the already-working `bubble-options.mesh` fan-out CSS). Remaining/deferred: visual confirmation of horizontal centering on a live compositor; entrance (`mesh-surface-entering`) is not yet similarly scoped for child popovers (only exit was the confirmed-broken path); the `module.json` "embeddable component, no surface geometry" manifest-shape decision below is a broader follow-on (multi-module design), left open.
- [x] **Larger design step — module-declared component variables.** Design completed in [`docs/component-configuration.md`](docs/component-configuration.md). Decision: configuration belongs to the component's `.mesh` source as a typed `<props>` public API rather than `module.json`; packaging stays in the manifest. The design specifies types/defaults, `prop(name)` CSS projection, reactive `props.name` script projection, generated settings UI, global/instance/per-instance persistence and precedence, validation/LSP diagnostics, i18n labels, token/icon integration, and the boundary between CSS/content sizing and top-level Wayland placement. Implementation remains phased in that document.
- [x] Icon rendering using icon packs — XDG resolution and SVG rasterization pipeline. Done: the full chain (`<icon>` → `DisplayPaintContent::Icon` → `render_display_icon_node` → registry/XDG/pack resolution → resvg/image raster + caches → blit, with built-in missing-icon fallback) was already implemented and unit-tested; added an end-to-end pixel-level proof on a real shipped surface (`shipped_navigation_icon_rasterizes_pixels_on_real_surface` in `real_surfaces.rs`) that paints `@mesh/navigation-bar` and asserts the volume `<icon>` rasterizes non-transparent pixels within its layout box. Follow-up resolved 2026-06-22: the off-buffer overflow (icon at x≈1978 on a 960px paint) was a **test artifact**, not a real layout bug — the proof's `audio_network_catalog` omitted `mesh.hyprland`/`mesh.power`, so `WorkspaceList`/`WindowTitle`/`BatteryButton` rendered unbounded ~700px error-string placeholders that inflated the bar past its intrinsic width. Switched the proof to `navigation_bar_catalog()` (all six consumed interfaces present), paints at a realistic 1280px panel width, and now asserts the right `.right-cluster` control cluster (and the volume icon inside it) stay on-buffer — turning the observation into a regression guard. Production content is already bounded (`window-title-row { max-width: 240px }`, small icons/pills). Robustness follow-up completed 2026-07-02: generated component-error boxes and text now carry a core marker whose post-restyle constraints cap them at 320px, allow flex shrink, clip overflow, and render a single-line ellipsis. This prevents one broken embedded module from expanding its host surface; covered by `generated_error_placeholder_is_bounded_after_restyle_constraints`.
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
- [x] **C (reframed) — Prune redundant capabilities.** Done 2026-06-19: removed restated consumer capabilities (`service.*.read/control`) from all 4 shipped backends (pipewire/pulseaudio/upower/hyprland-wm). Inverted the graph check — the contract's `[capabilities]` are consumer-only; providers are no longer _required_ to declare them, and declaring one now emits `provider_declares_consumer_capability` (replaces the old `missing_provider_required_capability`). Capabilities stay fully explicit; no inference. Test in `mesh-core-module`.
- [x] **D (reframed) — Cheapen the single interface path.** Done 2026-06-19. Part 1: the graph auto-selects a provider when exactly one enabled backend implements an interface (`InstalledModuleGraph::from_parts`); shipped `config/module.json` now only names `mesh.audio` (2 implementers) — `mesh.power`/`mesh.hyprland` resolve automatically. Part 2: `mesh.interface.file` is now optional (contract inferred from emitted state), and a backend can implement an interface with no separate interface module at all — no `missing_interface_contract_file`/`missing_provider_interface_module_dependency` for that path. Tests in `mesh-core-module`.
- [x] **F — Root-graph auto-discovery.** Auto-populate installed modules from `modulesDir`; `config/module.json` holds decisions only (active providers, disabled list, layout entrypoint, theme/locale/icon pack). Backlog sync 2026-06-20: `load_installed_module_graph()` now scans `modulesDir` when the root graph omits an explicit module inventory, and `InstalledModuleGraph::from_parts()` keeps provider/layout decisions in `config/module.json`.
- [ ] **E (deferred) — Unify the 4 contribution schemas.** Theme/icons/i18n/keybinds under one `contributes` shape — only where they share honest structure; revisit after A/B land.
- Rejected: capability inference (C original) and parallel inline-interface path (D original) — both trade conceptual-simplicity for typing-simplicity, the failure mode this redesign avoids.

### Module system — remaining open follow-ups

The 2026-06-18 redesign largely shipped (canonical `module.json` with `mesh.uses`/
`mesh.provides`/`mesh.implements`, graph as single source of truth, typed graph
diagnostics for interfaces/icons/i18n/keybinds/capabilities, library modules,
resource packs). Remaining open work:

- [x] Make event channels typed and declared. Backend `mesh.service.emit_event("WorkspaceChanged", payload)` is checked against the implemented interface contract by installed-graph source diagnostics (`undeclared_interface_event_emit`), and static frontend `local alias = require("mesh.interface")` subscriptions through `alias.Event:on(...)` or `alias.events.Event:subscribe(...)` are checked against required interface contracts (`undeclared_interface_event_subscription`). Runtime delivery also validates declared inline payload schemas, drops invalid events, and records `service_contract_warning` diagnostics (`backend_interface_event_validates_and_delivers_to_components`, `backend_interface_event_drops_invalid_payload_with_diagnostic`). Dynamic event names remain intentionally runtime-only because they cannot be resolved by static source analysis.
- [ ] Eliminate service-specific Rust branches where possible. Current audio optimistic state and some debug/profiling paths are pragmatic, but new module domains should route through interfaces/contracts/providers.
- [x] Treat manifests as defaults and user config as overrides. Done: `config/module.json` carries provider and layout decisions over auto-discovered module manifests; layered shell settings select theme, default icon pack, and locale; module settings files override manifest surface defaults and typed `<props>` defaults with global/per-instance precedence. Runtime settings reloads reapply theme/locale/module changes. Coverage includes graph active-provider/layout tests, config merge tests, `frontend_settings_override_surface_layout_defaults`, `load_frontend_module_settings_reads_prop_scopes`, and `settings_props_apply_global_and_per_instance_precedence`.
- [ ] Support multiple instances of the same frontend module. Module identity should not be the only surface identity; root graph should support configured instances like two panels or repeated widgets with separate settings/storage scopes.
- [x] Keep `self.storage` scoped to module/component/provider instance and use it for durable per-instance state, not installed graph state. Done: storage paths already encode kind/module/owner/instance and remain independent of installed-graph state; frontend embedded runtimes now pass the component package ID and concrete runtime instance key instead of collapsing all three scope dimensions to the module ID. Backend contexts retain provider-instance scoping. Added `frontend_storage_is_isolated_by_component_instance`.
- [ ] Settings UI generated from contributed schemas by default, with optional custom `settings_ui` entrypoint for advanced modules.
- [ ] Settings/diagnostics UI should show each module's uses/provides graph: required interfaces, active provider, optional interfaces, required icons, native binaries, capabilities, settings namespace, i18n catalogs, keybinds, health. Progress: `mesh.debug.module_graph` payload exists and the debug-inspector Modules tab renders the first entries. Added 2026-07-02: typed graph entries and JSON include required/optional native binaries, keybind action IDs, resolved `interface=provider` pairs, and structured native-binary availability states; the Modules view renders them, correctly handles structured provided-interface records, and filters across IDs, kinds, interfaces, providers, binaries, keybinds, and diagnostics. Binary resolution is shared with installed-graph diagnostics and supports explicit executable paths as well as PATH lookup. Remaining: per-module customization controls in the full settings UI.

### Embeddable popovers via `<popover>` surface promotion — 2026-06-21

**Problem.** `language-popover` and `theme-selector` are each shipped as a
_standalone frontend module that owns its own Wayland layer surface_, with
hardcoded geometry (`width/height/min/max = 112×74`) and hand-computed
positioning (`shell.position-surface` + `margin_top = -18` math). This is the
root cause of three observed defects:

1. **Not content-sized.** They declare `size: "content_measured"` but then pin
   `min == max`, cancelling it — forced because `bubble-options.mesh` lays its
   options out with `position: absolute` inside a `position: relative` stage,
   and absolutely-positioned children contribute **zero** to measured size, so
   there is nothing to measure.
2. **Separate surface for a 3-button menu.** Over-modularized; a tiny anchored
   menu does not need its own `shell.surface` capability, manifest, and module.
3. **Hover gap / flicker.** Two surfaces = two input regions with a physical gap
   between trigger and popover. Crossing it fires `pointerleave` → hide. The
   per-component `onSelectorEnter` re-activate keepalive is a fragile patch.

**Root constraint (verified).** A Wayland surface is a fixed-size buffer;
`PixelBuffer::set_pixel` (`render/src/surface/buffer.rs:111`) drops every
out-of-bounds pixel. `position: absolute` is layout-only — it cannot paint past
the host surface's buffer. The 56px nav-bar surface (`exclusive_zone: 56`) has
no pixels below the bar, so a below-bar popover _must_ live in some surface that
extends there. Today that's a sibling **overlay layer surface** (manual
position, hand-rolled dismiss). Dynamic-sized surfaces are _not_ impossible —
`content_measured` already resizes the launcher surface; the only impossibility
is one surface drawing outside its own bounds.

**Direction.** Make `<popover>` (already a real core element,
`elements/src/element.rs:64`) the **promotion boundary**: authored inline as a
child of the trigger's component (embeddable, downloadable, no manifest
geometry), and realized at runtime as a compositor **`xdg_popup`** child of the
parent surface via wlr-layer-shell `get_popup` + a positioner. The popup gives,
for free: content-driven size, compositor-side anchoring/flip-at-edge (kills the
margin math), and an input grab (kills the hover-gap flicker). The
sibling-layer-surface popover module path is retired for small menus; true
top-level surfaces (bar, launcher, full quick-settings panel) keep owning a
surface.

- [x] **Presentation: add an `xdg_popup` promotion path.** Done 2026-06-21.
      `SurfaceEntry.layer_surface` generalized into a `SurfaceRole { Layer | Popup }`
      enum (`wayland_surface/backend.rs`) so popups reuse the entire SHM /
      present / scale / HiDPI / input path — only creation, layer-config, and
      dismiss differ. New `wayland_surface/popup.rs` carries a presentation-level
      `PopupPlacement`/`PopupConfig` (anchor rect, size, anchor, gravity, constraint,
      offset, grab+serial) that mirrors `mesh_core_elements::PopoverPlacement` but
      stays independent of that crate; pure `map_anchor`/`map_gravity`/`map_constraint`
      onto `xdg_positioner` enums are unit-tested. Backend binds `xdg_wm_base`
      (`XdgShell`, optional), implements `PopupHandler` + `delegate_xdg_popup!`,
      and exposes `configure_popup` (create or `xdg_popup.reposition`),
      `destroy_popup`, `destroy_popups_for_parent`, `take_dismissed_popups`,
      `popup_supported`, plumbed through `PresentationEngine`. Popup created via
      `Popup::from_surface(None, …)` + `LayerSurface::get_popup` (parent role from the
      layer surface); grab taken only with a click serial (hover popovers stay
      no-grab). Compositor `done` removes the entry and queues the id for the shell.
      (Subsurface rejected: not reliably allowed to exceed parent geometry.)
- [ ] **Shell: one component → base surface + N popup targets.** A
      `FrontendSurfaceComponent` currently maps 1:1 to a surface; popups make it
      1:N. Generalize `SurfaceId`/presentation-handle bookkeeping, per-target paint
      buffers in `runtime_tree.rs`, element-metrics publication, and input routing
      so popup input routes back to the same VM with correct popup-local coords.
      **Reframed 2026-06-23 (web-like composition):** surfaces are *containers*, not
      authoring units — one parent surface holds a component tree; in-tree
      escape-bounds nodes (`<popover open>`, later `<tooltip>`/dropdowns) are
      *transparently* promoted to child `xdg_popup` surfaces fed by the same VM.
      Explicit new-surface authoring (sidebar/panel) stays a rare, deferred opt-in.
      Authors should not need to think about surfaces for ordinary layout:
      if inline UI uses `position: absolute` or another escape-bounds pattern and
      the runtime cannot physically paint it inside the parent buffer, the shell
      should derive the needed child surface automatically rather than requiring
      manifest geometry or user-managed surface IDs.
      **Foundation landed 2026-06-23 (plumbing first, no behavior change):**
      per-surface render state extracted into `SurfaceTarget`; `ComponentRuntime` now
      owns `parent: SurfaceTarget` + `children: Vec<ChildSurface>` (keyed by node
      `_mesh_key`) with `targets()`/`target_ref_for_surface`/`target_mut`
      (`shell/types.rs`); `component_target_for_surface` + every-target surface index
      with lazy rebuild-on-miss (`runtime/mod.rs`); the per-surface present pipeline
      extracted into `present_surface_target(index, TargetRef, …)` and the parent
      routed through it (`runtime/render.rs`); legacy separate-module `ActivatePopover`
      promotion still works (parent-only runtime). Proof:
      `component_runtime_resolves_parent_and_child_surface_targets`. Existing shell
      suite preserved at the 347-passed/7-known-failing baseline.
      **Consumer pass progress 2026-06-23:** `ShellComponent` now exposes
      `ChildSurfaceRequest` + `ChildSurfaceKind::{Popover, Overflow}` and
      `paint_child_surface(node_key, …)`; `FrontendSurfaceComponent` derives
      requests from the last painted tree for open in-tree `<popover>` nodes
      and can paint a keyed subtree into a child buffer at local origin. Tests:
      `open_popover_nodes_derive_child_surface_requests`,
      `closed_popover_nodes_stay_inline`.
      **Shell consumption progress 2026-06-24:** reconciles open popover requests
      into child `xdg_popup` surfaces, registers/tears down `self.surfaces` and
      `self.core.surfaces`, builds `PopupConfig`, drains compositor dismissals,
      paints/presents child targets through `present_surface_target(Child)`, routes
      child-surface input back to the same VM with popup-local coords, records child
      profiling, and paints debug layout overlays from child-local debug trees.
      Tests: `child_surface_reconcile_creates_popup_and_paints_subtree`,
      `child_surface_reconcile_removes_closed_popover`,
      `dismissed_popup_drain_removes_child_surface`,
      `child_surface_input_routes_to_local_child_handler_and_profiles`,
      `child_surface_debug_tree_offsets_layout_to_local_origin`.
      **Remaining (consumer pass):** later automatic `Overflow` derivation beyond
      explicit `<popover>` and production migration from legacy separate popover
      modules to in-tree popover nodes.
- [x] **Determinism decision: `<popover open>` always promotes when shown** —
      do **not** conditionally promote only when content overflows the host
      (the "measure first, surface only if it spills" model). Conditional promotion
      makes the same component render via two different paths (inline vs popup) with
      divergent input/grab/coordinate behavior and nondeterministic feel. Keep
      authoring inline; keep realization deterministic. Done 2026-06-24:
      `FrontendSurfaceComponent::child_surface_requests` derives every open in-tree
      `<popover>` as a child-surface request; there is no inline-vs-popup overflow
      branch for shown popovers.
- [ ] **Centralize the popover controller in core.** Replace per-component Lua
      hover/keepalive (`onSelectorEnter` re-activate) with a core state machine that
      owns: anchor rect, open/close, hover-bridge, dismiss, one-open-per-group
      exclusivity, and grab acquisition. Declarative authoring target:
      `<popover anchor={refs.language_button} open={open}>`. Keep `mesh.popover.*`
      as the imperative escape hatch. Progress 2026-06-24: shell now owns a
      hover-bridge controller for promoted popovers through `HidePopover {
      defer_for_hover_bridge }`, `pending_popover_hides`, scheduler deadlines,
      pointer-enter cancellation, and pointer-leave scheduling from promoted popup
      surfaces. `mesh.popover.hide(id, { bridge = true })` emits the new request,
      and `quick-settings` no longer carries popover-side `onpointerenter` /
      `onpointerleave` keepalive handlers. Core also enforces one-open-per-trigger
      for promoted sibling popovers and synchronizes compositor outside-dismiss
      (`xdg_popup.done`) back into shell visibility for both auto-derived child
      popups and legacy promoted popover modules. Remaining: migrate audio once
      drag/capture state is represented in core, then broaden the exclusivity
      policy beyond same-trigger siblings if needed.
      Follow-up 2026-06-24, fixed 2026-06-29 (`2425c33a`): language/theme
      options could still close while the pointer crossed into the promoted
      popup, because `PointerEventKind::Enter` updated backend pointer focus
      but emitted no shell-visible `WindowEvent`, so pending hover-bridge hide
      cancellation depended on a later `PointerMove`. Fixed by emitting a
      synthetic `PointerMove` at the entry coordinates on pointer enter
      (`presentation/src/wayland_surface/handlers.rs`), plus fixing
      `surface_is_promoted_popover` to detect in-tree child (xdg_popup)
      surfaces and `cancel_pending_popover_hide` to not call
      `set_surface_exiting` on the parent when cancelling a child hide.
- [x] **Grab vs hover nuance.** An xdg_popup grab requires a recent input
      _serial_ (a click) — so grabbed (click-to-dismiss-outside) popups can't be
      opened by pure hover. Decide per popover: hover-open menus stay no-grab (core
      hover-bridge handles dismiss); click-open menus take the grab. Recorded in
      `docs/frontend/elements.md` and `docs/frontend/mesh-syntax.md`; the Rust
      `PopoverGrab` contract already enforces `Hover` as the default and maps
      `grab="click"` to compositor grab requests.
- [x] **Buffer padding + input region for shadows.** Done 2026-07-02. Popup
      buffers were sized exactly to the popover's laid-out content box
      (`collect_child_surface_requests`, `shell_component.rs`), so any
      `box-shadow`/`filter` overshoot on the popover or its descendants (e.g.
      a floating bubble button's shadow) was hard-clipped at the buffer edge,
      and no input region was ever set for child/popup targets (harmless only
      because buffer == content). Alpha buffers were already in place
      (`Argb8888` everywhere), so no format change was needed — this was
      purely the geometry/input-region gap.
      Fix: `node_visual_bounds` extracted from the existing damage-rect shadow
      math (`visual_damage_rect_for_widget_node`) as a shared, unclipped f32
      helper; new `subtree_visual_bounds`/`popover_content_padding` walk the
      *whole* popover subtree (not just the popover node's own style) and
      return per-side padding so a shadow on any descendant is covered.
      `ChildSurfaceRequest`/`ChildSurface` carry `content_padding` through to
      `reconcile_child_surfaces` (`shell/runtime/render.rs`), which inflates
      the popup buffer/surface size by the padding, shifts the `xdg_positioner`
      offset back by the leading padding so the *visible* content stays
      anchored exactly where it would land unpadded, and
      `paint_and_present_child_surface` now sets the child's Wayland input
      region to the true (unpadded) content rect — mirroring the existing
      parent/tooltip `content_input_size()` pattern — so clicks over the
      shadow padding pass through instead of hitting a dead zone. `paint_child_surface`
      gained a `content_offset` param so painting still lands the unpadded
      content at the right spot inside the larger buffer. Test:
      `popover_with_descendant_box_shadow_gets_buffer_padding`
      (`shell_component.rs`). Full `mesh-core-shell` suite (388 tests) and
      workspace build pass.
- [x] **Content sizing + reposition.** Done: `reconcile_child_surfaces`
      (`shell/runtime/render.rs:512-559`) sizes each popup from
      `request.content_size` (the measured `<popover>` subtree) every frame,
      and `PresentationEngine::configure_popup` (`presentation/src/wayland_surface/backend.rs:834-843`)
      repositions an existing popup via `xdg_popup.reposition` instead of
      recreating it (`reposition_popup`, `backend.rs:948-963`) so anchor moves
      (output/exclusive-zone change) don't tear the popup down.
- [x] **Keyboard/focus + a11y across the surface boundary.** Done: promoted child
      surfaces route keyboard input back through the owning component VM and keyed
      popup subtree; click/grab ownership records the child surface as keyboard
      owner, while the existing focus traversal supports menu/menu-item roles,
      ArrowUp/ArrowDown sibling roving, Tab transfer, Escape, and return focus.
      Menu roles map through the compiler/render accessibility pipeline to AccessKit.
      Parent hide destroys every child popup and removes its core/presentation/
      surface-index bookkeeping; added
      `hiding_parent_surface_destroys_child_popups_and_clears_child_keyboard_focus`
      as the direct lifecycle regression proof.
- [x] **Compositor support caveat.** layer-shell `get_popup` is supported on
      wlroots/KDE/Hyprland but layer-shell itself is absent on GNOME — already
      inside MESH's `wlr-layer-shell-v1` compatibility constraint; recorded as a
      known non-goal boundary in `docs/frontend/elements.md` and
      `docs/frontend/mesh-syntax.md`.
- [x] **`module.json` rework — embeddable component, no surface geometry.**
      Done (`9305df00`, `35a045a1`): `language-popover` and `theme-selector`
      both ship `mesh.kind: "component"` with no `mesh.surface` block at all
      — confirmed by re-reading both `module.json` files 2026-07-02. Original
      note below kept for the design rationale.
      An embeddable popover should not declare a `mesh.surface` block at all
      (no anchor/layer/width/height/min/max). Decide the manifest shape for "a
      module that exports an embeddable component consumed by another module":
      either a new `mesh.kind` (e.g. `"component"`) or let a `frontend` module
      declare a component **export** with no surface entrypoint. Surface geometry
      stays only for true top-level surfaces; popover positioning becomes optional
      _positioner hints_ (anchor edge, gravity, offset) with sane defaults, not
      pinned pixel sizes. Migrate `language-popover` + `theme-selector` to this
      shape (they may stay independently downloadable, just embedded — not
      surface-owning); fold `bubble-options.mesh` layout into in-flow or an
      explicitly content-sized stage so measurement works.

---

## Codebase cleanup — 2026-06-22 audit

Findings from a four-agent scan of the largest production files. Two batches
already landed: **confirmed dead-code deletions** (commit `afc9a0d`) and
**cross-crate/intra-crate dedup** (commit `a4125d7`). The items below are the
remaining, deliberately-deferred findings (cheap quality wins + larger
refactors). Each cites `file:line` as of the audit; reverify line numbers
before editing.

### Cheap quality wins (low risk, do next)

All five landed 2026-06-23 (single commit).

- [x] Extract `reset_render_caches(&mut self)` from the ~8 identical cache-reset
      lines duplicated in `FrontendSurfaceComponent::theme_changed` and
      `locale_changed`. Done: shared helper drops the retained
      tree/layout/render-object/display-list caches; both hooks call it.
- [x] Collapse the `invalidate_surface_config` one-line wrapper into one
      implementation. Done: kept the widely-used `invalidate_surface_config`
      name (folded the `SURFACE_CONFIG` invalidation in), removed the
      `invalidate_surface_config_only` variant and updated its lone call site.
- [x] Rename `validate_phase87_attribute_value` → `validate_known_attribute_value`
      (`ui/elements/src/element.rs`). Done — pure rename of the production fn.
- [x] `request.rs`: extract the 4 identical `service_unavailable` error-JSON
      literals in `dispatch_service_command` into a `service_unavailable_response()`
      helper; collapsed the `Some(Err(()))`/`None` arms.
- [x] `debug.rs`: `module_graph_entries` iterated `graph.contributed_themes()`
      twice — combined into one `.map(...).unzip()`.

### Larger refactors (bigger diffs — best as separate reviewed PRs)

- [x] Split `FrontendSurfaceComponent::paint` (~486 lines,
      `shell/component/shell_component.rs:365`). `compute_tooltip_state()` extracted
      2026-07-02 (tooltip placement/opacity/slide + per-frame render hints, ~80 lines
      pulled out of the inline closure; `mesh-core-shell` full suite: 387 passed, 0
      failed). Completed 2026-07-02: extracted `paint_pixel_regions()` plus
      `paint_damage_rect()`/`paint_selected_pixels()` for the full-surface,
      single-rect, bounding-rect, and multi-rect paths while preserving tooltip
      damage filtering and merged paint metrics. Verified with
      `nix develop --command cargo test -p mesh-core-shell --lib` (388 passed).
- [x] `StyleResolver::apply_declaration` property table. Completed 2026-07-02:
      `css_property_table!` now generates both the zero-cost lowering match and
      the supported-property registry from one set of property arms, removing the
      separately maintained ~100-entry list. The existing diagnostic and
      no-diagnostic validation paths remain separate and converge only for the
      final lowering step. Verified with `cargo fmt --check` and
      `mesh-core-elements` lib tests (127 passed; the known pre-existing shipped
      audio padding fixture remains the sole failure).
- [x] `installed_graph.rs`: `build_graph_diagnostics` (~570 lines) now delegates
      independent passes to named helpers for frontend requirements, backend
      providers, contributed resources, source scans, interface files, and
      keybind trigger conflicts. Verified 2026-07-02 with `cargo fmt --check`
      and `mesh-core-module` lib tests.
- [x] `install_host_api` splits: frontend (`scripting/context/runtime.rs:824`,
      ~445 lines) and backend (`scripting/backend/runtime.rs:480`, ~200 lines).
      Break per-subsystem (`install_popover_api`, `install_locale_api`,
      `install_service_api`, `install_exec_api`, …). Backend done 2026-07-02:
      backend `install_host_api` now delegates to `install_service_api`,
      `install_exec_api`, `install_config_api`, and `install_log_api`; verified
      with `mesh-core-scripting` and `mesh-core-backend` lib tests. Frontend
      completed 2026-07-02: `install_host_api` now delegates to subsystem
      installers for module globals, events, UI, locale, logging, popover,
      loader/import, and refs APIs. Verified with `cargo fmt --check`,
      `mesh-core-scripting`, and `mesh-core-backend` lib tests.
- [x] `handle_component_input` (`shell/component/input/mod.rs`, ~500 lines):
      extracted `handle_key_pressed`/`handle_key_released` into
      `input/keyboard.rs`, leaving the top-level input dispatcher to delegate
      keyboard arms without changing activation order. Verified 2026-07-02 with
      `cargo fmt --check` and `mesh-core-shell` lib tests.
- [x] `annotate_runtime_tree` (`shell/component/runtime_tree.rs:577`, ~180 lines,
      11 args): introduced `RuntimeAnnotationContext` and split slider value
      preservation into dedicated helpers. Verified 2026-07-02 with
      `cargo fmt --check` and `mesh-core-shell` lib tests.

### Decisions / verify-then-act (judgment calls, don't blind-delete)

- [x] **`ComputedStyle::align_content` + `AlignContent`**. Done 2026-06-23:
      wired into `taffy_style_for_node` (`ui/elements/src/layout.rs`) — it's a
      real flex property, so the mapped value now forwards to Taffy. Added
      `align_content_end_pushes_wrapped_lines_to_cross_end` regression test
      proving wrapped lines respect the cross-axis distribution.
- [x] **Element-diagnostics feature is unwired.** Done 2026-06-23 (removal path).
      The dropped per-build call (`let _element_diagnostics = …`) ran
      `collect_element_diagnostics` on every node build and discarded the result;
      removing it orphaned `collect_element_diagnostics` + `attribute_static_value`
      (build confirmed both dead, plus the LSP never used them). Removed both and
      their three compiler-side tests (the two `frontend_element_diagnostics_*`
      and the `phase87/phase88_collects_*` tests). The reusable
      `validate_element_attribute`/`validate_element_event` primitives in
      `element.rs` stay (exported from `mesh-core-elements`, own tests) as the
      natural home if compile-time authoring diagnostics get surfaced later.
- [x] **Dead element compatibility tables.** Done 2026-06-23: verified zero
      reads of `ElementContractDef::compatibility` (the two `.compatibility`
      hits were the unrelated module-manifest field), then removed the field,
      the `ElementCompatibilityRef` struct, the `compat()` const fn, the three
      `HTML_REF`/`QT_REF`/`FLUTTER_REF` statics, the macro's `$compat` param,
      and the trailing compat arg from all 65 `contract!` invocations (regex).
      Net −98 lines in `element.rs`; workspace builds, elements tests pass
      (only the pre-existing audio-style baseline failure remains).
- [x] Dead `StyleResolver` non-cached `restyle_subtree` / `restyle_subtree_children`.
      Done 2026-06-23: confirmed only doc-comment references (the `_cached`
      variants carry the four production call sites in `rendering.rs`); removed
      both, folded their doc comments onto the `_cached` variants, and updated
      the references in `events.rs` and `restyle/metrics.rs`.
- [x] `PainterCommand::{DrawText,DrawPath}`, `PainterBlendMode::{Multiply,Screen}`,
      `PainterPath`/`PainterPathElement`. Resolved 2026-06-23 — investigation showed
      MESH already renders via Skia (`skia-safe`), so these were test-only *unwired*
      capabilities, not an alternate backend. Per product decision: **dropped
      `DrawText`** (text stays in `TextRenderer`) and **hooked up the rest** rather
      than deleting:
      - `mix-blend-mode` CSS (`normal/multiply/screen`) → `ComputedStyle` → painter
        applies it to an element's background fill via an isolated `save_layer`
        (`draw_with_blend`), on path paints, and on pushed layers. Removed the old
        "unsupported blend mode" diagnostics. New "compositing" row in
        `css-coverage.md`.
      - `DrawPath` now has a real producer: checked `checkbox`/`radio` paint a
        vector checkmark/dot (`DisplayPaintContent::Checkmark`), wired into both the
        session and buffer render paths. `#[allow(dead_code)]` removed from
        `PainterPath`/`PainterBlendMode`. Pixel-level tests cover both.
- [x] `service_name_from_interface` duplicated `pub(super)` in
      `shell/service.rs:85` and `scripting/context/proxy.rs:370`. Done
      2026-07-02: moved the helper to `mesh-core-service` beside
      `canonical_interface_name`, re-exported it, and replaced both local copies
      without adding a new dependency edge (shell and scripting already depended
      on `mesh-core-service`). Verified with `mesh-core-service`,
      `mesh-core-scripting`, and `mesh-core-shell` lib tests.
- [x] `draw_icon_resolution` shim + test-only `draw_named_icon_with_registry`.
      Done 2026-06-23: inlined the `draw_icon_resolution` shim (it only added
      `GlyphAxes::default()` before delegating to
      `draw_icon_resolution_with_axes`) into its two callers and removed it.
      `draw_named_icon_with_registry` was already `#[cfg(test)]`-gated.

### Migration tech-debt (flagged by project rules; verify before removing)

- [ ] Four remaining hand-written `.mesh`/`.luau` source mini-parsers in
      `installed_graph.rs:~908-1051` (`extract_t_keys_from_mesh_source`,
      `extract_mesh_event_publish_channels`, `extract_backend_emit_event_names`,
      `extract_keybind_subscriptions_from_mesh_source`). Progress 2026-07-02:
      `extract_icon_names_from_mesh_source` now uses the existing `.mesh`
      template AST (`parse_component` + `TemplateNode`) and walks elements,
      conditionals, loops, and component children instead of scanning strings.
      Project policy calls hand-rolled script string-parsing temporary migration
      code; migrate to AST-based analysis when the parser matures. Note:
      fixed 2026-07-01: `extract_keybind_subscriptions_from_mesh_source` now scans
      tag boundaries quote-aware, so `<`/`>` inside other attributes no longer
      hide `onkeybind`; AST-based migration remains open.
- [x] Backend lifecycle `init`/`onRender` fallbacks. Done 2026-07-02: confirmed
      every shipped backend (`modules/backend/*/src/main.luau`) already used
      `start(self)`, and found one shipped frontend straggler still on
      `onRender` (`modules/frontend/debug-inspector/src/main.mesh`) — migrated
      it to `render()`. Dropped the legacy-name fallback branch from
      `BackendScriptContext::call_init` (`scripting/backend/runtime.rs`, now
      requires `start`) and from `ScriptContext::call_render_lifecycle`
      (`scripting/context/runtime.rs`, now only recognizes `render`); updated
      `crate::shell::component::runtime::call_runtime_render_hook`'s
      `has_handler` gate to match. `is_reserved_backend_hook` still lists
      `"init"` defensively (keeps a stray `init` out of the public-function
      diagnostics listing even though it's no longer callable). Full
      workspace test suite passes at the pre-existing baseline (only the
      known `shipped_audio_style_fixture_resolves_painter_relevant_values`
      failure remains, plus `mesh-core-animation` unit tests which don't
      compile against current `ComputedStyle`/`AnimatableStyle` — both
      pre-existing, unrelated to this change).
- [x] `BackendScriptContext` / `ScriptContext` constructor explosion: ~5 `new_*`
      convenience constructors each chaining to the full one; production uses one.
      Done 2026-07-02: backend shorthand constructors (`new`,
      `new_with_settings`, `new_with_capabilities`, `new_with_storage_root`) are
      `#[cfg(test)]`, the production constructor remains
      `new_with_settings_and_capabilities`, and the storage-root frontend
      constructor now routes through a private initializer with the public
      storage-root variant test-only. Verified with
      `nix develop --command cargo test -p mesh-core-scripting --lib` and
      `nix develop --command cargo test -p mesh-core-backend --lib`.
- [x] `ModuleKind::{FontPack,Library} => ModuleType::Widget` lossy conversion
      (`module/package/module_manifest.rs:~544`). Done 2026-07-02: extended the
      legacy `ModuleType` with `FontPack`, `Library`, and `Component`, mapped
      canonical `ModuleKind` values directly, and updated frontend acceptance so
      `Component` remains embeddable while font packs/libraries no longer collapse
      into widgets. Added a conversion regression test and updated shipped popover
      assertions to expect `ModuleType::Component`. Verified with
      `nix develop --command cargo test -p mesh-core-module --lib`,
      `nix develop --command cargo test -p mesh-core-frontend --lib`, and
      `nix develop --command cargo test -p mesh-core-shell --lib`.

---

## Performance — remaining open items

Items owned by a milestone are listed with their milestone reference.

### P0 — scheduling and invalidation (→ v1.18 / v1.19)

- [x] Replace fixed 16ms shell loop sleep with event/deadline-driven scheduler. Done 2026-06-20: the Wayland shell loop now sleeps until computed deadlines or fd wakeups instead of forcing a 16ms idle cadence; `wait_for_events` polls Wayland + backend/IPC eventfd together so service and IPC messages interrupt long waits; component ticks can publish precise deadlines, with tooltip delay/fade using that path; Linux config/theme/source reloads wake through inotify instead of fixed short polling. The dev-window fallback blocks on eventfd when no minifb windows are open, and uses a 16ms pump only while minifb windows exist because minifb exposes no blocking event fd.
- [x] Stop broadcasting every backend service event to every component. Done: `Shell::deliver_service_event` gates delivery through `ShellComponent::observes_service_event`; frontend runtimes observe state updates only when their interface proxies have tracked fields or subscribed events, and named interface events only when that exact service/event subscription exists. `handle_service_event` then compares tracked field values before invalidating render state. Covered by `frontend_component_observes_only_subscribed_interface_events`, `frontend_component_keeps_service_updates_for_subscribed_event_services_only`, and tracked-field invalidation tests.
- [ ] Narrow script/service invalidation below tree-rebuild + pixel repaint; add typed state dependencies → v1.18
- [ ] Avoid full-tree restyle for safe interaction changes; use selector-dependency analysis → v1.18

### P0 — scripting (→ v1.17)

- [ ] One `mlua::Lua` VM per ScriptContext (`runtime.rs:92`); move to per-thread VM with `_ENV` isolation → v1.17
- [x] Bound instance proxy deep-clone removal. Done: live `bind:this` component bindings use a shared-VM metatable proxy over the child `_ENV`, so reads/writes and event channels no longer marshal a full JSON snapshot. Rust-side template state access uses `runtime_state()`'s mutation-generation-keyed `Arc<ScriptState>` cache, cloning only after an observable mutation rather than on every mount/read.
- [x] Remove tracked-field and side-channel map clones from state sync. Done: tracked service fields, interface subscriptions, published events, diagnostics, element actions, and storage tracking are shared through `Arc<Mutex<_>>`; installed proxies mutate the shared maps directly and `sync_side_channels` drains queued vectors/sets in place. Snapshot-returning getters remain only as explicit inspection APIs, not in the state-sync hot path.

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
- [x] Audit the other exec-polling backends. Backlog sync 2026-07-02: PulseAudio's
      old 2× `pactl` at 100ms path was already fixed in `e5223dc4` — it now uses
      `pactl subscribe`, a 1s safety poll while subscribed, and a 250ms fallback
      only when the stream cannot start. Other shipped polling backends use one
      command path at intervals of 500ms or slower. Added
      `bundled_pulseaudio_backend_does_not_restore_high_frequency_exec_polling`
      to guard the event-stream subscription and minimum fallback interval.

### P1 — presentation and memory (→ v1.20)

- [x] Preserve surface configuration state. Done: each `SurfaceTarget` retains `last_surface_config`; the render path compares all scalar layer-surface fields first and only constructs/clones `LayerSurfaceConfig` (including the namespace string) and calls presentation when configuration changed. Hidden surfaces clear the retained value so remapping still performs a fresh configure, while popup targets continue through their separate `PopupConfig` path.
- [x] Track damage as multiple rects deeper into the retained renderer. Done
      2026-07-02: `RetainedDisplayList` now preserves clipped, coalesced damage
      bounds for changed/added/removed entries instead of exposing only their
      bounding union. Sparse damage is bounded at 16 rects before union fallback,
      fed directly into the shell's existing multi-rect command filtering, pixel
      paint, SHM upload, and Wayland `damage_buffer` path. The legacy bounding
      metric remains for aggregate profiling. Covered by
      `display_list_preserves_disjoint_changed_entry_damage_rects`; full render
      (145 tests) and shell (390 tests) suites pass.
- [ ] Add performance profiles for canonical shell workloads (idle, pointer move, text update, scroll, icon grid, animation, theme reload, resize) → v1.21
- [x] Send `wl_surface::set_opaque_region` from the present path; compute union of fully-opaque background rects from retained display list. Backlog sync 2026-06-20: already wired — shell render computes the root opaque rect and presentation forwards it to `wl_surface::set_opaque_region`.
- [x] Wire `wp_blur_v1` / `org_kde_kwin_blur_v1` for backdrop-filter blur regions. Backlog sync 2026-06-20: already wired — shell render computes blur regions and the Wayland presentation backend stores/commits them through the compositor blur protocol when available.
- [x] HiDPI: plumb `wp_fractional_scale_v1` + `wp_viewporter`; render at native pixel density. Backlog sync 2026-06-20: already wired — Wayland surfaces bind fractional-scale + viewporter protocols, scale buffers to physical pixels, and set viewport destinations for fractional outputs.

### P2 — architecture

- [ ] Introduce interned `Symbol` / `TagId` types before further string-key cleanups → v1.23
- [ ] Add allocator-level profile mode (allocation counts per render pass) → v1.23
- [ ] Consider typed runtime node representation for hot paths (`WidgetNode` tag/attrs/content as strings today) → v1.23
- [ ] GPU rendering — after retained layout, smart invalidation, and damage tracking ship → v1.25
