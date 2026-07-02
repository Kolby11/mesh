# MESH — Active Backlog

Items marked `→ vX.Y` are tracked as GSD milestones in `.planning/ROADMAP.md`.

---

## Shell features

- [x] **TOP PRIORITY — finish promoted nav popover polish and remove manifest surface geometry.** Investigated 2026-07-01: `language-popover`/`theme-selector` were already migrated to `mesh.kind: "component"` with no `mesh.surface` block (anchor/layer/width/height/keyboard_mode/display_transition) — that part of this item was already done by an earlier pass (`9305df00`, `35a045a1`) and is verified by `shipped_tiny_nav_popovers_are_embeddable_components_without_surface_geometry`. No `offset-y`/`offset-x` markup nudges exist anywhere in the module tree — placement is already anchor-rect + CSS only (`anchor-ref` resolves the trigger's real measured layout box in `collect_child_surface_requests`/`popover_anchor_bounds`, `shell/component/shell_component.rs:1750-1884`). Horizontal anchoring: `anchor="bottom" gravity="bottom"` maps 1:1 to `xdg_positioner::{Anchor,Gravity}::Bottom` with no `Left`/`Right` bit (`shell/runtime/render.rs:953-978`, `presentation/src/wayland_surface/popup.rs:126-155`), which per the `xdg_positioner` protocol centers the popup horizontally on the anchor point — code-reviewed as spec-correct; not independently re-verified against a live compositor in this pass (no Wayland session in this environment). The real, confirmed gap was close/dismiss + exit-animation: closing child popovers were torn down (`destroy_child_surface_at`) the instant `open` flipped false, before their own CSS `.mesh-surface-exiting` transition (already authored in both components' `<style>`) ever got a chance to apply or run. Fixed: `ChildSurface` now carries a `closing_until` grace deadline sized from the popover's own resolved transition duration (`child_hide_transition_ms`); the shell keeps repainting/presenting the closing popup and calls a new `ShellComponent::set_closing_child_keys` so `finalize_tree` scopes `mesh-surface-exiting` to just that popover's subtree (not the whole tree) before style resolution runs, so the existing per-node CSS transition engine actually animates it — then tears the popup down once the deadline passes (or cancels cleanly if the popover reopens first). Tests: `child_surface_reconcile_plays_exit_transition_before_teardown`, `child_surface_reopen_cancels_pending_exit_transition`, `set_closing_child_keys_scopes_exit_transition_to_popover_subtree_only` (real `@mesh/theme-selector` component, asserts the class is applied/removed and a real transition starts). Also removed `theme-selector/src/components/bubble-burst.mesh`, a decorative burst animation that was never wired into `theme-selector/src/main.mesh` or referenced anywhere (dead code, not the actual bubble launch animation — that's the already-working `bubble-options.mesh` fan-out CSS). Remaining/deferred: visual confirmation of horizontal centering on a live compositor; entrance (`mesh-surface-entering`) is not yet similarly scoped for child popovers (only exit was the confirmed-broken path); the `module.json` "embeddable component, no surface geometry" manifest-shape decision below is a broader follow-on (multi-module design), left open.
- [x] **Larger design step — module-declared component variables.** Design completed in [`docs/spec/03-components.md`](docs/spec/03-components.md). Decision: configuration belongs to the component's `.mesh` source as a typed `<props>` public API rather than `module.json`; packaging stays in the manifest. The design specifies types/defaults, `prop(name)` CSS projection, reactive `props.name` script projection, generated settings UI, global/instance/per-instance persistence and precedence, validation/LSP diagnostics, i18n labels, token/icon integration, and the boundary between CSS/content sizing and top-level Wayland placement. Implementation remains phased in that document.
- [x] Icon rendering using icon packs — XDG resolution and SVG rasterization pipeline. Done: the full chain (`<icon>` → `DisplayPaintContent::Icon` → `render_display_icon_node` → registry/XDG/pack resolution → resvg/image raster + caches → blit, with built-in missing-icon fallback) was already implemented and unit-tested; added an end-to-end pixel-level proof on a real shipped surface (`shipped_navigation_icon_rasterizes_pixels_on_real_surface` in `real_surfaces.rs`) that paints `@mesh/navigation-bar` and asserts the volume `<icon>` rasterizes non-transparent pixels within its layout box. Follow-up resolved 2026-06-22: the off-buffer overflow (icon at x≈1978 on a 960px paint) was a **test artifact**, not a real layout bug — the proof's `audio_network_catalog` omitted `mesh.hyprland`/`mesh.power`, so `WorkspaceList`/`WindowTitle`/`BatteryButton` rendered unbounded ~700px error-string placeholders that inflated the bar past its intrinsic width. Switched the proof to `navigation_bar_catalog()` (all six consumed interfaces present), paints at a realistic 1280px panel width, and now asserts the right `.right-cluster` control cluster (and the volume icon inside it) stay on-buffer — turning the observation into a regression guard. Production content is already bounded (`window-title-row { max-width: 240px }`, small icons/pills). Robustness follow-up completed 2026-07-02: generated component-error boxes and text now carry a core marker whose post-restyle constraints cap them at 320px, allow flex shrink, clip overflow, and render a single-line ellipsis. This prevents one broken embedded module from expanding its host surface; covered by `generated_error_placeholder_is_bounded_after_restyle_constraints`.
- [x] Layer system — specify which Wayland layer (background/bottom/top/overlay) a surface targets; needed for proper popover/overlay stacking. Backlog sync 2026-06-20: already wired through manifest/settings surface layout (`mesh.surface.layer`), shell surface config, and layer-shell presentation backend.
- [x] Positioning system — `position: relative / absolute / fixed` in layout and paint; needed for tooltips, context menus, dropdowns. Backlog sync 2026-06-20: style parsing/resolution, layout insets, fixed-position viewport anchoring, retained display list, and painter handling are all present with focused layout tests.
- [ ] Settings module — surface for managing installed modules, active providers, theme, i18n → v1.22
- [ ] Popups / overlays — transient surfaces with custom content and dismiss behavior → v1.22
- [ ] Clean up backend modules and interfaces — consider moving the interface contract declaration from the separate `modules/interfaces/` directory into the implementing backend module, or bundling it as core metadata; evaluate impact on multi-provider resolution before changing

### Module architecture friction redesign — 2026-06-19

Brainstorm + decision record in `docs/design-architecture.md` (folded into `docs/spec/01-module-system.md`).
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

---

## Performance improvements — 2026-07-02 deep scan

Findings from a full-codebase performance scan (data handling, component
communication, events, rendering) motivated by the gap to QtQuick/webview-class
shells. Each item cites `file:line` as of this scan; reverify before editing.
Items that overlap an existing milestone entry above say so instead of
duplicating it.

### A. Data handling — Rust ↔ Lua boundary is JSON-shaped and clone-heavy

- [ ] **Full `ScriptState` clone per state mutation.** `runtime_state()`
      (`shell/component/runtime.rs:119-134`) clones the entire
      `HashMap<String, serde_json::Value>` whenever `mutation_generation`
      advanced — i.e. after *every* handler/render-hook that wrote any
      variable, per component instance, per build. The mutation-generation
      cache only helps the no-change case. Move toward persistent/COW state
      (e.g. `im::HashMap`, or `Arc<Value>` per variable so the clone is
      shallow), or let template eval read `ScriptState` directly instead of
      through a snapshot clone.
- [ ] **`ScriptState::get` clones a `serde_json::Value` per template read**
      (`scripting/context/state.rs:210-217`). Hot template expressions clone
      whole subtrees; when a template touches `elements`/`refs` this clones the
      entire per-frame metrics object. Return `Option<&Value>`/`Arc<Value>`
      through `VariableStore` (the trait already forces owned values —
      `ui/elements` `VariableStore::get`), and fix `keys()`'s `Vec<String>` +
      O(n²) `contains` merge (`state.rs:219-229`).
- [ ] **Deep JSON equality on every host write.** `set`/`set_host_value` run
      `reactive_values_equal` (full deep `Value == Value`) per write
      (`state.rs:79-115`). For the per-paint `elements` object this is an
      O(tree) deep compare every frame. Combine with the metrics fix below;
      for scalar writes keep it, for known-big host values use a
      generation/hash check instead.
- [ ] **Per-paint element metrics: build → deep-compare → JSON→Lua convert,
      every frame.** `publish_element_metrics`
      (`shell/component/interaction_state.rs:41-65`) serializes *every keyed
      node* to a `serde_json::Map` per paint, `set_host_value` deep-compares
      it, then `apply_element_metrics`
      (`scripting/context/runtime.rs:414-428`) converts the whole object to a
      Lua table **and** reinstalls bound element proxies — per frame, even
      when nothing scripted reads geometry that frame. Make `refs.<name>`
      reads lazy: keep metrics in a Rust-side store and resolve fields on
      `__index` (the proxy machinery already exists in `element_ref.rs`),
      publishing only a generation bump per paint; drop the eager
      `elements`/`refs` state tables or gate them on actual template reads.
- [ ] **Service payloads convert JSON→Lua per runtime per event.**
      `apply_service_payload` (`scripting/context/runtime.rs:388-406`) runs
      `lua.to_value(payload)` + `refresh_module_object()` for every runtime in
      every observing component on every backend emission; the shell also
      clones the payload into `cached_service_payloads` per event
      (`shell_component.rs:187-189`). Convert once per surface VM (runtimes
      sharing a `SurfaceVm` can share the converted table) and cache the
      module-object refresh.
- [ ] **Stringly-typed template expression values.** `eval_expr` returns
      `String` for everything (`frontend/compiler/src/expr.rs:26,162`);
      numeric ops re-`parse::<f64>` both sides per evaluation
      (`expr.rs:197`), `if` conditions compare against `"false"|"nil"|""|"0"`
      string literals, and every result is stored as an attribute `String`
      that downstream code re-parses. Introduce a small typed value enum
      (bool/number/string) for compiled-expression evaluation and only
      stringify at the attribute boundary — this also removes false
      attribute-hash dirtiness from float formatting.

### B. Component communication & input

- [ ] **Full widget-tree deep clone on every input event.**
      `handle_component_input` starts with `self.last_tree.clone()`
      (`shell/component/input/mod.rs:32-35`) — a recursive clone of every
      `WidgetNode` (two `BTreeMap<String,String>`s, style, children) at
      pointer-motion frequency. `apply_element_actions` does the same
      (`interaction_state.rs:78`). The clone exists only to appease borrows
      while handlers run against `&self`. Restructure so input reads a
      borrowed/`Arc`'d tree (e.g. keep `last_tree: Option<Arc<WidgetNode>>`,
      clone-on-write only when a handler actually mutates retained state), or
      split hit-test data (key, bounds, handlers) into a slim side structure
      built once per paint and let input run against that instead of the full
      tree.
