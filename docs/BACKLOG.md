# MESH — Active Backlog

This is the single active backlog for MESH. Specifications describe contracts;
guides describe current behavior; historical evidence belongs in `.planning/`.
Before implementing an older item, verify it against the source because later
work may have landed without updating its checkbox.

Items marked `→ vX.Y` are tracked as GSD milestones in `.planning/ROADMAP.md`.

Completed performance work, progress narratives, benchmark numbers, and
rejected experiments were archived to
`.planning/performance/performance-log.md` on 2026-07-13.
Section letters (A–V) in the performance items below refer to that log.

---

## Shell features

- [x] Settings module — surface for managing installed modules, active providers, theme, i18n → v1.22. Progress 2026-07-02: added shipped `@mesh/settings` frontend surface (`modules/frontend/settings`) with a right-overlay dialog, graph-backed installed-module list/filter, active-provider binding summary, and live theme/locale controls wired through existing `shell.set-theme` and `mesh.locale.set` paths. `@mesh/quick-settings` now exposes an Open settings action that publishes `shell.show-surface` for `@mesh/settings` and hides the quick-settings popover. The installed graph now auto-discovers the settings module and the fixture test asserts it. Added 2026-07-16: provider rows enumerate enabled alternatives and use a settings-only `shell.set-provider` path that starts the candidate in isolation, keeps the current provider active until readiness, and persists the selection atomically before the live handoff. Module rows apply enable/disable decisions live for both auto-discovery and explicit-inventory root graphs; frontend surfaces are mounted or torn down dynamically, inactive backend graph changes take effect immediately, and configuration is rolled back if graph reload or frontend activation fails. The UI protects itself, the active root layout, active providers, and pending providers from disable actions. Full-shell verification now mounts the shipped surface dynamically, publishes the real debug graph, renders through the shell and presentation pipeline, and asserts configured geometry plus substantial painted output.
- [ ] Popups / overlays — transient surfaces with custom content and dismiss behavior → v1.22

### Module architecture friction redesign — 2026-06-19

The brainstorm and decision record were folded into
`docs/spec/01-module-system.md`.
Attacks authoring friction on top of the shipped interface/provider/frontend spine
(easy / unified / configurable). Selected path: **A+B headline, C/D reframed, F follow-on, E deferred.**

- [ ] **E (deferred) — Unify the 4 contribution schemas.** Theme/icons/i18n/keybinds under one `contributes` shape — only where they share honest structure; revisit after A/B land.
- Rejected: capability inference (C original) and parallel inline-interface path (D original) — both trade conceptual-simplicity for typing-simplicity, the failure mode this redesign avoids.

### Module system — remaining open follow-ups

The 2026-06-18 redesign largely shipped (canonical `module.json` with `mesh.uses`/
`mesh.provides`/`mesh.implements`, graph as single source of truth, typed graph
diagnostics for interfaces/icons/i18n/keybinds/capabilities, library modules,
resource packs). Remaining open work:

- [ ] Eliminate service-specific Rust branches where possible. Progress 2026-07-26: command-to-state reactivity is a shell feature — contract methods declare `stateBinding: { field, fromArg }`, and core updates the canonical state for any interface (`pending_bound_service_state`) until the provider confirms it; the `mesh.theme` settings-injection branch became a generic `__shell` context (`{ theme, locale }`) injected into every backend's settings. Remaining: startup-sound path calls the mesh.audio handler directly; debug/profiling paths.
- [ ] Support multiple instances of the same frontend module. Module identity should not be the only surface identity; root graph should support configured instances like two panels or repeated widgets with separate settings/storage scopes.
- [ ] Implement named shell profiles as the starting point for root component
      instances, surface placement, provider bindings, resources, and
      profile-scoped overrides.
- [ ] Implement transactional live profile switching that retains identical
      service instances and leaves the active profile untouched when candidate
      initialization fails.
- [ ] Add external `contract.json` support with keyed state, method, event, and
      type objects; compile it into the existing `InterfaceContract` model and
      generate strict Luau/LSP types.
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
`.planning/performance/performance-log.md`.

### Migration tech-debt (flagged by project rules; verify before removing)

- [ ] Three remaining hand-written `.mesh`/`.luau` source mini-parsers in
      `installed_graph.rs:~908-1051` (`extract_t_keys_from_mesh_source`,
      `extract_mesh_event_publish_channels`, `extract_backend_emit_event_names`).
      Progress 2026-07-02:
      `extract_icon_names_from_mesh_source` now uses the existing `.mesh`
      template AST (`parse_component` + `TemplateNode`) and walks elements,
      conditionals, loops, and component children instead of scanning strings.
      Project policy calls hand-rolled script string-parsing temporary migration
      code; migrate to AST-based analysis when the parser matures. Note:
      fixed 2026-07-01: `extract_keybind_subscriptions_from_mesh_source` scanned
      tag boundaries quote-aware, so `<`/`>` inside other attributes no longer
      hid `onkeybind`. Replaced 2026-07-16 with template-AST traversal across
      elements, components, conditionals, and loops.

---

## Performance — open items

### Text rendering follow-ups

- [x] Improve first-miss ellipsis truncation by using shaped glyph advances
      instead of binary-search substring measurement.
