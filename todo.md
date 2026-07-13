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

- [ ] Fix the local dev environment (`xkbcommon.pc`, `freetype`, `fontconfig`
      via `nix develop`) so `mesh-core-shell` / `mesh-core-render` tests and
      in-crate benchmarks run again — several recent changes shipped with
      standalone approximations instead of in-crate verification.
- [ ] Canonical shell workload profiles (idle, pointer move, text update,
      scroll, icon grid, animation, theme reload, resize) → v1.21 (L tier 3
      harness; several open items say "measure with v1.21 profiles first")
- [ ] L tier 1 — in-shell perf HUD via the existing `DebugOverlay`: frame
      waterfall strip, live counters, paint flashing on damage rects (L)
- [ ] L tier 2 — cause attribution: per-rule restyle time, per-instance build
      time, per-command-kind paint time, wasted-work counters (L)
- [ ] L tier 3 — `mesh.debug.profiling_stream` over IPC, Chrome-trace/Perfetto
      export, CI regression baseline with tolerance band (L)

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
      Remaining: repeated same-alias instances share one runtime and always
      miss (needs per-occurrence instance identity — see the "multiple
      instances" module-system item); `render_slot` instances are not yet
      memoized; the M `BuildEffects` formalization still applies to future
      build side effects (new effects must add a mark counter or veto).
- [ ] Affected-subtree template re-evaluation via
      `NodeServiceFieldDependencies`; `narrow_script_update` still rebuilds
      the full tree before diffing → v1.27
- [ ] Generation-aware retained-tree diff: skip clean subtrees using dirty
      bits → v1.27. Remaining after landed progress: clean-subtree skipping,
      slotmap-keyed snapshot reuse (D).
- [ ] Triple full-tree fingerprinting on dirty frames: make
      `RetainedWidgetTree` the single fingerprint pass; render-object tree and
      display entries consume its per-node dirty flags (N).
- [ ] Any non-clean frame bypasses all generation shortcuts
      (`use_generation_shortcuts` requires an empty dirty set); widen to
      per-node dirty scoping together with the §N unification (P).
- [ ] Fuse the remaining unconditional `finalize_tree` annotation walks into
      one traversal → v1.27 (D; conditional walks already presence-gated,
      naive fusion rejected — see log).
- [ ] Display-list segment/rope command storage → v1.21: stop flattening
      retained subtrees into per-ancestor copies (O(n × depth) storage and
      re-copy, N addendum); dirty parents with layout/clip/transform changes
      still force descendant command rebuilds (N addendum).
- [ ] Retain Taffy node state across structural layout passes;
      `build_taffy_tree` still rebuilds a fresh TaffyTree per structural
      layout → v1.21 (non-structural dirty-node sync landed, T).
- [ ] Child popup surfaces bypass the retained pipeline: full clear + repaint
      through the immediate-mode painter per present, plus per-frame key
      walks (P); child buffers are still repainted eagerly even though sparse
      child damage now reaches presentation (U). Route child targets through
      the retained display-list + damage path and delete the immediate-mode
      painter twins (P structure item).
- [ ] Fractional HiDPI forces full-surface repaint every frame; fix the
      logical-vs-physical damage-clip mismatch through the painter. CPU-side
      experiment showed no win — re-test with compositor/upload damage
      instrumentation before concluding (D; likely the biggest win on
      fractional-scale outputs).

### P1 — threading (K)

- [ ] Parallelize paint across surfaces: phase-split `render_components` into
      a serial VM-bound phase and a parallel paint/SHM phase (rayon).
- [ ] Pipeline paint of frame N against script work of frame N+1
      (guarded-render-loop design; after the per-surface split).
- [ ] Tile-parallel raster for large damage (band-split full-surface
      repaints; only above a damage threshold, measure with v1.21 profiles).
- [ ] Move blocking file IO off the shell thread (i18n catalog mounts,
      settings/theme reloads, icon/SVG cache-miss rasterization on the paint
      path) via `spawn_blocking` + completion events.

### P1 — boundary & dispatch

- [ ] Per-paint element metrics: lazy `refs.<name>` field resolution for
      frames where metrics really changed (A; publication is already
      diff-gated and snapshots are lazy/sparse).
- [ ] Handler dispatch: graph-wide instance-key interning (B; dispatch-path
      borrowing landed).
- [ ] Shell-side subscription index (service → component indices) so event
      routing is a lookup, not N mutex acquisitions per event (C;
      component-local summary experiment rejected — see log).
- [ ] Push-based backend host API primitives (D-Bus signal subscribe,
      fd/socket watch, stream adoption) so providers are event-driven and the
      safety poll is fallback (C). Includes investigating `pw-dump --monitor`
      as a real volume event source for pipewire-audio (`pw-mon` emits no
      `changed:` block for volume).
- [ ] Handler sync fast path still round-trips every known global per handler
      (env read + conversion + deep-compare); needs `_ENV` as a forwarding
      proxy or Rust-owned globals — measure read-through cost first; pairs
      with v1.17 (R).
- [ ] Contract validation: move compiled event schemas / type classifications
      onto the registered contract (S; shell-side caches landed). Minor:
      broader API cleanup for retained interface names outside the command
      path (S).
- [ ] Storage reads clone per Lua access; future attempt needs shared
      immutable JSON values or lock avoidance without an extra Lua table
      lookup (I; naive Lua-side cache rejected — see log).

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
      strings are still allocated for interaction/refs (J).
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
- [ ] Per-rebuild prop churn — remaining: prop state writes and style-rule
      merge caching (M).
- [ ] Per-node build allocations — remaining: unique tracked-read string
      allocations (M).

### P2 — presentation & memory (H/U)

- [ ] Direct Skia paint into the mapped SHM canvas for full-present frames,
      keeping `PixelBuffer` as the retained/compare copy (H).
- [ ] SHM pool size classes (round up, viewport crop) so animated
      content-measured resizes stop reallocating the whole buffer set (H).
- [ ] Input normalization: public `WindowEvent`/`DevWindowEvent` surface-id
      payloads are still owned `String`s; move to `Arc<str>`/numeric ids
      (U; lookup index and key-name borrowing landed).
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
      (J; handlerless drags already use interaction restyle).
- [ ] Interaction frames still re-apply string style declarations per node;
      folds into typed declarations → v1.23 and narrower invalidation →
      v1.18 (P1 renderer item; indexed declaration metadata landed).
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