- [ ] **Hit-testing re-walks the tree per pointer motion.**
      `find_node_path_at` plus up to two `find_tooltip_by_key` walks and a
      `find_node_bounds_by_key` walk run per `PointerMove`
      (`input/mod.rs:195-226`). Build a flat hit-test index (sorted rects +
      parent links, from the same pass that publishes element metrics) per
      paint and answer motion queries from it; also lets the tree clone above
      die for the hover path.
- [ ] **Hover diff materializes descendant key sets as `HashSet<String>`.**
      `collect_interaction_changed_keys` clones every affected `_mesh_key`
      String and walks subtrees per hover change
      (`shell/component/rendering.rs:440-471`); `annotate_runtime_tree`
      re-`format!`s the `root/0/2/...` path key and re-FNVs it into `node.id`
      for every node every frame (`runtime_tree.rs:616-622,746`). Store the
      runtime key/id as typed fields computed once per retained node
      (overlaps the v1.27 "typed WidgetNode fields" item; the key-path
      allocation itself is not yet tracked anywhere).
- [ ] **Handler dispatch overhead per event.** `call_namespaced_handler`
      locks the runtimes mutex, allocates 3 Strings for namespacing, and
      unconditionally runs `resync_binding_neighbors` over every linked
      instance after each handler (`shell/component/runtime.rs:494-560`).
      Track "did a cross-`_ENV` write actually happen" (a dirty bit set by
      the live-binding `__newindex`) and skip neighbor resync when clean;
      intern instance keys.