- [x] Add profiling visibility for text and glyph cache pressure: entry counts,
      hits, misses, invalidations, and shaping time.
- [x] Add locale-, script-, and direction-sensitive text cases to canonical
      performance workloads before changing shaping behavior further. Added
      2026-07-24: the retained text-cache workload now runs Latin/LTR,
      Arabic/RTL, Japanese/CJK LTR, and Devanagari/LTR cases, asserting cold
      shaping misses and warm cache reuse independently for every case.

Full history, benchmark baselines, and rejected experiments live in
`.planning/performance/performance-log.md`; section letters (A–V) below
reference it. The historical subsystem map is
`.planning/performance/sections.md`. Milestone refs unchanged.

### P1 — structural render pipeline

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
- [x] Generation-aware retained-tree diff: skip clean subtrees using dirty
      bits → v1.27. `RenderObjectTree` now counts visited
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
      slot-indirect versus 74.5ms direct (1.3x faster). Added 2026-07-20: the
      authoritative retained-tree pass now fingerprints each fresh node
      directly against its stable slot instead of staging a second full-tree
      `NodeId`→snapshot map and draining it afterward. Per-slot traversal epochs
      also skip the stale-entry scan when the frame is non-structural, while
      insertion/removal/reorder frames retain the checked pruning fallback.
      Across three runs of 2,000 one-node-dirty 1,365-node release updates,
      snapshot-map staging took 544.4–551.8ms versus 445.1–448.5ms for direct
      retained slots (1.218–1.230x faster); the
      relative speedup is now part of the checked performance gate. Added later
      2026-07-20: targeted interaction restyles carry their authoritative dirty
      roots into the retained-tree pass. Dirty roots and descendants receive
      full fingerprints; clean nodes check only child identity and the cheap
      layout tuple so layout propagation from a changed leaf is still detected.
      Structural mismatches fall back before retained state is mutated, while
      CSS/scroll/surface animation frames stay on the full path. Across three
      runs of 2,000 one-leaf-dirty 1,365-node release updates, full direct-slot
      fingerprinting took 455.6–470.4ms versus 78.6–83.0ms scoped
      (5.667–5.793x faster); this relative speedup is also checked in CI.
      Animation ticks now publish the exact nodes whose displayed animatable
      style changed while walking the existing animation pass. Frames requested
      exclusively by animations, plus targeted interaction restyles that also
      advance animations, merge those roots into the scoped retained update.
      Any unrelated invalidation, scroll animation, surface transition, or
      structural mismatch retains the full fallback. Across three runs of 2,000
      four-node-animated 1,365-node release updates, full fingerprinting took
      469.1–477.9ms versus 80.5–83.7ms scoped (5.689–5.829x faster), now checked
      in the performance gate. An end-to-end benchmark including animation
      tracking, restyle, layout, display-list work, and raster measured 250
      one-node-animated 1,026-node frames at 558.9–563.6ms with full retained
      fingerprints versus 423.7–433.4ms scoped across three runs (1.301–1.324x
      faster overall); this result is gated too. Narrow script builds now
      compare the fresh tree directly with the previous painted tree, using
      cheap field equality to identify changed roots without hashing clean
      nodes. Stable non-structural results feed the same scoped retained update;
      structural, scrolling, surface-transition, root-wide, and at-least-half-
      tree changes promote to the full path before retained state is mutated.
      Across three runs of 2,000 one-leaf-dirty 1,365-node updates, full retained
      fingerprinting took 473.8–483.5ms versus 280.4–303.4ms for direct diff plus
      scoped fingerprints (1.562–1.704x faster). End-to-end handler-to-pixel
      measurements over 100 one-node-dirty 1,026-node frames improved from
      563.4–621.3ms to 509.8–573.5ms (1.083–1.167x overall). Both relative
      results are performance-gated. Scoped retained updates now also pass
      direct references to their changed nodes into render-object sync instead
      of discarding that locality and walking the full tree again. Across three
      runs of 5,000 one-node-dirty 2,049-node updates, direct synchronization
      took 0.76–0.78ms versus 102.2–104.7ms for the tree walk (130.2–135.9x);
      combined retained diff plus render sync improved from 131.9–133.9ms to
      98.7–100.6ms over 2,000 1,365-node frames (1.327–1.341x). Both are now
      performance-gated. Paint-only frames and scroll animation ticks now enter
      the same scoped path with no style roots: child identity and cheap layout
      tuples are still checked across the tree, so changed typed scroll metrics
      are found without rehashing clean style and attributes. Across three runs
      of 2,000 one-node-scrolled 1,365-node updates, full fingerprinting took
      451.4–467.3ms versus 55.8–57.3ms scoped (7.984–8.349x). End-to-end
      paint-only measurements over 150 clean 2,050-node frames improved from
      544.7–577.2ms to 387.8–428.4ms (1.330–1.488x). Both are performance-gated.
      Structural changes, broad/root style changes, surface transitions, and
      external rebuilds deliberately retain the full checked fallback, completing
      clean-subtree skipping for the authoritative non-structural mutation paths.
      Added 2026-07-25: stable full render-object syncs now retain already-matching
      child-ID vectors instead of clearing and rewriting them after the structural
      equality check; three release runs of 200,000 256-child updates measured
      1.42–1.97x faster for that subpath. Per-node dirty-category comparison now
      returns its changed bit directly instead of copying and comparing the
      eleven-counter aggregate summary around every node; three release runs of
      five million material diffs measured 1.29–1.34x faster. Both relative
      improvements are checked by the canonical performance gate.
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
      paint commands checked against the full path. The retained-to-render handoff
      now carries direct changed-node references for scoped updates, eliminating
      render sync's redundant clean-node traversal while preserving the existing
      structural/broad fallback. Remaining: share the retained tree's computed
      fingerprint payloads directly, and prove any additional dirty categories
      before widening them. Added 2026-07-25: authoritative text/accessibility-only
      display updates retain their unchanged in-surface and compositor blur
      metadata instead of rescanning the full command list and widget tree.
      Blur-sensitive structural, transform, clip, opacity, geometry, material,
      and primitive categories still force recomputation. Three alternating
      release runs of 2,000 text updates on a 1,261-node blur-bearing surface
      measured 1.156–1.172x faster end to end; parity and the conservative
      relative speedup are checked in the canonical performance gate.
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

