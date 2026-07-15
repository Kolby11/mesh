# MESH — Active Backlog

Items marked `→ vX.Y` are tracked as GSD milestones in `.planning/ROADMAP.md`.

Completed performance work, progress narratives, benchmark numbers, and
rejected experiments were archived to `docs/performance-log.md` on 2026-07-13.
Section letters (A–V) in the performance items below refer to that log.

---

## Shell features

- [ ] Settings module — surface for managing installed modules, active providers, theme, i18n → v1.22. Progress 2026-07-02: added shipped `@mesh/settings` frontend surface (`modules/frontend/settings`) with a right-overlay dialog, graph-backed installed-module list/filter, active-provider binding summary, and live theme/locale controls wired through existing `shell.set-theme` and `mesh.locale.set` paths. `@mesh/quick-settings` now exposes an Open settings action that publishes `shell.show-surface` for `@mesh/settings` and hides the quick-settings popover. The installed graph now auto-discovers the settings module and the fixture test asserts it. Remaining: write-through controls for enabling/disabling modules and switching active providers, plus full-shell render verification once the environment has the `xkbcommon` development package required by `smithay-client-toolkit`.
- [ ] Popups / overlays — transient surfaces with custom content and dismiss behavior → v1.22
- [x] Clean up backend modules and interfaces — done 2026-07-13: interface contracts are now JSON objects inside `module.json` (no TOML, no separate contract files); single-provider domains declare them inline in the backend module (`mesh.interfaces[]`, shipped for `mesh.wm` in `@mesh/hyprland-wm` and `mesh.power` in `@mesh/upower-power`), multi-provider domains keep a standalone interface module (`mesh.audio`) which always wins over inline duplicates. Contract type expressions are validated by a shared grammar at graph build (`invalid_interface_contract` / `duplicate_interface_declaration` / `missing_interface_contract` diagnostics). `mesh.hyprland` renamed to the generic `mesh.wm` (`focus_workspace`, `service.wm.*`). Backend runtimes are supervised: exponential-backoff restarts, session quarantine after 3 failed cycles, failover to next-priority provider. Legacy paths deleted (loose `interfaces/*.toml` scan, `legacy_backend_candidates_from_discovery`, stray `audio.toml`/`debug.toml`). Reserved `shell.*` channel namespace: unknown publications become diagnostics instead of phantom service commands (fixes the brightness denial spam).

### Module architecture friction redesign — 2026-06-19

Brainstorm + decision record in `docs/design-architecture.md` (folded into `docs/spec/01-module-system.md`).
Attacks authoring friction on top of the shipped interface/provider/frontend spine
(easy / unified / configurable). Selected path: **A+B headline, C/D reframed, F follow-on, E deferred.**

- [ ] **E (deferred) — Unify the 4 contribution schemas.** Theme/icons/i18n/keybinds under one `contributes` shape — only where they share honest structure; revisit after A/B land.
- Rejected: capability inference (C original) and parallel inline-interface path (D original) — both trade conceptual-simplicity for typing-simplicity, the failure mode this redesign avoids.

### Module system — remaining open follow-ups

The 2026-06-18 redesign largely shipped (canonical `module.json` with `mesh.uses`/
`mesh.provides`/`mesh.implements`, graph as single source of truth, typed graph
diagnostics for interfaces/icons/i18n/keybinds/capabilities, library modules,
resource packs). Remaining open work:

- [ ] Eliminate service-specific Rust branches where possible. Progress 2026-07-13: audio optimistic mute is now generic — contract methods declare `optimistic: { field, fromArg }` and core applies the patch for any interface (`pending_optimistic_state`); the `mesh.theme` settings-injection branch became a generic `__shell` context (`{ theme, locale }`) injected into every backend's settings. Remaining: startup-sound path calls the mesh.audio handler directly; debug/profiling paths.
- [ ] Support multiple instances of the same frontend module. Module identity should not be the only surface identity; root graph should support configured instances like two panels or repeated widgets with separate settings/storage scopes.
- [ ] Settings UI generated from contributed schemas by default, with optional custom `settings_ui` entrypoint for advanced modules.
- [ ] Settings/diagnostics UI should show each module's uses/provides graph: required interfaces, active provider, optional interfaces, required icons, native binaries, capabilities, settings namespace, i18n catalogs, keybinds, health. Progress: `mesh.debug.module_graph` payload exists and the debug-inspector Modules tab renders the first entries. Added 2026-07-02: typed graph entries and JSON include required/optional native binaries, keybind action IDs, resolved `interface=provider` pairs, and structured native-binary availability states; the Modules view renders them, correctly handles structured provided-interface records, and filters across IDs, kinds, interfaces, providers, binaries, keybinds, and diagnostics. Binary resolution is shared with installed-graph diagnostics and supports explicit executable paths as well as PATH lookup. Added later 2026-07-02: shipped `@mesh/settings` consumes the same debug graph for end-user module/provider visibility and theme/locale controls. Remaining: per-module customization controls in the full settings UI.

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