- [ ] **`bind:this`/live-binding writes mark the whole surface dirty.** Any
      handler that touches state invalidates via `invalidate_script_state()`
      → full template re-eval + tree rebuild (narrow path still rebuilds the
      full tree first — see v1.27 item). The typed state-dependency work
      (v1.18) should extend to handler writes: record which public members a
      template actually binds and skip rebuilds for writes nothing binds to.

### C. Events & service delivery

- [x] **EventBus clones the full JSON payload per subscriber.**
      `broadcast::Sender<Event>` with `Event { channel: String, source:
      String, payload: Value }` (`foundation/events/src/lib.rs`) — every
      publish deep-clones channel/source/payload once per receiver. Wrap the
      event in `Arc<Event>` (broadcast clones are then pointer bumps) and
      intern channel names. Done 2026-07-02: `EventBus` now broadcasts
      `Arc<Event>` so subscribers receive pointer clones of one event
      allocation; channel-name interning remains a later refinement. Covered
      by `subscribers_receive_shared_event_payload`.
- [ ] **Per-event mutex churn in the observation gate.**
      `deliver_service_event` (`runtime/service_state.rs:167-184`) calls
      `observes_service_event` on every component per event, which locks that
      component's `runtimes` mutex and queries tracked-field maps
      (`shell_component.rs:261-268`); several tracked-field APIs clone whole
      maps/sets (`tracked_service_fields()`,
      `tracked_fields_for_service()` — `scripting/context/runtime.rs:478-497`).
      Maintain a shell-side subscription index (service → component indices),
      invalidated when a runtime's tracked fields/subscriptions change, so
      event routing is a lookup instead of N mutex acquisitions.
- [ ] **No coalescing of service events within one loop wake.** Each backend
      emission is delivered and invalidates independently
      (`runtime/mod.rs:323`); a chatty backend (e.g. volume drag feedback)
      can trigger several rebuild-invalidations between two paints. Coalesce
      queued `ServiceEvent::Updated` per service before delivery (keep only
      the newest payload per service per wake; named interface events stay
      ordered).
- [ ] **Backend providers still exec-poll by default.** `spawn_backend_service`
      drives a tokio interval that re-runs `exec` subprocesses per tick
      (`runtime/backend/src/lib.rs:157-…`). The pipewire item above tracks one
      backend; the generic gap is push-based host API primitives (D-Bus
      signal subscribe, fd/socket watch, `pw-dump --monitor`-style stream
      adoption) so providers can be event-driven and the safety poll becomes
      the fallback, not the mechanism.

### D. Rendering & per-frame work

- [ ] **Fractional HiDPI forces full-surface repaint every frame.** `paint`
      sets `surface_pixels_invalid = true` whenever `scale` is non-integer
      (`shell/component/shell_component.rs:460-462`), so on a 1.25/1.5×
      output *every* frame is a full clear+repaint+full-damage present —
      partial damage only exists at integer scales. Fix the underlying
      logical-vs-physical damage-clip mismatch (compute/clip damage in
      physical pixels through the painter) so the retained partial path works
      at fractional scale. Likely the single biggest win on fractional-scale
      setups.
- [ ] **Per-frame full-tree fingerprinting even when clean.**
      `RetainedWidgetTree::update` re-walks every node, hashes ~50 style
      fields + every attribute/handler string (FNV byte-at-a-time), allocates
      a snapshot (with a `child_ids` Vec) per node into a scratch map, and
      clones snapshots on change (`runtime_tree.rs:98-163,293-392`). The
      v1.27 "generation-aware diff" item covers skipping clean subtrees; add:
      hash with a word-at-a-time hasher (fxhash), reuse snapshot allocations
      in place (index by slotmap key instead of rebuilding the `NodeId` map),
      and stop hashing shell-owned annotation attributes that already have
      typed change tracking (`_mesh_scroll_*`, `_mesh_key`).
- [ ] **`WidgetNode` allocation profile.** Every node carries `tag: String`,
      `attributes: BTreeMap<String,String>`, `event_handlers:
      BTreeMap<String,String>` (`ui/elements/src/tree.rs:44-68`), rebuilt
      from the template on every script invalidation and deep-cloned by the
      input path. Interning (v1.23) plus a small-map type (attrs are
      typically <8 entries; `Vec<(Symbol, CompactString)>` beats a BTreeMap)
      and moving shell annotations (`_mesh_key`, scroll offsets, focus flags,
      selection coords — currently formatted floats in string attributes,
      `runtime_tree.rs:729-743`, `rendering.rs:697-728`) to typed fields
      would shrink both build and diff cost. Overlaps v1.23/v1.27; listed
      here because the *authoring* of new annotations keeps growing the
      string surface.
- [ ] **`finalize_tree` runs ~8 full-tree walks per finalized frame** beyond
      the annotation fuse already tracked (v1.27): `annotate_runtime_tree`,
      `append_class_recursive` (exit/enter classes), `annotate_surface_shortcuts`,
      `annotate_overflow_tree`, `merge_runtime_primitive_defaults`,
      `collapse_promoted_popover_wrappers`, `constrain_error_placeholders`,
      `annotate_selection_tree` (`shell/component/rendering.rs:238-432`).
      Several are only relevant when a feature is active (no popovers → no
      collapse walk; no selection → skip). Gate the conditional walks on
      cheap presence flags and fold the unconditional ones into the fused
      traversal.