- [x] Per-paint element metrics: lazy `refs.<name>` field resolution for
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
      Metrics snapshots now move into their final `elements`/`refs`
      destination and clone only for additional aliases, instead of cloning
      every publication and dropping the original. Across 512,000 single-name
      snapshots, release measurements were 2.585s clone-and-drop versus 1.832s
      move-final (1.41x faster), with multi-alias parity covered.
      Snapshot `f32` fields now use serde_json's direct numeric conversion
      instead of the general-purpose `json!` serialization path. Five million
      release conversions measured 25.81ms through the macro versus 22.56ms
      direct (1.14x), with finite, signed-zero, NaN, and infinity parity
      covered.
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
      Completed 2026-07-24: live refs now share a surface-owned Rust metrics
      store and lower only the requested element snapshot into Lua, caching
      that table per proxy and publication version. The live store and
      template-facing state share one `Arc<Value>`, preserving full-snapshot
      template semantics without cloning the JSON tree; embedded component
      contexts share the same live store. Across three release runs of 2,000
      changed 256-element snapshots with one ref read, eager full JSON→Lua
      publication took 619.0–622.8ms versus 9.87–10.44ms for shared lazy
      publication and access (59.49–62.72x faster). The relative speedup is
      checked by the canonical performance gate.
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
      check (~3.3x faster for that subpath). Added 2026-07-20: handler-only
      contexts now track completion of initial globals discovery explicitly
      instead of treating an empty known-globals list as "not discovered" and
      rescanning `_ENV` after every handler. Over 20,000 release no-op handler
      calls with 256 functions, repeated scanning measured 789.9ms versus 4.2ms
      with the explicit discovery flag (~188x faster); late-created globals
      remain covered by the write log. Added later 2026-07-20: discovered scalar
      globals now move behind an `_ENV` forwarding table, so assignments enter
      the write log and unchanged scalars need no Lua lookup or comparison.
      Across 5,000 release no-op handlers with 512 scalar globals, the previous
      known-global read/equality path measured 779.7ms versus 36.1ms through the
      write-log proxy (~21.6x faster). Live bindings read the forwarded values
      without exposing host globals, reload restores them before execution, and
      scalar↔table transitions retain reactive semantics. Remaining: compound
      table globals still require reads because nested in-place mutations do not
      assign through `_ENV`; eliminating those reads needs recursively tracked
      tables or Rust-owned reactive values.
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
      scroll metrics already typed). Progress 2026-07-15: retained display
      payloads for text, input value/placeholder, and icon source/name now use
      `Arc<str>` with pointer-first equality. Dirty-node rebuilds reuse the
      prior allocation when payload bytes are unchanged, avoiding string
      allocation for style-only updates. Added 2026-07-23: compiler event-
      attribute classification now strips the `on` prefix as a borrowed slice
      instead of allocating a normalized `String`; a 2M-classification release
      benchmark measured 13.40–16.23ms allocating versus 5.00–6.00ms borrowed
      across three runs (2.23–3.24x faster). Added 2026-07-26:
      `element_contract_for_tag` (`crates/core/ui/elements/src/element.rs`) —
      called once per node from `accessibility_for_element` on every tree
      build, and per tag/attribute from LSP validation — resolved its result by
      scanning all 62 `ELEMENT_CONTRACT_DEFS` entries and string-comparing
      `def.tag`, so late definitions (`panel`, `surface`, `widget`,
      `list-item`) and unknown tags cost the full scan. Tags now dispatch
      through a generated `match` over the tag literals, which lowers to a
      length switch instead of a scan, and the matched slot indexes the
      unchanged definition array. Drift is closed from both sides: each arm
      resolves its slot in an inline `const` block via `contract_slot_of`, so a
      tag missing from `ELEMENT_CONTRACT_DEFS` fails the build, and
      `element_contract_dispatch_covers_every_definition` fails if a definition
      has no arm or the arms are misordered. A release benchmark over 3,000,000
      representative lookups measured 52.1–55.3ms scanning versus 11.3–11.5ms
      dispatching across three runs (4.53–4.91x faster); lookup parity with the
      scan, including near-miss and unknown tags, is covered and the relative
      speedup is gated as `element_contract_dispatch_speedup`. A `HashMap`
      index was measured and rejected at only 2.0x. Added later 2026-07-26:
      `WidgetNode::module_id` moved from `Option<String>` to
      `Option<Arc<str>>`. Every node built from one module carries the same
      identity string, so the compiler resolves it once through a bounded
      most-recently-used list (`shared_module_id`, a short string comparison,
      not a hash) and clones a pointer per node instead of allocating and
      copying the id. Isolated release measurements over one million
      assignments were 14.66–15.69ms per-node `String` versus 4.17–5.07ms
      shared `Arc` (3.00–3.76x), gated as `shared_module_id_speedup`; the
      bounded cache is covered for reuse, distinct modules, and post-eviction
      correctness. Element tag identity also stops round-tripping through an
      owned `String`: runtime tags are static strings, so `build_element_node`
      keeps them borrowed for style/mask/accessibility lookups and lets the
      node own the single allocation, and the style-matching id is borrowed
      instead of cloned. `{#for}` child vectors are now built at their exact
      final capacity rather than grown, since each reallocation re-copies
      every ~900-byte `WidgetNode` pushed so far (600-node release benchmark:
      167.3–174.0ms growing versus 142.8–153.5ms reserved, 1.10–1.22x, gated
      as `for_children_capacity_speedup`). Added 2026-07-27: widget-tree
      *attributes* are now interned and flat.
      `WidgetNode::attributes` (and the component-prop maps that share its
      shape) moved from `BTreeMap<String, String>` to a purpose-built
      `AttributeMap` (`crates/core/ui/elements/src/attributes.rs`) whose keys
      are an `AttrKey` enum: names in the known template/runtime vocabulary
      resolve to `&'static str` through a generated `match`, and anything else
      shares an `Arc<str>` from a bounded per-thread intern cache — so a node no
      longer allocates, copies, and frees one `String` per attribute per build.
      The map itself is a sorted `Vec` rather than a B-tree, built at the
      source-attribute count so the whole map costs one allocation; iteration
      order, `map.get("class")` borrowed lookups, and `entry(..).or_insert(..)`
      all behave exactly as the `BTreeMap` did, which a randomized differential
      test asserts against a live `BTreeMap<String, String>` reference. Drift is
      closed from both sides: `well_known_covers_every_contract_attribute` fails
      if an element contract declares a name the vocabulary is missing. Style
      resolution lost two hot substring searchers in the same pass: the
      `value.contains("var(") || value.contains("prop(")` pair asked per literal
      declaration per node became one byte scan
      (`references_style_function`, 3.21–4.23x over 3,000,000 values, gated as
      `style_function_scan_speedup`), and `trim_end_matches("px")` — a two-way
      searcher for a two-byte suffix, run once per length value — became
      repeated `strip_suffix`. Building a tree's worth of attribute maps and
      holding them, which is what makes the key allocations cost more than a
      tcache round-trip, measured 50.3–51.4ms owned-`String` `BTreeMap` versus
      46.6–47.3ms interned flat map over 900 trees of 456 nodes (1.08x), gated
      as `interned_attribute_map_speedup`. End-to-end effect of this checkpoint:
      three interleaved release runs of `widget_tree_build_end_to_end_benchmark`
      measured 0.500–0.506ms per build before versus 0.442–0.453ms after
      (1.10–1.15x for the whole 456-node tree build; the runs are interleaved
      because consecutive before/after batches drift by more than the effect); `perf` over the same
      workload shows the B-tree node allocation, walk, and teardown symbols gone
      from the profile entirely. Note that
      `accessibility_empty_attribute_guard_speedup` was rebaselined from 1.60 to
      1.15 as a consequence: the empty-map lookup chain it is measured against
      got cheaper (47.4ms → 30.1–31.4ms per 2M nodes) while the guarded path was
      unchanged. Widget-tree *tags*, attribute *values*, and the broader symbol
      types remain open; the same profile now puts the dominant remaining cost
      in style resolution (~45% of the build: theme-default clones,
      `apply_indexed_declaration`, and token→number resolution), i.e. the typed
      style declaration item below, not further attribute work.
      Added 2026-07-27: ordered attribute construction now takes an append-only
      fast path in `AttributeMap::insert`, avoiding the binary search and tail
      shift used by out-of-order inserts. The existing release construction
      gate remains green at 1.046x versus the owned `BTreeMap` reference; the
      randomized differential test continues to cover arbitrary insertion
      order and replacement semantics.