- [ ] **Shell: one component → base surface + N popup targets.** A
      `FrontendSurfaceComponent` currently maps 1:1 to a surface; popups make it
      1:N. Generalize `SurfaceId`/presentation-handle bookkeeping, per-target paint
      buffers in `runtime_tree.rs`, element-metrics publication, and input routing
      so popup input routes back to the same VM with correct popup-local coords.
      **Reframed 2026-06-23 (web-like composition):** surfaces are _containers_, not
      authoring units — one parent surface holds a component tree; in-tree
      escape-bounds nodes (`<popover open>`, later `<tooltip>`/dropdowns) are
      _transparently_ promoted to child `xdg_popup` surfaces fed by the same VM.
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

---

## Codebase cleanup — 2026-06-22 audit

Findings from a four-agent scan of the largest production files. Two batches
already landed: **confirmed dead-code deletions** (commit `afc9a0d`) and
**cross-crate/intra-crate dedup** (commit `a4125d7`). Completed items moved to
`docs/performance-log.md`.

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

---

## Performance — open items

Full history, benchmark baselines, and rejected experiments live in
`docs/performance-log.md`; section letters (A–V) below reference it. The
subsystem map is `PERFORMANCE_SECTIONS.md`. Milestone refs unchanged.

### P0 — measurement infrastructure (do before more hot-path work)

- [x] Fix the local dev environment (`xkbcommon.pc`, `freetype`, `fontconfig`
      via `nix develop`) so `mesh-core-shell` / `mesh-core-render` tests and
      in-crate benchmarks run again — verified 2026-07-14 with the shell
      regression suite and a release-only in-crate benchmark.
- [x] Canonical shell workload profiles (idle, pointer move, text update,
      scroll, icon grid, animation, theme reload, resize) → v1.21 (L tier 3
      harness; several open items say "measure with v1.21 profiles first").
      Shipped 2026-07-15: all eight are typed benchmark scenarios exposed in
      `mesh.debug`, the inspector fallback catalog, and debug-benchmark IPC,
      with stable shipped targets, stage priorities, and fresh-session hints.
      The prior hover, surface open/close, keyboard traversal, and backend
      update scenarios remain available; `pointer_update` stays stable as the
      compatibility ID for the pointer-move profile.
- [ ] L tier 1 — in-shell perf HUD via the existing `DebugOverlay`: frame
      waterfall strip, live counters, paint flashing on damage rects (L)
- [ ] L tier 2 — cause attribution: per-rule restyle time, per-instance build
      time, per-command-kind paint time, wasted-work counters (L)
- [ ] L tier 3 — `mesh.debug.profiling_stream` over IPC, Chrome-trace/Perfetto
      export, CI regression baseline with tolerance band (L). Progress
      2026-07-14: the existing `mesh.debug` service payload now exposes a
      bounded, order-stable `profiling_stream` of shell and surface samples,
      deduplicated against shell roll-ups. Chrome-trace/Perfetto-compatible
      complete events now include monotonic session timestamps, durations, and
      per-surface lanes. Remaining: CI tolerance enforcement.

### P0 — scheduling & invalidation

- [ ] Narrow script/service invalidation below tree-rebuild + pixel repaint;
      add typed state dependencies → v1.18. Extend to handler/`bind:this`
      writes: record which public members templates bind and skip rebuilds for
      writes nothing binds to (B).
- [ ] Avoid full-tree restyle for safe interaction changes; use
      selector-dependency analysis → v1.18
- [ ] One `mlua::Lua` VM per ScriptContext; move to per-thread VM with `_ENV`
      isolation → v1.17. Pairs with the handler-sync fast path below (R).

### P1 — structural render pipeline

- [ ] **Component-level render memoization (I) — largest structural win.**
      Cache each embedded/local instance's built subtree keyed by (props
      fingerprint, `ScriptState::mutation_generation`, locale/theme
      generation) and reuse it wholesale on rebuild.
      Shipped 2026-07-13 (`shell/component/memo.rs`): `render_import` now
      memoizes each imported/local instance's built subtree keyed by props
      fingerprint (props + typed handler calls), the instance's own **and
      every descendant instance's** mutation generation, theme `Arc`
      identity, locale, and container size. Build side effects are handled
      by mark counters: promoted-popover wrappers and error placeholders
      replay their presence flags on reuse; surface-portal state writes veto
      caching. Cache cleared by `reset_render_caches` (theme/locale/source
      reload). Coverage: unchanged-sibling reuse, prop invalidation,
      descendant-generation invalidation, popover-flag replay. Release
      benchmark: 200 rebuild+paint cycles of a 12-child surface measured
      212.7ms forced-miss versus 134.5ms memoized (1.6x end-to-end,
      including the untouched restyle/layout/paint stages).
      Added 2026-07-14: manifest slot contributions now share the same
      generation-aware memo contract, keyed by a stable JSON-props fingerprint;
      their `_mesh_slot_source` annotation remains per-render and portal writes
      still veto caching. Release benchmark: 200 rebuild+paint cycles of 12
      slot contributions measured 133.6ms forced-miss versus 96.1ms memoized
      (1.4x end-to-end). Slot-prop cache probes now hash the ordered manifest
      map directly rather than allocate/sort a scratch vector; 1M 24-prop
      fingerprints measured 894.2ms versus 860.8ms (~3.9% faster). Their
      fingerprint is now computed once while the immutable frontend catalog is
      built, removing that hash from every render-time slot memo probe; 1M
      24-prop probes measured 949.6ms render-time hashing versus 0.44ms for
      the precomputed read (~2162x for the eliminated operation). Embedded
      local/import/slot instance keys now use a single pre-sized allocation
      rather than `format!`; 1M representative slot keys measured 76.2ms
      versus 13.8ms (~5.5x faster). Slot catalog lookup IDs and memo
      descendant prefixes now also use one pre-sized allocation: 1M long IDs
      measured 55.1ms versus 0.44ms and 47.5ms versus 0.43ms respectively.
      Embedded local runtime IDs use the same construction, and cycle detection
      short-circuits after its second matching ancestor; a 64-entry-stack
      benchmark measured 74.2ms formatting/full scan versus 1.3ms for the
      helpers (~57.1x combined). The compiler's handler namespace construction
      now uses the pre-sized path too (83.6ms versus 13.5ms for 1M names,
      ~6.2x). Remaining:
      repeated same-alias instances share one
      runtime and always miss (needs per-occurrence instance identity — see the
      "multiple instances" module-system item); the M `BuildEffects`
      formalization still applies to future build side effects (new effects must
      add a mark counter or veto).