- [ ] **Restyle re-applies string declarations per node on interaction
      frames** — tracked (v1.23 typed declarations + v1.18 selector
      dependencies). New detail from this scan: the interaction narrow path
      falls back to *full-tree* restyle whenever `affected_keys` is empty
      (`rendering.rs:314-329`) including the common pointer-leave case; make
      the empty-diff case a no-op restyle instead of a full pass when
      previous state exists.
- [ ] **Layout + display list**: Taffy tree rebuilt per layout pass and
      display-list subtree flattening per update are already tracked
      (v1.21). Reaffirmed as the dominant structural-frame costs behind
      restyle in this scan; no new sub-findings.
- [ ] **CPU Skia raster + SHM is the ceiling.** Painting is skia-safe CPU
      raster into `PixelBuffer` + SHM upload (`render/src/surface/painter/backend.rs`);
      blur/shadows/gradients are CPU per damaged pixel. GPU rendering is
      deferred (v1.25) — when it lands, prefer a `wgpu`/Skia-GPU surface per
      output with the retained display list as the command source, and keep
      SHM as fallback. Until then, the damage-path fixes above (especially
      fractional scale) are the effective lever.
- [ ] **Present pipeline recomputes derived regions every visible frame.**
      `present_surface_target` recomputes opaque rect, input region, and blur
      region from the display list on every present
      (`shell/runtime/render.rs:804-837`) and issues
      `update_input_region`/`update_opaque_region` unconditionally; cache the
      last-sent regions per surface and skip the recompute+protocol calls
      when the display-list generation didn't change.
- [ ] **Child popover reconcile allocates per frame.**
      `reconcile_child_surface_requests` re-derives requests, builds
      `HashSet<String>`s of node keys, hex-encodes child surface ids, and
      clones `PopupConfig`s every frame for every component
      (`shell/runtime/render.rs:386-478,970-977`), even for the common
      zero-popover case. Early-out when the component has no popover nodes
      and no live children; cache the encoded child surface id on the
      `ChildSurface`.

### E. Style system — second-pass findings

- [x] **`StyleRuleIndex` rebuilt per node on the tree-build path.** The
      restyle path caches the selector index (`cached_style_rule_index`), but
      the *build* path — `build_element_node` →
      `resolve_node_style_for_module` →
      `resolve_node_style_with_attrs_no_diagnostics` — constructs
      `StyleRuleIndex::new(rules)` **for every node**
      (`ui/elements/src/style/resolve.rs:538`, called from
      `frontend/compiler/src/render.rs:535`). Every full tree rebuild pays
      O(nodes × rules) just building throwaway indexes. Additionally
      `inherited_style_mask(rules, …)` re-scans the full rule list per node
      (`render.rs:528-529`). Build one index per tree build and thread it
      (plus the inherit mask) through `build_widget_node`. Done 2026-07-02:
      `BuildStyleContext` now builds one `StyleRuleIndex` per component tree
      build and threads it through recursive `build_widget_node` calls via a
      new indexed resolver entry point; inherited-style masks continue through
      their existing cached helper. Covered by
      `indexed_module_style_resolution_matches_non_indexed_resolution` and
      the `mesh-core-frontend` render suite.
- [ ] **Every declaration resolves through a String round-trip.** Theme
      tokens are stored as `TokenValue::Number` but resolution formats them
      (`format!("{n}")`, `resolve.rs:402`) and downstream re-parses
      (`parse_px`, `Color::from_hex` — `resolve.rs:446-461`); `var()`
      resolution walks embedded-reference string substitution per value.
      This is the inner loop of both build and restyle. Extends the v1.23
      typed-declaration item: resolve tokens to typed values
      (`Color`/`f32`/enum) once per theme load and make
      `apply_declaration` consume typed values, keeping strings only for
      diagnostics.
- [ ] **Theme component defaults re-applied per node from string maps.**
      `apply_theme_component_defaults` parses `HashMap<String, String>`
      defaults on every node resolution (already visible in the
      post-2026-06-10 toggle profile note above). Pre-bake per-tag
      `ComputedStyle` prototypes once per theme change and start resolution
      from a memcpy of the prototype instead of re-applying string
      declarations.
- [x] **`surface_css_props()` recomputed at least twice per paint.** Both
      `finalize_tree` (`rendering.rs:291`) and
      `apply_style_animations_with_previous` (`animation.rs:108`) call it;
      each call clones the runtime state (`runtime_state()`) and rebuilds the
      props map. Compute once per paint and pass it down; invalidate on
      props/state change. Done 2026-07-02: `paint` now computes the surface
      prop map once and passes it through rebuild, narrow-script rebuild,
      retained restyle, finalize, and style animation paths; direct
      `build_tree`/test helpers keep a compatibility wrapper that computes
      the map once for that standalone call.

### F. Animation & layout per-frame overhead

- [x] **Whole-tree animation bookkeeping runs even with zero animations.**
      Every paint collects `previous_visual_styles()` — a
      `HashMap<String, AnimatableStyle>` keyed by cloned `_mesh_key` strings
      for the entire tree (`animation.rs:88,380-388`) — then
      `apply_style_animations_to_node` walks every node again with a fresh
      `StyleResolver`. Components with no `transition`/`animation` CSS should
      skip both walks entirely (a per-component "has animatable rules" flag
      computed at compile time); with animations active, track only nodes
      that have transitions declared instead of snapshotting every node.
      Done 2026-07-02: `FrontendSurfaceComponent` now records whether the
      parsed root/local component styles contain transition/animation
      declarations or keyframes. `paint` skips both `previous_visual_styles`
      and the animation traversal when that flag is false and there are no
      active transition/keyframe maps. The gate stays conservative for
      components that declare any animatable CSS or are already animating;
      per-node transition tracking remains a later refinement. Covered by
      `detects_animatable_style_rules_from_declarations_and_keyframes`.