- [ ] Typed style declarations end-to-end: resolve theme tokens to typed
      values once per theme load; `apply_declaration` consumes typed values,
      strings only for diagnostics (E; borrowed simple-value fast paths
      landed across properties). Progress 2026-07-15: static diagnostic
      property/message prototypes are prepared once per `StyleRuleIndex`
      generation, removing repeated per-matched-node message formatting while
      preserving diagnostic parity and rule-index invalidation. Typed style
      value lowering remains open. Added 2026-07-27: per-node theme-default
      delivery stopped being a deep copy behind two hashed lookups. The
      resolver's `(module_id, tag)` default caches now hold
      `Rc<ThemeComponentDefaults>`, so a node clones one `ComputedStyle` (the
      mutable base its own rules are applied to) and bumps a refcount instead of
      also deep-copying the default *variables* map; the variables are read
      through, and the per-node `VARIABLE_SCRATCH` is only seeded when a theme
      actually declares default custom properties (the shipped theme declares
      none, so the common path seeds nothing). In front of the hashed maps sits
      a bounded most-recently-used list compared by string rather than hashed —
      the same shape as `shared_module_id` — because a tree walks long runs of
      the same `(module_id, tag)` and each node was otherwise paying two SipHash
      computations of short strings. A release benchmark over 400,000 node
      resolutions measured 38.9–40.5ms for hashed lookup plus deep clone versus
      19.6–21.2ms for the front cache plus shared defaults (1.84–1.98x), gated
      as `shared_theme_defaults_speedup`; the front cache is covered for
      tag/module key separation, correctness past eviction, and for not letting
      one node's resolution mutate the defaults the next node starts from.
      Measured end to end with interleaved release runs of
      `widget_tree_build_end_to_end_benchmark`: 0.443–0.449ms per build before
      versus 0.416–0.424ms after (1.06x), and style resolution fell from 45% to
      39% of the build in `perf`. Cumulatively with the interned attribute map
      above, three interleaved pairs measured 0.504–0.511ms per build before
      both checkpoints versus 0.417–0.428ms after (1.18–1.22x).

      Added 2026-07-28: ordinary no-diagnostic theme-default lowering now
      survives fresh `StyleResolver` construction. `Theme` carries a monotonic
      style revision; clones with identical contents retain it, while mutable
      access to tokens, defaults, or module contributions advances it before
      exposing the data.
      Resolvers share a bounded thread-local cache keyed by that revision,
      `(module_id, tag)`, and an exact collision-checked prop snapshot, so
      identical component instances share lowering while different `prop(...)`
      values remain isolated. Correctness coverage proves cross-resolver
      sharing, mutation invalidation, and prop-map isolation. Across three
      release runs of 100,000 fresh prop-bearing resolvers, repeated lowering
      took 125.7–128.8ms versus 18.83–20.35ms through the revision cache
      (6.18–6.84x); the relative speedup is checked by the canonical
      performance gate. The representative 456-node tree build measured
      0.390–0.397ms after this checkpoint versus 0.416–0.424ms before it
      (1.05–1.09x end to end). Typed property values and one-time token lowering
      remain open.