- [ ] Affected-subtree template re-evaluation via
      `NodeServiceFieldDependencies`; `narrow_script_update` still rebuilds
      the full tree before diffing → v1.27. Added 2026-07-14: narrow ancestor
      expansion now walks a reusable ancestor stack instead of allocating a
      full NodeId→parent map; the 1,365-node benchmark measured 78.2ms parent
      map versus 29.7ms stack walk (2.6x). An interim pass reserved the
      retained-node count for fresh narrow/layout snapshot maps. Follow-up
      2026-07-14: narrow and layout analysis now walks the retained slotmap directly
      instead of building a temporary fresh snapshot map; a same-run release
      benchmark measured 396.1ms map-based versus 317.1ms direct over 2,000
      passes (1.25x). The returned affected-node sets now reserve a capped
      initial capacity to avoid resize churn without overallocating sparse
      changes; a 4,096-node release microbenchmark measured 2.327s growing
      versus 2.238s reserved (1.04x). Service-field reverse dependencies now
      use a nested service→field index, removing two temporary String
      allocations per queried field; 1M release lookups measured 33.7ms
      tuple-key allocation versus 27.7ms borrowed lookup (1.2x).
- [ ] Generation-aware retained-tree diff: skip clean subtrees using dirty
      bits → v1.27. Remaining after landed progress: clean-subtree skipping,
      slotmap-keyed snapshot reuse (D). `RenderObjectTree` now counts visited
      nodes and skips the full retained-object stale-entry scan on clean
      non-structural updates; a 4,096-entry release benchmark measured 65.3µs
      retain scanning versus 10.7µs conditional skip (6.1x). Added
      2026-07-15: non-structural render-object sync now consumes the retained
      tree's per-node dirty index, walking clean nodes without rebuilding or
      hashing their paint data; insert/remove/reorder changes retain the full
      diff fallback. A 2,049-node release benchmark with one changed text node
      over 2,000 updates measured 511.1ms full rehash versus 44.2ms sparse
      sync (11.6x faster for render-object synchronization, including dirty-set
      membership checks). Follow-up 2026-07-15: the retained tree now publishes
      a reusable sparse `NodeId` set for non-structural downstream sync instead
      of resolving NodeId→slot and then probing the dirty secondary map for
      every visited node. Structural insertion frames skip populating the set.
      Across 13.65M sparse membership probes, release measurements were 95.9ms
      slot-indirect versus 74.5ms direct (1.3x faster).
- [ ] Triple full-tree fingerprinting on dirty frames: make
      `RetainedWidgetTree` the single fingerprint pass; render-object tree and
      display entries consume its per-node dirty flags (N). Progress
      2026-07-15: release display-entry collection now patches retained entries
      only for dirty nodes when render-object changes are limited to
      text/primitive/accessibility payloads. Material, opacity, geometry,
      transform, clip, and structural changes conservatively retain the full
      collection path. A 2,521-node release benchmark over 2,000 one-node
      patches measured 2.874s full signature collection versus 385.7ms sparse
      patching (7.5x faster for entry collection). Follow-up 2026-07-15: sparse
      updates now patch the retained entry map in place instead of copying and
      comparing every clean entry. A same-shape release benchmark measured
      469.6ms for the copied-map path versus 267.2ms in place (1.8x faster for
      sparse patching), and 2.833s for full signature collection (10.6x slower
      than the final path). Material-only updates now use the sparse path too;
      geometry, transform, clip, opacity, and structural changes retain the
      conservative fallback. A 2,521-node end-to-end display-list benchmark
      over 1,000 one-node color changes measured 4.325s full rebuild versus
      459.9ms sparse update (9.4x faster), with retained entries, damage, and
      paint commands checked against the full path. Remaining: share the
      retained tree's computed fingerprints directly instead of only its dirty
      scope, and prove any additional dirty categories before widening them.