- [ ] **Retained Taffy layout still re-syncs every node's style per pass.**
      `compute_incremental` → `update_retained_node_styles` walks the whole
      tree rebuilding `taffy_style_for_node` and re-populating
      `node_map`/`text_nodes` HashMaps on every layout-dirty frame
      (`ui/elements/src/layout.rs:346-390`), even when one node changed.
      Feed the retained-tree dirty set (already computed in
      `RetainedWidgetTree::update`) into layout so only dirty nodes get
      `set_style` calls — Taffy caches internally, but MESH pays the full
      style-conversion walk. (Structural rebuild case is tracked at v1.21;
      this is the *non-structural* per-frame cost.)

### G. Lua runtime — state sync & handler overhead

- [ ] **`sync_state_from_lua` converts every user global per handler call.**
      The "fast path" still reads and Lua→JSON-converts *every known user
      global* (changed or not), deep-compares each in `state.set`, **and**
      runs a full `_ENV` `pairs()` scan to discover newly-created globals —
      after every event handler and render hook
      (`scripting/context/runtime.rs:1466-1510`). Replace polling with a
      write log: the component `_ENV` already goes through a metatable for
      live bindings — record written keys in `__newindex` and sync only
      those; run the discovery scan only when the write log saw an unknown
      key.
- [ ] **Handler-call side channels drain through multiple mutexes per call.**
      `sync_side_channels` locks published-events, diagnostics, element
      actions, storage-tracking mutexes sequentially per handler invocation
      (`runtime.rs:1533+`). Mostly-empty in steady state — swap to a single
      shared "any pending" atomic flag checked before taking any lock.
- [ ] **Embedded component handler values serialize JSON into attribute
      strings.** `build_component_ref` encodes handler-call props as
      `serde_json::json!({"h":…,"a":…}).to_string()` stored in a node
      attribute and re-parsed at dispatch (`frontend/compiler/src/render.rs:666-676`).
      This also churns the attribute hash. Store structured handler bindings
      on `WidgetNode` (typed field) instead of JSON-in-a-string.

### H. Presentation & memory

- [ ] **Extra full-buffer memcpy per present.** Skia paints into
      `PixelBuffer`, then `copy_bgra_to_canvas`/`copy_bgra_damage_to_canvas`
      memcpys into the SHM mapping (`presentation/src/wayland_surface/backend.rs:514-646`).
      The damage-scoped copy path is good, but full-present frames (first
      paint, resize, fractional scale until fixed) pay paint + full copy.
      Have Skia render directly into the mapped SHM canvas
      (`with_skia_canvas` over the pool slot) for the active buffer,
      keeping `PixelBuffer` only as the retained/compare copy — or adopt
      double-buffered direct paint once damage tracking is per-buffer.
- [ ] **SHM pool thrash on resize.** Any size change clears and re-creates
      all `SHM_BUFFER_POOL_DEPTH` buffers (`backend.rs:251-260`). A
      content-measured surface that animates its size (expanding popover,
      growing launcher list) reallocates the whole buffer set every frame.
      Round buffer allocation up to size classes (e.g. next-64px) and
      present with viewport crop, so gradual resizes reuse allocations.
- [ ] **Startup compiles modules serially.** Module discovery + `.mesh`
      parse + compile runs one directory at a time on the main thread
      (`shell/discovery.rs:126+`). Parse/compile are pure per-module —
      parallelize with rayon/spawn_blocking to cut shell start latency
      (matters for session startup perception vs. quickshell).

### I. Composition, display list & proxies — third-pass findings

- [ ] **No component-level render memoization — the strategic gap.** Every
      surface rebuild re-evaluates *every* embedded/local component's
      template from scratch: `render_import`
      (`shell/component/composition.rs:12-100`) re-clones props into a fresh
      `HashMap<String, serde_json::Value>`, re-`format!`s instance keys,
      re-runs `bind_child_instance`, and re-renders the child subtree even
      when that instance's props and script state are untouched. This is why
      one reactive variable changing anywhere re-costs the whole surface.
      Each `EmbeddedFrontendRuntime` already has a
      `ScriptState::mutation_generation`; cache each instance's built
      subtree keyed by (props fingerprint, state generation, locale/theme
      generation) and reuse it wholesale on rebuild. This is the
      component-granular complement to the v1.27 node-level narrow re-eval
      and probably the single largest structural win for complex surfaces.
- [ ] **Fresh `self` Lua table per lifecycle call.** `current_self_table()`
      builds a new table + metatable (module/component ids, storage proxy,
      event channels) on every lifecycle handler invocation — including
      `render(self)` per frame per instance
      (`scripting/context/runtime.rs:714-716,760+`). Build once per
      instance, cache on the context, refresh only when identity/locale
      changes.
- [ ] **Interface-proxy field reads take a mutex per Lua `__index`.** Every
      `audio.percent`-style read locks `tracked_service_fields`
      (`scripting/context/proxy.rs:108-170,316-324`) to record the tracked
      field. Steady-state render hooks re-record the same fields every
      frame. Record into a lock-free per-VM scratch (plain `RefCell` — the
      VM is single-threaded) or only record fields not already tracked
      (check with a read-optimized set).
- [ ] **Display-list rebuild allocates fresh entry maps per frame.**
      `update_inner` builds `ordered_entries: Vec` + `next: HashMap` from
      scratch every non-generation-matched update
      (`render/src/display_list.rs:760-762`) — unlike `RetainedWidgetTree`,
      which keeps a scratch map. Entry comparison also deep-compares
      per-entry cloned strings (`content`/`value`/`src`/`name` are cloned
      into every rebuilt `DisplayPaintNode`, `display_list.rs:2081-2113`).
      Reuse scratch allocations; share node text via `Arc<str>` between
      `WidgetNode` and display entries so comparison is pointer-first.
      (Subtree command arrays are already `Arc<[DisplayPaintCommand]>`, so
      reuse of clean subtrees is cheap — the waste is in the per-entry
      bookkeeping, not the commands.)