- [x] Typed template-expression attribute storage (A). Internal evaluation was
      already typed, but results still stringified into attributes. Progress
      2026-07-15: boolean, nil, number, string, and compound JSON values remain
      typed through expression evaluation; attribute-boundary stringification
      is still the remaining step. Added 2026-07-25: `accessibility_for_element`
      (`crates/core/frontend/compiler/src/render.rs`) parses every element's
      attributes back into typed accessibility fields (`disabled`, `checked`,
      `selected`, `min`/`max`, etc.) on every tree build via ~15 separate
      `BTreeMap` lookups; elements with no attributes at all — including every
      synthetic `{#if}`/`{#for}` column wrapper — now return the default
      accessibility state immediately instead of probing the empty map. A
      release benchmark over 2,000,000 empty-attribute elements measured
      47.7ms for the full lookup chain versus 26.6ms guarded (1.79–1.81x
      across repeated runs); parity with the unguarded chain is covered for
      both empty and populated attribute maps, and the relative speedup is
      now checked by the canonical performance gate
      (`accessibility_empty_attribute_guard_speedup`). Added 2026-07-26:
      elements that *do* carry attributes no longer probe the map once per
      accessibility field either. The ~19 remaining `BTreeMap` descents (each a
      tree walk with string comparisons, for keys that are almost always
      absent) became one ordered pass over the node's own attributes with a
      match on the key; alternative spellings (`aria-label`/`label`/`alt`,
      `title`/`tooltip`, `key`/`keybind`/`shortcut`, `expanded`/`open`) are
      collected during the pass and resolved by the same precedence
      afterwards. A release benchmark over 2,000,000 four-attribute elements
      measured 373.3–440.0ms for the lookup chain versus 52.3–54.9ms for the
      single pass (7.14–8.02x across three runs), gated as
      `accessibility_attribute_pass_speedup`; parity with the chain is covered
      for the full attribute set, every single-attribute map, and maps with
      each higher-precedence spelling removed. Completed 2026-07-28:
      non-string template bindings now enter `AttributeMap` as their original
      JSON boolean, number, null, array, or object instead of being formatted
      immediately. The existing string-facing map API remains compatible:
      string values stay allocation-free, while a boxed typed value lazily
      caches its historical string representation only when a legacy consumer
      requests it. Equality, ordered iteration, replacement, mutable access,
      owned iteration, and serialization retain their previous string
      semantics. Accessibility lowering uses a pointer-sized typed value view,
      so boolean and numeric fields no longer stringify and parse during a
      tree build. Correctness coverage exercises every stored JSON category,
      lazy materialization, mutation, serialization, map equality, runtime
      expression lowering, and accessibility parity. Across three release
      runs building and lowering six bound attributes on 500,000 nodes,
      stringify-plus-parse took 139.6–143.9ms versus 84.6–88.0ms for typed
      storage (1.64–1.68x); the relative speedup is checked by the canonical
      performance gate (`typed_attribute_storage_speedup`).