- [ ] Any non-clean frame bypasses all generation shortcuts
      (`use_generation_shortcuts` requires an empty dirty set); widen to
      per-node dirty scoping together with the §N unification (P). Interaction
      changed-key sets now reserve path-derived capacity, and descendant nodes
      of an already-affected interaction subtree skip redundant changed-set
      hash probes. Progress 2026-07-15: the shell no longer gates downstream
      generation reuse on an entirely clean component frame. Non-structural
      render-object updates consume the retained per-node dirty index, and the
      display list always consumes the authoritative retained generation, so
      script/service invalidations that produce no visual tree change skip its
      full entry/signature scan while still honoring surface resize and forced
      full-damage policy. A 2,521-node release benchmark over 2,000 unchanged
      non-clean syncs measured 3.202s scanning entries versus 39.2µs through
      the retained-generation gate (~81,724x for the eliminated scan).
      Remaining: scope the retained widget tree's own fingerprint traversal and
      unify changed-node fingerprints across retained/render/display layers.
- [x] Fuse the remaining unconditional `finalize_tree` annotation walks into
      one traversal → v1.27 (D). Done 2026-07-15: runtime-state annotation now
      performs its existing pre-order work and overflow bounds/clamping on
      node exit, removing the second unconditional key-path traversal while
      preserving the required post-order overflow semantics. The interaction
      crate exposes a one-node overflow primitive; its standalone tree API is
      retained. A 1,365-node release benchmark over 2,000 finalization passes
      measured 271.5ms separate versus 219.2ms fused (1.2x faster). Full tree
      debug snapshots, scroll-offset results, interaction tests, and scroll-ref
      integration tests match the former path. Conditional marker walks remain
      presence-gated and selection remains a keyed lookup.
- [ ] Display-list segment/rope command storage → v1.21: stop flattening
      retained subtrees into per-ancestor copies (O(n × depth) storage and
      re-copy, N addendum); dirty parents with layout/clip/transform changes
      still force descendant command rebuilds (N addendum). Dirty-ancestor
      collection now reuses its path and ancestor-set allocations during
      retained subtree rebuilds; a release benchmark measured 6.39ms fresh
      versus 4.38ms reused over 50,000 sparse walks (1.46x). Progress
      2026-07-15: command-span metadata is now retained only as local subtree
      facts and assembled directly into one root index per update, eliminating
      the previous descendant-span vector copy at every ancestor. Equivalent
      2,521-span release construction measured 104.9ms with ancestor copying
      versus 52.1ms with single-root assembly over 1,000 passes (2.0x faster).
      Command arrays are still flattened and remain the next segment-storage
      step. Rejected 2026-07-15: retaining only local commands but eagerly
      reconstructing the compatibility root slice improved isolated flattening
      2.6x, yet regressed the one-node sparse update from 459.9ms to 603.0ms
      because it required per-node traversal/lookups. The retained baseline was
      restored and remeasured at 459.8ms; the next design must let replay consume
      segments directly instead of eagerly re-flattening them.
- [x] Retain Taffy node state across structural layout passes → v1.21. Done
      2026-07-15: production structural passes reconcile keyed nodes in the
      per-surface `TaffyTree`, preserving their `TaffyNodeId`s across insertion,
      removal, and reorder. Reconciliation now compares retained style, node
      context, and child order before calling Taffy's mutators, so unchanged
      descendants keep their cached layout instead of being dirtied. A
      1,365-node release benchmark over 200 root reorders measured 148.3ms for
      fresh `build_taffy_tree` reconstruction versus 67.5ms retained (2.2x
      faster), with identical geometry and stable existing node IDs. The fresh
      builder remains only for stateless compatibility calls and invalid-cache
      initialization.
- [ ] Child popup surfaces bypass the retained pipeline: full clear + repaint
      through the immediate-mode painter per present, plus per-frame key
      walks (P); child buffers are still repainted eagerly even though sparse
      child damage now reaches presentation (U). Route child targets through
      the retained display-list + damage path and delete the immediate-mode
      painter twins (P structure item). Progress 2026-07-13: child-surface
      reconciliation now uses borrowed entering-key slices for empty/common
      paths and checks retained child node keys by borrowed `&str` instead of
      cloning each key before requested-key membership. Release benchmark over
      8M child-key checks measured 64.3ms clone-before-check versus 6.7ms
      borrowed comparison (~9.6x faster for that subpath). Progress
      2026-07-15: promoted child buffers now cache an authoritative component
      paint generation plus exit mode, scale, and content origin. Stable
      frontend popups skip both immediate-mode raster and presentation; legacy
      components without a generation remain conservatively eager, and every
      cache-key change repaints. A 160x90 release benchmark over 10,000 stable
      frames measured 4.07ms for the formerly mandatory clear alone versus
      27.3us for the full cache-key check (~149x faster, before subtree paint and
      SHM presentation savings). Follow-up 2026-07-15: retained paint subtrees
      now carry independent generations, and promoted child surfaces key their
      paint cache from the popup subtree instead of the parent's broad display-
      list generation. Unrelated sibling/parent paint no longer repaints a
      stable popup, while popup-descendant changes still advance its generation.
      A 160x90 release lower-bound benchmark across 10,000 unrelated parent
      frames measured 4.290ms / 10,000 clears with the broad generation versus
      3.562us / 1 clear with the subtree generation (~1,204x for the gate plus
      avoided clear). Follow-up 2026-07-15: child targets now own bounded,
      origin-aware retained display lists and replay retained commands instead
      of calling the immediate tree painter. Pixel parity is covered. A
      61-node popup over 400 root-opacity transition frames measured 38.0ms
      immediate versus 30.9ms retained (1.23x); a one-descendant sparse
      material workload measured 17.5ms versus 4.83ms (3.61x). Remaining:
      consume child-local damage for partial buffer clear/raster/present rather
      than replaying the full retained command stream on every changed child
      generation.