- [ ] **Storage reads clone per Lua access.** `self.storage.key` reads lock
      the storage mutex and clone the JSON value per access
      (`scripting/storage.rs:275-307`); render hooks that read storage pay
      this per frame. Minor today; becomes visible once handlers use
      storage more. Consider caching the storage table Lua-side and
      invalidating on write.
- [ ] **Keybind/shortcut annotation scans string attributes per frame.**
      `annotate_surface_shortcuts` and keybind resolution walk the tree
      matching `onkeybind`/accesskey attribute strings each finalize (part
      of the finalize-walk set in D); resolving declared keybinds to a
      compiled map at tree-build time (they cannot change between rebuilds)
      removes the per-frame scan. Fold into the fused-walk work.

### J. Algorithmic complexity — quadratic hot-path patterns (fourth pass)

Targeted scan for accidentally-super-linear loops. These compound with each
other: an uncoalesced motion event multiplied by an O(depth × n) hover dispatch
multiplied by O(n) tree clones is where interaction latency actually goes.

- [x] **Pointer-motion events are not coalesced — the multiplier on
      everything.** `dispatch_wayland_events` pops each queued event and runs
      the full input pipeline per event (`shell/runtime/wayland.rs:14-116`).
      A 1000 Hz mouse queues ~16 `PointerMove`s per 60 Hz frame; each one
      pays the full-tree clone, 5+ tree walks, and (during slider drag) a
      *full script-state rebuild* (`input/mod.rs:163-186`). Coalesce
      consecutive `PointerMove` events for the same surface down to the
      latest position before dispatch (buttons/enter/leave act as barriers)
      — standard practice in every toolkit, small diff, up to ~16× reduction
      in per-frame input work. Do the same for `Scroll` deltas (sum them).
      Done 2026-07-02: presentation input coalescing now handles both
      pointer moves and scrolls before the shell dispatch queue; moves keep
      the latest position, scrolls sum deltas and keep the latest pointer
      position, leave/button/key/char events flush the affected surface, and
      pointer/scroll transitions preserve order. Covered by
      `coalesces_scroll_deltas_for_same_surface`,
      `pointer_moves_and_scrolls_flush_each_other_in_order`, and the existing
      pointer coalescing tests.
- [ ] **Hover-transition dispatch is O(path-depth × tree).** For every key
      entering/leaving the hover path, `dispatch_hover_transition_handlers`
      runs up to two `find_event_handler` full-tree walks, then
      `build_click_event` (another full-tree bounds walk), then
      `call_node_handler` → `find_event_handler` *again*
      (`input/mod.rs:348-392`). Crossing one nested row costs ~5 full-tree
      walks per path node. One pre-walk building
      `key → (&node, handlers, bounds)` for the union of both paths makes the
      whole dispatch a single O(n) pass — or free once the per-paint
      hit-test/key index (item B) exists.
- [x] **`prune_stale_interaction_targets` clones every key to validate ~4.**
      Every paint walks the whole tree cloning every `_mesh_key` String into
      a fresh `HashSet` (`interaction_state.rs:313-338` via
      `collect_all_keys`) just to check whether `focused_key`,
      `focus_visible_key`, `hovered_key`, and the selection anchor still
      exist. Invert it: 4 × `find_node_by_key` probes (early-exit walks), or
      4 lookups against the retained tree's existing `node_keys` map. Turns
      an O(n)-allocations-per-paint pass into effectively O(1). Done
      2026-07-02: pruning now probes only the tracked focus/hover/pointer/
      slider/selection keys with `find_node_by_key` and no longer builds the
      full cloned-key `HashSet`. Covered by existing prune/selection cleanup
      tests.
- [x] **`collect_interaction_changed_keys` is O(changed-keys × tree).** For
      each hover/focus-changed key it calls `collect_descendant_keys`, which
      restarts the search *from the root* (`rendering.rs:440-471,618-632`).
      A hover change across a path of depth d scans the tree d times. Single
      walk that checks membership against the changed-key set and switches
      to collect-mode inside matched subtrees: O(n) total. Done 2026-07-02:
      replaced per-key root rescans with `collect_changed_subtree_keys`, a
      single traversal that enters affected mode at changed hover/focus keys
      and collects descendant `_mesh_key`s. Removed the now-dead
      `collect_all_keys` helper. Covered by
      `collect_changed_subtree_keys_collects_descendants_in_one_walk` and the
      restyle suite.
- [ ] **The "narrow" script path currently costs extra and saves nothing.**
      `narrow_script_update` does the *full* template rebuild, then
      `narrow_script_diff` re-snapshots and re-hashes every node
      (`runtime_tree.rs:187-217`), then `narrow_expand_ancestors` builds a
      full parent map (`rendering.rs:741-768`) — and the result only feeds
      telemetry (`affected_node_count`, `narrow_path_active`,
      `rendering.rs:202-218`); the returned tree is finalized/restyled/laid
      out in full regardless. Until the v1.27 subtree re-eval lands, this is
      pure added O(2n) hash work on the most common invalidation class
      (service updates). Either wire `full_affected` into finalize (skip
      restyle/layout outside affected subtrees) now, or gate the diff behind
      profiling mode.
- [ ] **Runtime key paths make deep trees O(n × depth).** Every node's key
      is the full slash-joined ancestor path built with
      `format!("{key}/{index}")` and FNV-hashed from scratch per node per
      frame (`runtime_tree.rs:616-622,281-291`) — a 10-deep list row hashes
      ~40-byte strings for every row every frame, and key length grows with
      depth. Derive ids by hash-chaining `(parent_id, child_index)` — O(1)
      per node, no string at all — and keep the string path only for debug
      builds / diagnostics.