- [ ] Remaining interaction identity is string-keyed end to end
      (`hovered_path`, `focused_key`, `input_values`, `slider_values`);
      migrate to `NodeId` together with metrics/refs publication so
      `_mesh_key` strings lose their last hot consumers (Q); runtime key-path
      strings are still allocated for interaction/refs (J). The earlier scroll
      overflow annotation reserved its reusable root key-path buffer; a
      20,000-pass release benchmark measured 796.1ms unreserved versus 769.5ms
      reserved (1.03x). That intermediate optimization was superseded by the
      NodeId scroll-state checkpoint below. Added 2026-07-23: pointer and keyed
      tooltip traversal carry
      borrowed owner/text references and allocate only the final API result
      instead of allocating at every tooltip-bearing ancestor. A 64-node,
      100,000-lookup release benchmark measured 331.61–342.17ms eager versus
      171.52–173.52ms borrowed across three final runs (1.93–1.99x faster); the
      canonical fused pointer-motion workload moved from a 700.51ms baseline
      to 627.77–669.92ms across three final runs (1.05–1.12x faster).
      Follow-up 2026-07-23: visible-tooltip rendering now resolves inherited
      text, the owner node, and transformed owner bounds in one allocation-free
      traversal instead of separate tooltip, node, and bounds walks. Across
      three release runs of 20,000 deep lookups in a 2,601-node tree, separate
      walks took 3.96–4.56s versus 2.86–2.89s fused (1.38–1.58x faster for the
      per-frame lookup portion). Added later 2026-07-23: the resolved tooltip
      render target is cached by retained-tree generation plus hovered key, so
      stable fade and paint-only frames skip the tree entirely; hover changes
      clear it and any retained layout/style/attribute/state/structure change
      refreshes it. Across three release runs of 20,000 stable deep-tree frames,
      repeated fused resolution took 3.04–3.39s versus 243–265µs for guarded
      cache hits (11,693–13,929x faster for the eliminated lookup). The cached
      text and frame-local paint tuple now share `Arc<str>` ownership instead
      of deep-cloning a `String` so later mutable shell work can outlive the
      cache borrow. Across three release runs of one million representative
      504-byte tooltip handoffs, `String` cloning took 9.45–16.02ms versus
      2.77–6.79ms for `Arc` cloning (2.25–3.41x faster).
      Hover-transition path differences now use the shared root-to-leaf prefix
      instead of two pairwise membership scans and temporary vectors. Across
      three release runs of 200,000 deep sibling transitions on 2026-07-24,
      pairwise scans took 4.667–4.799s versus 36.0–37.8ms for prefix slices
      (123.5–132.1x faster). The fused multi-key node lookup now returns keys
      borrowed from the retained tree instead of cloning every matched key.
      Across three release runs of 20,000 dense lookups on 2026-07-24, owned
      results took 427.3–432.4ms versus 284.8–287.4ms borrowed
      (1.498–1.518x faster). Keyboard navigation key paths and candidate lists
      now borrow retained-tree keys and allocate only the selected result.
      Three release runs of 200,000 64-node key-path lookups measured
      238.0–247.5ms with owned strings versus 124.7–126.5ms borrowed
      (1.881–1.985x faster). All three relative improvements are checked by the
      canonical performance gate. Added 2026-07-26: overflow annotation
      (`crates/core/ui/interaction/src/scroll.rs`) allocated a `key.to_string()`
      `scroll_offsets` entry for every node in the tree, scrollable or not, so
      the map grew to roughly one permanent entry per node ever rendered
      instead of one per actual scroll container; every other reader of
      `scroll_offsets` already treats a missing key as the default zero offset,
      so non-scrollable nodes (the common case) now skip the map entirely and
      report a local zero offset. A release benchmark over a 781-node tree
      with only the root scrollable measured 335.4ms unconditional touching
      (781 map entries) versus 93.6ms scrollable-gated (1 map entry) — 3.46–
      3.58x faster across two runs, gated as
      `scroll_offset_scrollable_gate_speedup`. Completed scroll-state
      checkpoint 2026-07-28: wheel/two-finger hit results, live offsets,
      smooth-scroll animations, overflow annotation, and scroll-into-view
      updates now carry stable `NodeId` values end to end. String key paths
      remain only at the imperative ref lookup boundary, and standalone
      overflow annotation no longer constructs or formats structural paths.
      Across three release runs annotating 20,000 781-node trees, string-keyed
      state took 1.101–1.111s versus 237.8–253.0ms for NodeId state
      (4.39–4.66x faster), gated as `node_id_scroll_offsets_speedup`.
      Scroll action regressions now use explicitly non-shrinking overflow
      content so they exercise real scrolling rather than a flex-shrunk child.
      Completed checked-state checkpoint 2026-07-28: checkbox, switch, radio,
      and option selection state plus its previous-frame restyle snapshot now
      use stable `NodeId` keys. Runtime annotation no longer hashes structural
      strings for every checked-state lookup, and checked-state diffs pass IDs
      directly into targeted restyling instead of reconstructing them from
      paths. Script handlers retain readable string keys only at their dispatch
      boundary. Across three release runs of 40,000 1,024-node annotation
      passes, string-keyed state took 419.5–421.0ms versus 247.7–253.4ms for
      NodeId state (1.66–1.69x faster), gated as
      `node_id_checked_state_speedup`.