- [x] Fractional HiDPI forces full-surface repaint every frame → v1.21. Done
      2026-07-15: logical damage is now converted to physical painter-buffer
      coordinates by flooring near edges and ceiling far edges before clear
      and clip, so fractional scales no longer require the full-paint fallback.
      The Wayland SHM/protocol path uses the same edge mapping, fixing missed
      columns for non-origin fractional rectangles. A 1.5x, 1920x1080 release
      upload benchmark over 100 frames measured 70.4ms full-buffer copying
      versus 9.5us sparse copying (7,413x faster), with the physical 37x31
      damage rect verified at both painter and presentation boundaries.

### P1 — threading (K)

- [ ] Parallelize paint across surfaces: phase-split `render_components` into
      a serial VM-bound phase and a parallel paint/SHM phase (rayon).
- [ ] Pipeline paint of frame N against script work of frame N+1
      (guarded-render-loop design; after the per-surface split).
- [ ] Tile-parallel raster for large damage (band-split full-surface
      repaints; only above a damage threshold, measure with v1.21 profiles).
- [ ] Move blocking file IO off the shell thread (i18n catalog mounts,
      settings/theme reloads, icon/SVG cache-miss rasterization on the paint
      path) via `spawn_blocking` + completion events. Progress 2026-07-15:
      file-backed icon freshness checks dropped the one-second global
      `Instant`/LRU layer after its release benchmark exposed a regression:
      50,000 direct metadata/key probes measured 51.3ms versus 76.7ms through
      the former TTL cache (1.5x faster), while also making file changes visible
      immediately. File-extension dispatch in the same paint/opacity path now
      uses borrowed case-insensitive comparisons instead of allocating a
      lowercase `String`; 2M mixed classifications measured 56.7ms allocating
      versus 35.9ms borrowed (1.6x faster). Remaining: move cache-miss reads and
      rasterization off-thread rather than doing either on the paint path.

### P1 — boundary & dispatch

- [ ] Per-paint element metrics: lazy `refs.<name>` field resolution for
      frames where metrics really changed (A; publication is already
      diff-gated and snapshots are lazy/sparse). Progress 2026-07-13:
      `refs.<name>` now caches the live element proxy table and element
      method closures after first access, while field reads still resolve
      against the current `__mesh_element_metrics` table. Release benchmark
      over 100,000 handler probes measured 342.5ms rebuilding proxy/function
      objects versus 134.9ms cached (~2.5x faster). Remaining: Rust-side lazy
      metrics storage so changed frames avoid whole-snapshot JSON→Lua
      publication when scripts read only a few fields. Added 2026-07-14:
      metrics collection now looks up `id`, `ref`, and `_mesh_bind_this` once
      per node and reuses those borrows for publication, avoiding the prior
      contains-then-get map probes. The existing release ref-only benchmark
      remains 7.24s collect-both versus 3.91s refs-only (~1.9x).
      Added 2026-07-14: refs publication now applies the live proxy while
      borrowing the snapshot, then moves that same JSON value into script
      state instead of cloning the full refs table. A release ownership
      benchmark measured 1.601s clone versus 996.7ms move over 20,000
      256-entry snapshots (1.6x).
      Ref-name → node-key publication now reuses its `HashMap` backing storage
      between paints; a release benchmark measured 1.368s fresh versus 719ms
      reused over 20,000 512-entry maps (1.9x).
      Added 2026-07-14: runtime annotation now indexes the active hover path
      once per tree pass rather than scanning it for every node; the release
      lookup benchmark measured 137.7ms path scans versus 56.8ms hash-set
      membership (2.4x). Shortcut accessibility annotation also borrows each
      node's keybind ID for lookup and pre-sizes its keybind index from the
      resolved shortcut count. Finalization now reuses prior hover/focus
      snapshot storage via `clone_from`, and interaction result sets reserve
      their directly changed-key lower bound only on non-empty changes. The
      resolved shortcut cache now also retains the preformatted accessibility
      index, so unchanged finalize passes borrow it instead of rebuilding the
      map; the release microbenchmark measured 3.297ms rebuild versus 2.4µs
      cached lookup over 1,000 probes.