- [ ] **`finalize_tree` closing-popover pass: O(closing-keys × tree)**
      `find_node_by_key_mut` per closing key (`rendering.rs:273-279`).
      Trivial count in practice; fold into the fused annotation walk (D)
      rather than fixing separately.
- [ ] **Slider drag worst case = every quadratic above at once.** Each
      uncoalesced motion during a drag runs slider-value tree walks ×3, a
      handler call (Lua + full `sync_state_from_lua`), then
      `invalidate_script_state()` → full template rebuild + restyle + layout
      + paint (`input/mod.rs:163-186`). With motion coalescing (above) plus
      routing slider drags through the STATE/interaction-restyle path
      instead of SCRIPT invalidation (the knob position is
      shell-owned state — `slider_values` — not script state), a drag frame
      should cost a targeted restyle, not a rebuild.

### K. Threading & repaint suppression (fifth pass)

MESH is effectively single-threaded for all UI work: script execution, tree
build, restyle, layout, Skia raster, and present for **every surface** run
serially inside `Shell::run` on the main thread (`shell/runtime/mod.rs:173+`).
The Tokio runtime (`runtime/mod.rs:182`) only hosts backend pollers and IPC.
QtQuick's render loop, by contrast, splits scene-graph sync from rendering.
The Lua VMs are `!Send` and must stay on the shell thread — but everything
after the display list is built does not.

- [ ] **Parallelize paint across surfaces.** After `finalize_tree`, painting
      is pure: display list + `PixelBuffer` in, pixels out. Surfaces are
      independent (own buffer, own damage). Restructure `render_components`
      into two phases — phase 1 (serial, VM-bound): script hooks, build,
      restyle, layout, display-list update per dirty surface; phase 2
      (parallel): `paint_pixel_regions` + SHM copy per surface via rayon
      scope. The painter's text/glyph/gradient caches are already
      `thread_local` (`painter/backend.rs:29`, `text.rs:28-48`), so worker
      threads get their own — verify cache hit rates don't crater with a
      pinned worker-per-surface mapping. Bar + popover + launcher painting
      concurrently roughly divides paint latency by the surface count.
- [ ] **Pipeline paint against the next frame's script work.** Even with one
      surface, phase 2 for frame N can overlap phase 1 of frame N+1 (double-
      buffer the `PixelBuffer`, hand the display list snapshot to a render
      thread, present from there). This is the classic guarded-render-loop
      design; it halves effective frame latency for rebuild-heavy frames.
      Bigger lift than per-surface parallelism — do that first.
- [ ] **Tile-parallel raster for large damage.** Within one buffer, split
      full-surface repaints (theme change, first paint, launcher open) into
      horizontal bands painted in parallel (disjoint `&mut [u8]` slices via
      `split_at_mut`; each band gets its own Skia canvas with a band clip).
      Only worth it above a damage-area threshold; measure with the v1.21
      profiles first.
- [ ] **Move blocking file IO off the shell thread.** `load_graph_i18n_catalogs`
      does `fs::read_to_string` per catalog on mount (`component/runtime.rs:136-171`),
      settings/theme reloads re-read files inline in the loop, and icon/SVG
      cache *misses* rasterize on the paint path. Route one-shot IO through
      `spawn_blocking` with a completion event (the loop already wakes on
      eventfd), and make icon-cache misses paint a placeholder frame and
      fill in on the next wake instead of stalling the frame.