- [x] Allocator-level profile mode (allocation counts per render pass) →
      v1.23. Added 2026-07-24: the opt-in `allocation-profiling` build wraps
      the system allocator with allocation-free thread-local counters and
      attributes allocation/deallocation operations, bytes, and reallocations
      to each completed surface render pass while suspending counters around
      profiler bookkeeping writes. Cumulative and bounded recent samples are
      published through `mesh.debug` and rendered in the Overview and Surfaces
      inspector views. `./tools/profile-shell alloc` builds and launches the
      mode; normal builds remain uninstrumented, and the mode is deliberately
      exclusive with Tracy's global allocator. Three alternating
      release runs of four million 64-byte allocation/deallocation pairs
      measured the counter wrapper at 1.047–1.098x the raw system allocator
      time (4.7–9.8% profiling overhead per allocation pair); the overhead is
      bounded by the canonical performance gate.
- [ ] Magic-string protocol at the composition boundary (`__mesh_embed__::`,
      `__mesh_binding_*`, `__mesh_bind_this`, promoted-popover marker) —
      typed channels between compiler and shell (M).

### P2 — composition correctness & structure (M)

- [x] Typed handler-call linkage preserves authored prop identity, so two props
      bound to the same handler name retain their own typed arguments. Added
      2026-07-20: component-call props now survive event-name normalization,
      use distinct render-time tokens, and lower to the real namespaced handler
      only after the child tree is built; compiler-boundary and lowering
      regressions cover equal target handlers with different arguments. Added
      2026-07-23: dispatch now borrows typed handler arguments when either the
      prebound or runtime argument side is empty, retaining allocation only for
      a real two-sided merge. Across three release runs of one million
      runtime-only dispatches, clone-and-extend took 90.4–92.2ms versus
      9.26–9.43ms borrowed, a conservative 9.6x improvement. The remaining
      two-sided merge now allocates its final capacity once instead of cloning
      into an exact-sized vector and growing it again. Across three release
      runs of one million mixed prebound/runtime merges, clone-then-grow took
      247.5–248.9ms versus 193.2–197.5ms presized, a conservative 1.25x
      improvement. Added 2026-07-26: `apply_indexed_prop_handler_calls`
      (`crates/core/shell/src/shell/component/composition.rs`) recurses the
      whole embedded subtree once any prop-bound callback is passed down, and
      pre-sized its per-node `handler_calls` buffer to `node.event_handlers.len()`
      even though most handler-bearing nodes (plain `onclick`/`onchange`/etc.)
      never match one of the few prop-bound-call tokens, so the buffer is
      allocated and then thrown away empty. It now starts as `Vec::new()` and
      only allocates on an actual match. A release benchmark over a 781-node
      subtree where every node carries 3 non-matching handlers measured a
      consistent but modest 1.05–1.11x across four runs (76.4ms presized vs.
      71.5ms unallocated in the recorded run), gated as
      `unmatched_prop_handler_call_speedup`.
- [ ] `{#if}`/`{#for}` always wrap children in a synthetic `column` node;
      needs a fragment/transparent-container concept.
- [ ] No keyed list diffing; `{#for}` identity is positional — add `key=`
      (pairs with component memoization and v1.27).
### P2 — presentation & memory (H/U)

- [ ] Direct Skia paint into the mapped SHM canvas for full-present frames,
      keeping `PixelBuffer` as the retained/compare copy (H).
- [x] SHM pool size classes (round up, viewport crop) so animated
      content-measured resizes stop reallocating the whole buffer set (H).
      Completed 2026-07-28: Wayland surfaces with `wp_viewporter` now round
      physical SHM allocations up to 64px classes and crop the allocation to
      the rendered extent before applying the logical destination. A six-frame
      content-size jitter sequence now keeps one pool configuration instead of
      six; compositors without a viewporter deliberately retain exact-size
      allocations. Strided full and sparse copies preserve the visible source
      pixels inside a larger canvas, and newly allocated buffers begin with
      full visible damage so a first sparse frame cannot expose uninitialized
      pixels. The viewport crop uses post-`buffer_scale` source coordinates,
      including fractional-scale frames. Presentation tests pass (55 passed,
      12 ignored).
- [ ] Rotation transforms allocate a temp `PixelBuffer` + full subtree
      repaint per frame; low priority until rotation ships in surfaces
      (P; scratch-buffer reuse rejected — see log).

### P2 — architecture

- [ ] GPU rendering after retained layout, smart invalidation, and damage
      tracking ship → v1.25: `wgpu`/Skia-GPU surface per output, retained
      display list as command source, SHM fallback (D). Plan written
      2026-07-15 (`.planning/todos/pending/2026-07-15-gpu-rendering-backend.md`):
      Skia-GL (Ganesh) first — same Canvas API as the shipped raster backend,
      EGL buffer-age partial present preserves the damage pipeline; wgpu/Vello
      stays the replacement candidate behind the backend-neutral painter API.