- [ ] Handler dispatch: graph-wide instance-key interning (B; dispatch-path
      borrowing landed). Progress 2026-07-13: scheduled handler dispatch now
      precomputes the `__mesh_embed__::{instance_key}::{handler}` target when
      `shell.schedule-handler` is received, so due ticks clone one cached
      handler string instead of cloning instance+handler and formatting every
      time. Release benchmark over 5.12M due-handler prep entries measured
      351.4ms format-on-tick versus 207.8ms cached-name (~1.7x faster).
      The schedule-time namespace builder now uses one pre-sized allocation;
      1M representative names measured 83.7ms `format!` versus 13.5ms
      pre-sized construction (~6.2x faster).
      Follow-up same day: denied service-command routing now checks the
      borrowed cached control capability before command/interface string
      allocation; release benchmark over 200,000 denied events measured 36.3ms
      eager allocation versus 29.1ms deferred allocation (~1.2x faster).
      Service-command capability checks now borrow the cached control
      capability instead of cloning it on the production path; release
      benchmark over 1M checks measured 27.6ms cached clone versus 19.2ms
      borrowed cached `Arc` (~1.4x faster). Rejected: preallocating the script
      request vector was not stable in a fresh release run (363.0ms
      filter-collect versus 365.5ms preallocated loop), so that change was
      dropped. Follow-up 2026-07-15: ordinary graph namespacing now builds the
      `__mesh_embed__::{instance_key}::` prefix once per traversal and appends
      each raw handler with exact capacity instead of formatting the complete
      namespace per handler. A 1,000-node release benchmark over 200 traversals
      measured 79.6ms per-handler formatting versus 31.8ms shared-prefix
      construction (2.50x; repeat 2.53x). Follow-up 2026-07-15: plain root
      dispatch borrows the surface instance ID through runtime lookup and only
      materializes it for published-event or live-binding post-processing.
      Across 2M release lookups, owned-ID dispatch measured 41.4ms/41.0ms
      versus 30.9ms/31.4ms borrowed (1.34x/1.30x).
      Remaining: full graph-wide instance-key interning for ordinary handler
      dispatch.
- [x] Shell-side subscription index (service → component indices) so event
      routing is a lookup, not N mutex acquisitions per event (C). Shipped
      2026-07-13: components can expose a `ServiceObservationSummary`; shell
      builds a dirty-refresh delivery index keyed by service update name and
      interface event name, with unknown-summary components preserved on the
      legacy fallback gate. Index refreshes after component registration,
      mount, and render/template dependency discovery. Release benchmark:
      20,000 `mesh.audio` events across 256 summarized components measured
      202.8ms full scan versus 20.3ms indexed (~10x faster). Validation
      2026-07-14 fixed initial lazy-index construction so the first event is
      never dropped; the rebuilt index is then marked clean. A fresh release
      run measured 204.4ms full scan versus 15.3ms indexed (~13.4x), with the
      accepted-delivery profiling regression covered.
- [ ] Push-based backend host API primitives (D-Bus signal subscribe,
      fd/socket watch, stream adoption) so providers are event-driven and the
      safety poll is fallback (C). Includes investigating `pw-dump --monitor`
      as a real volume event source for pipewire-audio (`pw-mon` emits no
      `changed:` block for volume).
- [ ] Handler sync fast path still round-trips every known global per handler
      (env read + conversion + deep-compare); needs `_ENV` as a forwarding
      proxy or Rust-owned globals — measure read-through cost first; pairs
      with v1.17 (R). Progress 2026-07-13: `mesh.ui.request_redraw()` now uses
      a Rust atomic side-channel instead of a Lua global flag, removing the
      idle `__mesh_request_redraw` `_ENV` read from every handler sync; release
      benchmark over 1M idle redraw checks measured 159.7ms Lua global reads
      versus 1.8ms atomic checks (~90.6x faster for that check). The assigned
      new-global write log now has an atomic pending flag, so handlers that do
      not create new globals skip the empty mutex drain; release benchmark over
      1M empty checks measured 5.8ms mutex drain versus 1.7ms atomic pending
      check (~3.3x faster for that subpath). Remaining: every known global is
      still read unless the future `_ENV` forwarding proxy/Rust-owned-global
      architecture lands.
- [x] Contract validation: move compiled event schemas / type classifications
      out of the per-event path (S). Shipped 2026-07-13: shell now keeps a
      contract-validation cache keyed by the resolved contract `Arc`, with
      runtime-metadata state fields filtered once, compiled `TypeExpr`s stored
      per state/event payload field, and interface events addressed through an
      event-name map instead of a linear scan. Cache rebuilds automatically
      when the registry resolves a replacement contract. Release benchmark:
      100,000 validations against the last event in a 64-event contract
      measured 23.5ms uncached scan versus 10.7ms cached (~2.2x faster).
      Remaining S follow-up: broader API cleanup for retained interface names
      outside the command path.