- [ ] **Dedup service payloads before touching any runtime.** A poll backend
      re-emitting unchanged JSON still costs, per emission: payload clone
      into `cached_service_payloads`, `apply_service_payload` (JSON→Lua
      conversion + `refresh_module_object`) for every observing runtime, an
      `apply_service_update` deep-compare per runtime, and a tracked-fields
      scan (`shell_component.rs:184-236`). The rebuild is correctly skipped,
      but all boundary work still runs at poll frequency × runtimes. Add
      `payload == cached` short-circuit at the top of `handle_service_event`
      (and ideally shell-level in `deliver_service_event` so non-observing
      components aren't even iterated).
- [ ] **Gate interaction invalidation on rule existence.** Every hover-path
      change calls `invalidate_interaction_restyle()` unconditionally
      (`input/mod.rs:242,269`), triggering a restyle + layout + retained
      hash + display-list pass even when **no `:hover`/`:focus`/`:active`
      rule can match the affected nodes** — the common case for plain
      text/box/row nodes. At compile time, collect the set of
      (tag/class/id) keys referenced by state-dependent selectors per
      component; on hover change, skip invalidation entirely when neither
      the old nor new path intersects it. Complements the v1.18
      selector-dependency item but is much cheaper to ship: it's a presence
      check, not a dependency graph. Same gate applies to
      `prune`-style hover annotation (`_mesh_key` state bits) — nodes that
      no state rule targets don't need `state.hovered` maintained at all.
- [ ] **Skip `render()`/`render_layout` bookkeeping for clean visible
      surfaces.** `ShellComponent::render` runs per loop iteration for every
      `wants_render` component and re-applies anchor/layer/margins/keyboard
      mode to the surface struct plus a `tracing::debug!` format of node
      counts/roles (`shell_component.rs:392-424`) before paint decides
      nothing changed. Cheap individually, but it's per-frame steady-state
      work; fold it into the config-changed path (the
      `last_surface_config` compare already exists downstream).

### L. Live performance debugging — design

Goal: see hotspots *live* while interacting with the shell, with cause
attribution (which rule, which component, which invalidation), without the
measurement tool perturbing what it measures. Builds on what already exists:
`ProfilingStage` accumulators + `ProfilingSnapshot` (`runtime/profiling.rs`),
`ProfilingInvalidationSnapshot` (per-paint rebuild/retained/narrow/damage
counts), the `DebugOverlay` painter, `mesh.debug.*` IPC, and the
debug-inspector's profiling start/stop. Tiered by effort:

- [ ] **Tier 0 — Tracy live flamegraph via a feature flag.** The codebase is
      already instrumented with `tracing` spans/events throughout. Add a
      `perf-tracy` cargo feature that installs `tracing-tracy` as a layer;
      running the shell with it + the Tracy profiler UI gives live frame
      flamegraphs, per-span self-time, plots, and memory zones with ~zero
      new code. Add explicit `tracing::span!` around the missing hot spans
      first: `build_tree`, `finalize_tree` sub-walks, `restyle`, `layout`,
      `retained_tree.update`, `display_list.update`, `paint_pixel_regions`,
      `present_with_damage`, `sync_state_from_lua`, `call_handler`,
      `handle_component_input`. This is the fastest path to "where do the
      milliseconds go" and validates every item in sections A–K empirically.
- [ ] **Tier 1 — in-shell perf HUD painted by the renderer, not a module.**
      A HUD that is itself a `.mesh` surface would pollute the numbers with
      its own rebuild/restyle cycle at every update. Instead extend the
      existing `DebugOverlay` (which already paints layout bounds directly
      into the buffer post-paint, `frontend/render/src/surface/debug_overlay.rs`)
      with a profiling mode, toggled by the existing `CoreRequest` debug
      path:
      - **frame waterfall strip**: last ~120 frames as stacked bars (script /
        build / restyle / layout / display-list / paint / SHM / present),
        color-coded, 16.6 ms budget line — the data is already in
        `ProfilingSurfaceSnapshot.recent_samples`, it just needs a ring
        buffer keyed by frame rather than by stage;
      - **live counters**: FPS, presents vs skipped, damage area % of
        surface, retained-path vs full-rebuild ratio, narrow-path hits — all
        already in `ProfilingInvalidationSnapshot`, currently only visible in
        the inspector module;
      - **paint flashing** (the Chrome/KWin repaint debugger): translucent
        colored overlay on each frame's damage rects, decaying over ~300 ms.
        This makes "we repainted the whole bar for a clock tick" *visible*
        instantly, and is the single best tool for the repaint-suppression
        work in K. Trivial to add: the damage rects are already in
        `last_present_damage_rects` when the overlay paints.
      HUD paint cost must be excluded from the recorded stages (paint it
      after `PaintTraversal` is recorded) and its damage must not feed back
      into the damage stats (flag its rects).
- [ ] **Tier 2 — cause attribution (top-N tables).** Stages say *what phase*
      is slow; attribution says *why*:
      - per-style-rule cumulative restyle time + match count (time
        `apply_declaration` per rule id in the cached index; report top 10
        selectors);
      - per-component-instance build time (wrap `render_import`/embedded
        instance eval — directly measures the memoization win in I);
      - per-node paint time bucketed by command kind (text/shadow/blur/
        gradient/icon) — the painter already returns `PaintMetrics` with
        shaping/raster micros, extend to per-kind totals;
      - wasted-work counters: rebuilds whose retained diff was empty,
        restyles with zero changed styles, service deliveries whose payload
        was identical (K), motion events coalesced vs dispatched (J).
      Surface these in the HUD's second page and in the IPC snapshot.
- [ ] **Tier 3 — streaming + offline analysis.**
      - `mesh.debug.profiling_stream`: push per-frame profiling records over
        the existing IPC bus so an external `mesh-tools-cli perf top`
        TUI can show live tables without any in-shell UI (and without the
        HUD's paint cost);
      - Chrome-trace/Perfetto JSON export of a captured window (the
        `ProfilingSample` ring buffers already hold timestamps+durations) for
        offline flamegraph comparison before/after each A–K fix;
      - wire the existing `DebugBenchmarkSnapshot`/`BenchmarkScenarioSnapshot`
        types to the canonical-workload profiles item (v1.21): scripted
        scenarios (idle 10 s, pointer sweep, slider drag, popover open/close,
        theme switch) that run headless and emit a JSON summary — this is the
        regression harness that keeps the wins from A–K from rotting.
      Compare runs in CI against a stored baseline with a tolerance band.

### Suggested attack order

1. **Pointer-motion + scroll coalescing (J)** — one small diff in
   `dispatch_wayland_events`; divides all per-motion costs by the
   motion-to-frame ratio. Do this first.
2. Fractional-scale partial damage (D, first item) — biggest visible win on
   scaled outputs, bounded scope.
3. Per-node `StyleRuleIndex` rebuild on the build path (E) — turns every
   script-driven rebuild from O(nodes × rules) into O(nodes + rules); tiny
   diff.
4. Per-paint key/hit-test index (B + J) — kills the input-path tree clone,
   the 5-walk hover dispatch, and the per-paint `prune_stale` key sweep with
   one shared structure.
5. `sync_state_from_lua` write log (G) — removes per-handler full-globals
   conversion; helps every interaction.
6. Slider-drag reclassification + narrow-path gating (J) — makes drags cost
   a restyle instead of rebuild+diff overhead.
7. Element-metrics laziness (A) — removes per-paint JSON build/compare/convert.
8. Animation walk gating (F) — free win for the common no-animation surface.
9. Event routing index + payload `Arc` (C) — cheap, unblocks chatty backends.
10. Service-payload dedup + interaction rule-existence gate (K) — two small
    diffs that eliminate steady-state work at poll/hover frequency.
11. Per-surface parallel paint (K) — first threading step; needs the
    phase-split refactor of `render_components` but no new invalidation
    machinery.
12. Component-level render memoization (I) — largest structural win; plan it
    with the v1.18/v1.27 invalidation work since it shares the dependency
    bookkeeping.
13. State snapshot COW + typed expression/declaration values (A/E) — feeds
    the same invalidation work.
14. Paint/script pipelining + tile-parallel raster (K) — after the
    per-surface split proves the phase boundary; pairs naturally with the
    GPU work (v1.25).