- [ ] Real in-surface blur — plan in
      `.planning/todos/pending/2026-07-15-in-surface-blur.md`. Shipped
      2026-07-15: in-surface `backdrop-filter` executes on both the retained
      display-list path and the immediate painter (BLUR-03 no-ops removed;
      Skia `apply_backdrop_filter_impl` was already implemented). Sparse
      damage is blur-aware: the display list tracks backdrop read regions
      (node rect + 3×radius pad) for blur nodes that have painted content
      beneath them in paint order, and `expand_damage_for_backdrop_filters`
      grows intersecting effective-damage rects to the whole region at the
      shell choke point, so the blur re-reads a consistently repainted
      backdrop (pixel-parity test: sparse expanded repaint == full repaint).
      A surface root with an empty in-surface backdrop (nav bar) contributes
      no region, so bar damage stays minimal. Promoted child popup surfaces
      now also get compositor blur regions (`child_surface_blur_region` →
      `update_blur_region`), driven by the child display list's
      backdrop-filter nodes; frosted bubble popovers (language/theme) and the
      audio popover use translucent cards + `backdrop-filter`. Remaining:
      element `filter: blur()` still blurs only the node's own painted shape
      (mask filter) — full subtree blur needs layer push/pop command kinds in
      the retained display list; downsample-blur-upsample bounding and the
      GPU path per the plan doc.
- [x] Make command-bound service state a generic shell feature (S).
      Contract methods declare `stateBinding: { field, fromArg }`; successful
      dispatch writes the canonical `(interface, field)` state, republishes it
      to all observers, retains it across stale provider updates, and clears
      the pending binding on confirmation. Verified with a non-audio service
      regression so this path cannot silently depend on `mesh.audio`.
- [x] Delete the discovery-order backend candidate compatibility lane (V).
      Startup and supervised restarts consume the installed graph's explicit
      active provider through `backend_launch_candidates_from_graph` /
      `launch_candidate_for_provider`; missing or invalid selected providers
      produce typed degraded-mode lifecycle statuses. Verified 2026-07-23 that
      an installed, runnable but unselected discovered provider is never used
      as a fallback when the graph-selected provider is unavailable.
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
      Added 2026-07-23: dynamic inline-style strings now use a bounded
      thread-local cache of parsed and indexed declarations, including cached
      parse failures for diagnostic parity. Across three release runs of
      50,000 repeated resolutions, uncached parsing took 193.6–200.8ms
      (3.872–4.015µs/node) versus 16.0–19.4ms cached
      (0.320–0.387µs/node), a conservative 10.0x improvement and roughly 12x
      at the median. Static stylesheet declarations were already indexed;
      broader typed declaration storage remains open. Added later 2026-07-23:
      nodes whose candidates come from one rule-index bucket now iterate that
      bucket directly instead of copying its IDs into scratch storage and
      sorting/deduplicating them. Multi-bucket nodes retain the ordered,
      deduplicated fallback. Across three release runs over two million
      single-class nodes, the previous reused-scratch copy-sort-dedup path took
      29.2–31.0ms versus 22.7–23.2ms direct, a conservative 1.26x improvement.
      Added 2026-07-25: the single-bucket path now streams the indexed rule
      slice before borrowing merge scratch; multi-bucket candidate collection
      retains the ordered sort/dedup fallback. The release benchmark records
      `style_single_bucket_speedup` and gates the 1.20x checkpoint.
      Added 2026-07-26: the inherited-text-style mask
      (`crates/core/frontend/compiler/src/style.rs`) kept its own cached
      candidate list but still scanned *every* rule carrying an inheritable
      declaration for every node. Candidates are now bucketed by a selector key
      that is necessary for a match (tag, class, id; universal and `*`-state
      selectors stay unkeyed and are visited by all nodes), and compound
      selectors file under their first keyable part — so a node visits only its
      own buckets, and `selector_matches` still decides every visited
      candidate. Candidates whose mask bits are already accumulated skip
      selector matching entirely. A release benchmark over 1,400,000 lookups
      against a 94-rule stylesheet measured 336.9ms full candidate scan versus
      119.2ms bucketed (2.83x), gated as
      `inherited_style_mask_bucket_speedup`, with mask parity against the
      retained full-scan reference covered across tags, classes, ids,
      compounds, container queries, and rule-set changes.
      End-to-end effect of this checkpoint: a new release benchmark
      (`widget_tree_build_end_to_end_benchmark`) builds a 456-node tree from a
      representative component (nested rows/columns, classes, text and
      expression nodes, `{#if}`, a 64-item `{#for}`, 16-rule stylesheet).
      Across three runs each, the pre-checkpoint tree took 0.571–0.579ms per
      build versus 0.505–0.510ms after (1.12–1.15x for the whole tree build).
      Profiling the same workload shows the remaining time is dominated by
      allocator traffic and `BTreeMap<String, String>` attribute comparisons
      (~25% malloc/free, ~10% memmove, ~8% memcmp) — i.e. the typed
      `WidgetNode` and typed-declaration items above, not further per-node
      pruning. Followed up 2026-07-27 by the interned flat `AttributeMap`
      (see the interning item above), which removed the B-tree half of that
      allocator traffic.
- [x] Minor: display-list `update_inner` is ~220 lines mixing diff, damage,
      and metrics assembly; split when next touched (N). Completed 2026-07-28:
      entry reconciliation and sparse/full damage accounting now live in the
      dedicated `reconcile_entries` helper; the full render test suite passed
      with 188 tests and 35 ignored release benchmarks.

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