- [ ] Storage reads clone per Lua access; future attempt needs shared
      immutable JSON values or lock avoidance without an extra Lua table
      lookup (I; naive Lua-side cache rejected — see log). Progress
      2026-07-13: storage `__index` now borrows string keys, calls read sinks
      with `&str`, and converts the stored JSON value by reference under the
      storage lock instead of cloning the `Value` per Lua read. Release
      benchmark over 100,000 nested table reads measured 1.987s cloned
      key/value versus 1.633s borrowed key/value (~1.2x faster). Storage-read
      tracking now uses an atomic boolean instead of locking a mutex for every
      read when render dependency tracking is off; release benchmark over 1M
      false checks measured 4.5ms mutex versus 0.44ms atomic (~10.2x faster
      for that check). Rejected 2026-07-15: exact-semantics nested-value caches
      both regressed 100,000 realistic reads — Rust recursive deep-copy cache
      measured 1.221s current versus 1.815s cached (0.67x), and Luau
      `table.clone` plus recursive arrays measured 1.237s versus 1.611s
      (0.77x). Both prototypes were reverted. Remaining: broader shared
      immutable storage values or lock avoidance needs a design that avoids
      rebuilding detached Lua tables per access.

### P2 — typing & interning (→ v1.23)

- [ ] Interned `Symbol`/`TagId` types; typed `WidgetNode` representation
      (tag/attrs/content as strings today), small-map attributes, and moving
      remaining shell annotations to typed fields (v1.23; `mesh_key` and
      scroll metrics already typed).
- [ ] Typed style declarations end-to-end: resolve theme tokens to typed
      values once per theme load; `apply_declaration` consumes typed values,
      strings only for diagnostics (E; borrowed simple-value fast paths
      landed across properties).
- [ ] Typed template-expression attribute storage; internal evaluation is
      already typed, results still stringify into attributes (A).
- [ ] Interaction identity is string-keyed end to end (`hovered_path`,
      `focused_key`, `scroll_offsets`, `input_values`, `slider_values`);
      migrate to `NodeId` together with metrics/refs publication so
      `_mesh_key` strings lose their last hot consumers (Q); runtime key-path
      strings are still allocated for interaction/refs (J). Scroll overflow
      annotation now reserves the reusable root key-path buffer; a 20,000-pass
      release benchmark measured 796.1ms unreserved versus 769.5ms reserved
      (1.03x).
- [ ] Allocator-level profile mode (allocation counts per render pass) →
      v1.23
- [ ] Magic-string protocol at the composition boundary (`__mesh_embed__::`,
      `__mesh_binding_*`, `__mesh_bind_this`, promoted-popover marker) —
      typed channels between compiler and shell (M).

### P2 — composition correctness & structure (M)

- [ ] **`and`/`or` template expressions diverge from Lua semantics**
      (correctness): `{name or "Anonymous"}` renders `true`/`false`;
      `is_truthy` treats `"0"`/`""` as falsy; `a or b and c` parses with
      inverted precedence. Fix as part of the typed expression-value enum.
- [ ] Typed handler-call linkage matches by value equality; two props bound
      to the same handler name get the wrong args — link by prop name.
- [ ] `{#if}`/`{#for}` always wrap children in a synthetic `column` node;
      needs a fragment/transparent-container concept.
- [ ] No keyed list diffing; `{#for}` identity is positional — add `key=`
      (pairs with component memoization and v1.27).
- [x] Per-rebuild prop churn — prop state writes and style-rule
      merge caching (M). Added 2026-07-14: slot result vectors reserve their
      catalog-known contribution count; small (up to eight) runtime prop maps
      skip a separate public-prop count pass; multi-prop handler indexes and
      per-node matched-call buffers reserve their known upper bounds. The
      existing release benchmarks continue to show runtime-prop construction
      at 608.1ms filtered collect versus 441.8ms presized (1.4x) and indexed
      handler matching at 944.8ms repeated scan versus 524.2ms (1.8x).
      Unchanged runtime prop publication now compares host-owned JSON by
      reference and clones only on change; 100,000 nested-value release writes
      measured 15.5ms for the owned unchanged gate versus 4.3ms borrowed
      (~3.6x). Local component host/style rules and their selector index are
      now prepared once per host-module/alias pair and reused across cache
      misses; 20,000 preparations of a 64-rule cascade measured 706.0ms versus
      10.7µs for cached access (eliminated work; ~66,069x in isolation).
      JSON→typed-prop validation and CSS projection now borrow their source
      values instead of deep-cloning before conversion, and CSS prop maps
      pre-size from the declaration count; 20,000 nested-value conversions
      measured 418.9ms owned versus 247.1ms borrowed (~1.7x).
- [x] Per-node build allocations — unique tracked-read string allocations
      reduced 2026-07-13: service proxy tracking now checks the per-evaluation
      observed-field set by borrowed `&str` before allocating `String`s for a
      newly observed field. Duplicate field reads no longer allocate a rejected
      key string. Release benchmark: 1,000,000 duplicate tracked reads measured
      31.5ms allocate-before-insert versus 16.5ms borrowed gate (~1.9x faster).
      Remaining M follow-ups: broader typed composition channels and other
      per-node allocation sources tracked separately.

### P2 — presentation & memory (H/U)

- [ ] Direct Skia paint into the mapped SHM canvas for full-present frames,
      keeping `PixelBuffer` as the retained/compare copy (H).
- [ ] SHM pool size classes (round up, viewport crop) so animated
      content-measured resizes stop reallocating the whole buffer set (H).
- [x] Input normalization: public `WindowEvent`/`DevWindowEvent` surface-id
      payloads moved from owned `String`s to shared `Arc<str>` IDs
      (U; lookup index and key-name borrowing landed). Progress 2026-07-13:
      disabled keyboard-repeat setup now returns before classifying
      non-repeating key names; release benchmark over 500,000 key batches
      measured 31.6ms classify-first versus 0.88ms repeat-gate-first (~36x
      faster for disabled-repeat setup). Single-character repeatable keys now
      bypass the modifier-name scan entirely; release benchmark over 1M key
      batches measured 11.1ms full scan versus 6.3ms length gate (~1.8x
      faster for that classification path). Case-insensitive key classification
      now caches the needle byte slice outside the haystack window loop; release
      benchmark over 300,000 key batches measured 60.40ms per-window
      `as_bytes()` versus 59.93ms cached needle bytes (small ~0.8% win).
      Completed 2026-07-15: presentation event surface IDs, dev-window keys,
      Wayland wl-surface reverse IDs, pointer/keyboard focus, keyboard repeat,
      input-coalescing state, and the shell routing boundary now share
      `Arc<str>` IDs. Cloning 2M representative event IDs measured 10.74ms for
      owned `String` versus 6.57ms shared (1.6x faster); the 500,000-event shell
      split/route benchmark measured 10.18ms for allocating a target ID versus
      8.89ms for moving the shared ID (~1.15x faster).
- [ ] Startup: parallelize module discovery + manifest parsing (frontend
      compilation already runs through Rayon) (H/V). Progress 2026-07-13:
      installed-graph auto-discovery now loads sorted module manifests through
      an ordered Rayon pipeline, preserving deterministic graph assembly while
      moving per-module file IO and JSON parsing off the startup thread.
      Release benchmark over 12 iterations of 192 synthetic modules measured
      80.5ms serial versus 12.5ms parallel (~6.5x faster). Follow-up same day:
      shell legacy discovery now separates deterministic recursive manifest-dir
      discovery from serial registration and loads manifests in parallel; release
      benchmark over 12 iterations of 192 synthetic modules measured 24.2ms
      serial versus 5.4ms parallel (~4.5x faster). Remaining: measure real
      startup profile impact with canonical v1.21 workloads.
- [ ] Rotation transforms allocate a temp `PixelBuffer` + full subtree
      repaint per frame; low priority until rotation ships in surfaces
      (P; scratch-buffer reuse rejected — see log).

### P2 — architecture

- [ ] GPU rendering after retained layout, smart invalidation, and damage
      tracking ship → v1.25: `wgpu`/Skia-GPU surface per output, retained
      display list as command source, SHM fallback (D).
- [ ] Eliminate service-specific Rust branches: the hardcoded `mesh.audio`
      optimistic-mute merge in `normalize_service_event` /
      `apply_optimistic_audio_muted_state` should become an optimistic-state
      declaration in the interface contract (S).
- [ ] `legacy_backend_candidates_from_discovery` is a compat lane duplicating
      graph-driven candidate selection; hard startup error or documented
      degraded-mode boot, then delete (V).
- [ ] Slider drags with `change`/`release` handlers still take script
      invalidation; closing this fully needs v1.18 narrow invalidation
      (J; handlerless drags already use interaction restyle). Added 2026-07-14:
      active-slider pointer moves now resolve the node and transformed,
      scroll-adjusted bounds in one allocation-free traversal rather than
      separate node and bounds searches. Paired text-input and hover
      enter/leave handler dispatches also reuse one immutable JSON event
      payload instead of cloning it for the second synchronous handler.
- [ ] Interaction frames still re-apply string style declarations per node;
      folds into typed declarations → v1.23 and narrower invalidation →
      v1.18 (P1 renderer item; indexed declaration metadata landed). Animation
      frames now reuse live-key sets and previous-style snapshot storage;
      release microbenchmarks measured 2.35x and 1.68x over fresh allocations.
- [ ] Minor: display-list `update_inner` is ~220 lines mixing diff, damage,
      and metrics assembly; split when next touched (N).

### Suggested attack order (updated 2026-07-13)

1. Local dev environment fix (xkbcommon/freetype/fontconfig) — restores
   in-crate verification for shell/render changes.
2. Canonical workload profiles + perf HUD with paint flashing (L / v1.21) —
   gates every decision below.
3. Fractional-scale partial damage, re-tested with upload instrumentation (D).
4. Child-popup retained pipeline, paint + present together (P + U).
5. Build purity (`BuildEffects`, M) → component-level render memoization (I).
6. Narrow invalidation / typed state dependencies (v1.18) + affected-subtree
   re-eval (v1.27).
7. Per-surface parallel paint (K phase split).
8. Interning / typed `WidgetNode` (v1.23) as the long tail.
