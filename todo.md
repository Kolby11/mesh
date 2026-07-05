# MESH — Active Backlog

Items marked `→ vX.Y` are tracked as GSD milestones in `.planning/ROADMAP.md`.

---

How I split it: (1) component composition & template eval, (2) retained tree/diff/display list, (3) style system & theming, (4) rendering/paint, (5) interaction & input, (6) script runtime & Lua boundary, (7) events/services/backends, (8) layout, (9) presentation & memory, (10) shell orchestrator/threading/startup, (11) instrumentation. Each section lists what already shipped and an ordered set of next upgrades, so you can work one section at a time as you asked.

## Shell features

- [ ] Settings module — surface for managing installed modules, active providers, theme, i18n → v1.22. Progress 2026-07-02: added shipped `@mesh/settings` frontend surface (`modules/frontend/settings`) with a right-overlay dialog, graph-backed installed-module list/filter, active-provider binding summary, and live theme/locale controls wired through existing `shell.set-theme` and `mesh.locale.set` paths. `@mesh/quick-settings` now exposes an Open settings action that publishes `shell.show-surface` for `@mesh/settings` and hides the quick-settings popover. The installed graph now auto-discovers the settings module and the fixture test asserts it. Remaining: write-through controls for enabling/disabling modules and switching active providers, plus full-shell render verification once the environment has the `xkbcommon` development package required by `smithay-client-toolkit`.
- [ ] Popups / overlays — transient surfaces with custom content and dismiss behavior → v1.22
- [ ] Clean up backend modules and interfaces — consider moving the interface contract declaration from the separate `modules/interfaces/` directory into the implementing backend module, or bundling it as core metadata; evaluate impact on multi-provider resolution before changing

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

- [ ] Eliminate service-specific Rust branches where possible. Current audio optimistic state and some debug/profiling paths are pragmatic, but new module domains should route through interfaces/contracts/providers.
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
**cross-crate/intra-crate dedup** (commit `a4125d7`). The items below are the
remaining, deliberately-deferred findings (cheap quality wins + larger
refactors). Each cites `file:line` as of the audit; reverify line numbers
before editing.

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

## Performance — remaining open items

Items owned by a milestone are listed with their milestone reference.

### P0 — scheduling and invalidation (→ v1.18 / v1.19)

- [ ] Narrow script/service invalidation below tree-rebuild + pixel repaint; add typed state dependencies → v1.18
- [ ] Avoid full-tree restyle for safe interaction changes; use selector-dependency analysis → v1.18

### P0 — scripting (→ v1.17)

- [ ] One `mlua::Lua` VM per ScriptContext (`runtime.rs:92`); move to per-thread VM with `_ENV` isolation → v1.17
### P1 — renderer hot paths

- [ ] Interaction frames still re-apply string style declarations per node (`apply_declaration_no_diagnostics` + theme defaults maps dominate the post-2026-06-10 toggle profile); folds into the typed/compiled declaration work → v1.23 and narrower invalidation → v1.18
- [ ] Avoid flattening retained display-list subtrees into a new flat command buffer on each update; move toward segment/rope-style command storage → v1.21
- [ ] Replace per-node string/hash-heavy style matching with interned/typed node keys; remaining after first pass: interned tags, classes, attribute keys → v1.23
- [ ] Retain Taffy node state across layout passes; `build_taffy_tree` rebuilds a fresh TaffyTree every layout → v1.21
- [ ] Affected-subtree template re-evaluation: `narrow_script_update` rebuilds the full tree (full template eval) then diffs; use `NodeServiceFieldDependencies` to re-evaluate only nodes whose tracked fields changed → v1.27
- [ ] Generation-aware retained-tree diff: `RetainedWidgetTree::update` FNV-hashes every node's style + attribute strings per paint; skip clean subtrees using dirty bits → v1.27
- [ ] Fuse the five per-frame `finalize_tree` annotation walks into one traversal; move hot annotations from string attributes to typed `WidgetNode` fields → v1.27

### P1 — backend modules

- [ ] Investigate `pw-dump --monitor` as a real volume event source for the pipewire-audio backend — `pw-mon` emits no `changed:` block for volume changes (verified with and without `--hide-params`), so the stream currently only signals client/object lifecycle, and volume detection leans on the safety poll
### P1 — presentation and memory (→ v1.20)

- [ ] Add performance profiles for canonical shell workloads (idle, pointer move, text update, scroll, icon grid, animation, theme reload, resize) → v1.21
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

- [ ] **Per-paint element metrics: build → deep-compare → JSON→Lua convert,
      every frame.** `publish_element_metrics`
      (`shell/component/interaction_state.rs:41-65`) serializes _every keyed
      node_ to a `serde_json::Map` per paint, `set_host_value` deep-compares
      it, then `apply_element_metrics`
      (`scripting/context/runtime.rs:414-428`) converts the whole object to a
      Lua table **and** reinstalls bound element proxies — per frame, even
      when nothing scripted reads geometry that frame. Make `refs.<name>`
      reads lazy: keep metrics in a Rust-side store and resolve fields on
      `__index` (the proxy machinery already exists in `element_ref.rs`),
      publishing only a generation bump per paint; drop the eager
      `elements`/`refs` state tables or gate them on actual template reads.
      Progress 2026-07-02: the state-side deep-compare portion is removed for
      unchanged metrics via full-JSON fingerprints (see previous item), but
      eager JSON construction remains. Progress 2026-07-03: the scripting
      runtime now caches the last successfully installed refs fingerprint, so
      unchanged paints skip JSON→Lua conversion and bound-proxy reinstallation.
      A release benchmark over 20k unchanged publications measured 90.711ms for
      the eager path versus 0.140ms for the fingerprint-gated path (~647x
      faster). Rust-side tree walking/JSON construction and lazy `refs` field
      resolution remain open. Progress 2026-07-04: metrics publication is now
      gated by the retained-tree diff. Paint/style/state-only frames skip the
      Rust tree walk, JSON maps, fingerprints, runtime lock, and proxy update;
      layout, attribute, child, insertion, and removal changes still publish.
      A 1,365-node release microbenchmark over 2,000 unchanged passes measured
      23.236s for rebuilding snapshots versus 1.188us for the dirty-summary
      gate. Lazy field resolution remains open for frames where metrics really
      changed. Progress 2026-07-05: metric usage is now split between
      `elements` and `refs`, so ref/id-only components keep publishing the
      public `refs` table and live proxies without also building the all-node
      `elements` snapshot. A 341-node release benchmark over 2,000 changed
      publications measured 6.069s for collect-both versus 3.823s for
      refs-only (1.6x faster). Progress 2026-07-05: the collector now builds
      full JSON snapshots lazily only for nodes that actually publish to
      `elements` or `refs`, while reading scroll offsets directly for traversal
      through unpublished ancestors. A sparse-ref 341-node release benchmark
      over 2,000 publications measured 1.872s for eager per-node snapshots
      versus 205.920ms for lazy snapshots (9.1x faster). Lazy field resolution
      remains open for frames where metrics really changed.
- [ ] **Stringly-typed template expression values.** `eval_expr` returns
      `String` for everything (`frontend/compiler/src/expr.rs:26,162`);
      numeric ops re-`parse::<f64>` both sides per evaluation
      (`expr.rs:197`), `if` conditions compare against `"false"|"nil"|""|"0"`
      string literals, and every result is stored as an attribute `String`
      that downstream code re-parses. Introduce a small typed value enum
      (bool/number/string) for compiled-expression evaluation and only
      stringify at the attribute boundary — this also removes false
      attribute-hash dirtiness from float formatting.
  - [x] 2026-07-04: compiled expression evaluation now carries an internal
        bool/number/string value enum through boolean operators, ternaries,
        comparisons, concatenation, translation, and JSON variable reads, then
        stringifies only at the public `eval_expr` boundary. Numeric JSON
        comparisons avoid per-evaluation string allocation and `parse::<f64>`.
        A release benchmark over 500k numeric comparisons measured 36.848ms for
        the old string-parse shape versus 29.394ms for typed comparison (1.3x
        faster).
  - [ ] Attribute storage remains string-based until the downstream
        `WidgetNode`/style contracts are typed.

### B. Component communication & input

- [ ] **Handler dispatch overhead per event.** `call_namespaced_handler`
      locks the runtimes mutex, allocates 3 Strings for namespacing, and
      unconditionally runs `resync_binding_neighbors` over every linked
      instance after each handler (`shell/component/runtime.rs:494-560`).
      Track "did a cross-`_ENV` write actually happen" (a dirty bit set by
      the live-binding `__newindex`) and skip neighbor resync when clean;
      intern instance keys. Progress 2026-07-04: ordinary handler names now
      bypass legacy JSON-descriptor parsing unless the first byte is `{`;
      legacy pre-bound descriptors remain supported. A release benchmark over
      500k pointer-handler unpacks measured 43.866ms with failed JSON parsing
      versus 37.898ms with the syntax gate (1.2x faster). The binding-resync
      and instance-key allocation work remains open; a simple `__newindex`
      dirty bit is insufficient because Lua does not invoke it when replacing
      existing globals. Progress 2026-07-04: live `bind:this` proxies now set
      a per-runtime external-access flag only when another component writes
      through the proxy or calls a proxied function. Post-handler neighbor
      resync consumes that flag and skips untouched linked runtimes, while
      touched child-call semantics remain covered by
      `bind_this_event_handler_calls_child_live_and_resyncs_it`. A release
      benchmark over 2k untouched-neighbor checks measured 3.194ms for
      unconditional child resync versus 42.743us for the flag-gated skip
      (74.7x faster).
  - [x] 2026-07-04: plain handler argument unpacking now borrows the handler
        name and event args instead of cloning them into a fresh `String`/`Vec`
        on every dispatch; legacy JSON descriptors and typed pre-bound handler
        args still allocate only when merging is needed. A release benchmark
        over 500k plain handler transfers measured 40.768ms for clone-transfer
        versus 2.548ms for borrowed transfer (16.0x faster).
  - [ ] Instance-key interning remains open.
- [ ] **`bind:this`/live-binding writes mark the whole surface dirty.** Any
      handler that touches state invalidates via `invalidate_script_state()`
      → full template re-eval + tree rebuild (narrow path still rebuilds the
      full tree first — see v1.27 item). The typed state-dependency work
      (v1.18) should extend to handler writes: record which public members a
      template actually binds and skip rebuilds for writes nothing binds to.

### C. Events & service delivery

- [ ] **Per-event mutex churn in the observation gate.**
      `deliver_service_event` (`runtime/service_state.rs:167-184`) calls
      `observes_service_event` on every component per event, which locks that
      component's `runtimes` mutex and queries tracked-field maps
      (`shell_component.rs:261-268`); several tracked-field APIs clone whole
      maps/sets (`tracked_service_fields()`,
      `tracked_fields_for_service()` — `scripting/context/runtime.rs:478-497`).
      Maintain a shell-side subscription index (service → component indices),
      invalidated when a runtime's tracked fields/subscriptions change, so
      event routing is a lookup instead of N mutex acquisitions. Experiment
      2026-07-03: a component-local copied summary refreshed after paint/input
      was rejected and reverted. Its production-faithful single-runtime
      release benchmark improved the event gate only from 2.544ms to 2.221ms
      over 100k calls (1.1x), while refreshes had to clone the tracked maps.
      The shell-side index described above remains the viable design because it
      also eliminates the O(component count) scan rather than merely moving
      locks into refresh work.
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
      output _every_ frame is a full clear+repaint+full-damage present —
      partial damage only exists at integer scales. Fix the underlying
      logical-vs-physical damage-clip mismatch (compute/clip damage in
      physical pixels through the painter) so the retained partial path works
      at fractional scale. Likely the single biggest win on fractional-scale
      setups. Experiment 2026-07-03: a physical-damage rect fix was correct
      byte-for-byte against forced-full repaint, but rejected as a performance
      change for now. End-to-end release benchmarks did not show a meaningful
      CPU-side win: one-box 1200×600 forced-full 28.066ms vs partial 28.367ms,
      and large 3600×1800 forced-full 74.918ms vs partial 73.910ms. Revisit
      with compositor/upload damage instrumentation before marking this done.
- [ ] **Per-frame full-tree fingerprinting even when clean.**
      `RetainedWidgetTree::update` re-walks every node, hashes ~50 style
      fields + every attribute/handler string (FNV byte-at-a-time), allocates
      a snapshot (with a `child_ids` Vec) per node into a scratch map, and
      clones snapshots on change (`runtime_tree.rs:98-163,293-392`). The
      v1.27 "generation-aware diff" item covers skipping clean subtrees; add:
      hash with a word-at-a-time hasher (fxhash), reuse snapshot allocations
      in place (index by slotmap key instead of rebuilding the `NodeId` map),
      and stop hashing shell-owned annotation attributes that already have
      typed change tracking (`_mesh_scroll_*`, `_mesh_key`). Partial
      2026-07-03: `RuntimeTreeHasher` now implements primitive `write_*`
      methods so numeric style fields are mixed word-at-a-time instead of
      falling back to byte-at-a-time hashing. A release benchmark over 500k
      style fingerprints measured 118.362ms for the old byte fallback versus
      63.946ms primitive-aware (1.9x faster). Snapshot allocation reuse and
      broader shell-owned annotation filtering remain open. Progress
      2026-07-04: `_mesh_key` is no longer included in the attribute hash
      because the same identity is already encoded by the retained `node.id`;
      structural movement still changes parent `child_ids`. A 10-level-key
      release microbenchmark over 2M fingerprints measured 98.724ms with the
      redundant key hash versus 44.799ms without it (2.2x faster). Scroll and
      other shell annotations remain hashed until they have equivalent typed
      change tracking. Progress 2026-07-04: retained snapshot `child_ids` now
      use inline storage for up to eight children, eliminating the per-node
      heap allocation for normal UI trees while spilling safely for wider
      containers. A 4-child release microbenchmark over 2M snapshots measured
      9.811ms with fresh `Vec` allocation versus 2.810ms inline (3.5x faster).
      The transient retained dirty `SecondaryMap` now also swaps through a
      scratch slot instead of reallocating on each interaction update; a
      128-dirty-node release benchmark over 20k updates measured 6.622ms fresh
      versus 3.419ms reused (1.9x faster). Progress 2026-07-04: retained
      snapshot updates now remove stale nodes before draining the per-frame
      scratch map, then move changed/inserted `RetainedNodeSnapshot`s into
      slotmap storage instead of cloning them. Release benchmark:
      clone-transfer 216.847ms vs drain-move 177.698ms over 5.12M snapshot
      transfers (1.2x faster). Broader clean-subtree skipping and slotmap-keyed
      snapshot reuse remain open. Progress 2026-07-04: pre-bound event handler
      args in retained attribute fingerprints now hash `serde_json::Value`
      structure directly instead of allocating a serialized string for every
      arg. A release benchmark over 500k nested JSON arg fingerprints measured
      433.760ms for `to_string` hashing versus 92.355ms for direct typed
      hashing (4.7x faster).
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
      here because the _authoring_ of new annotations keeps growing the
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
      traversal. Partial 2026-07-03: promoted-popover collapse and generated
      error-placeholder constraints are now guarded by component-level presence
      flags set at the actual marker creation sites; normal trees skip both
      marker walks. Surface shortcut annotation now also returns before loading
      keyboard settings when neither manifest nor legacy settings declare
      shortcuts. A release microbenchmark over 20k plain-tree finalizations
      measured 361.497ms for the two old marker walks versus 2.375us for the
      gated path (152k×). The always-needed annotation/restyle/layout walks and
      broader traversal fusion remain open. Rejected experiment 2026-07-04:
      fusing surface and child enter/exit class annotation into one full-tree
      traversal measured 87.249ms versus 52.903ms for the existing targeted
      searches/subtree walks (0.6x). The prototype was reverted; any future
      fusion needs to avoid scanning unrelated branches. Progress 2026-07-04:
      text-selection annotation now resolves the selected `_mesh_key` with the
      existing keyed node lookup and annotates only that node instead of running
      a selection-specific recursive tree walk. Release benchmark on a broad
      tree: recursive 4.072s vs keyed 3.857s over 10k iterations (1.1x faster).
- [ ] **Layout + display list**: Taffy tree rebuilt per layout pass and
      display-list subtree flattening per update are already tracked
      (v1.21). Reaffirmed as the dominant structural-frame costs behind
      restyle in this scan; no new sub-findings. Progress 2026-07-04:
      render-object sync now reuses the per-update `dirty_nodes` allocation
      and replaces the separate `visited` hash set with an epoch mark stored on
      each retained render object. Release benchmark: visited set 158.594ms vs
      epoch marks 89.300ms over 20k synthetic updates (1.8x faster). Rejected
      experiment: changing render-object child IDs from `Vec` to `SmallVec`
      measured slower (11.285ms `Vec` vs 21.992ms `SmallVec`) and was reverted.
- [ ] **CPU Skia raster + SHM is the ceiling.** Painting is skia-safe CPU
      raster into `PixelBuffer` + SHM upload (`render/src/surface/painter/backend.rs`);
      blur/shadows/gradients are CPU per damaged pixel. GPU rendering is
      deferred (v1.25) — when it lands, prefer a `wgpu`/Skia-GPU surface per
      output with the retained display list as the command source, and keep
      SHM as fallback. Until then, the damage-path fixes above (especially
      fractional scale) are the effective lever.
### E. Style system — second-pass findings

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
  - [x] 2026-07-05: no-diagnostics theme default application now applies
        borrowed property names directly instead of constructing a temporary
        `Declaration` for every default on every node. A release benchmark over
        200k default applications measured 165.045ms for declaration allocation
        versus 154.446ms for direct property application (1.1x faster). Full
        pre-baked `ComputedStyle` prototypes remain open.
### F. Animation & layout per-frame overhead

- [ ] **Retained Taffy layout still re-syncs every node's style per pass.**
      `compute_incremental` → `update_retained_node_styles` walks the whole
      tree rebuilding `taffy_style_for_node` and re-populating
      `node_map`/`text_nodes` HashMaps on every layout-dirty frame
      (`ui/elements/src/layout.rs:346-390`), even when one node changed.
      Feed the retained-tree dirty set (already computed in
      `RetainedWidgetTree::update`) into layout so only dirty nodes get
      `set_style` calls — Taffy caches internally, but MESH pays the full
      style-conversion walk. (Structural rebuild case is tracked at v1.21;
      this is the _non-structural_ per-frame cost.) Progress 2026-07-04 for the
      paint-only case: when available geometry and layout dirtiness are both
      unchanged, `compute_incremental` now returns before rebuilding node/text
      maps, converting styles, or calling Taffy `set_style`. A layout-dirty or
      resized frame still synchronizes the full retained tree immediately
      before layout, preserving deferred correctness. The 1,365-node release
      microbenchmark over 2,000 paint-only passes measured 378.430ms for the
      old synchronization walk versus 40.369us for the fast path (9,374x).
      Dirty-node-only synchronization within actual layout passes remains a
      possible follow-up once retained-tree dirty IDs are exposed here.
      Rejected experiment 2026-07-04: retaining and clearing the temporary
      `node_map`/`text_nodes` allocations made the end-to-end layout pass
      slightly slower (77.502ms scratch versus 77.033ms fresh, 0.99x), because
      full style synchronization and map clearing dominate. The prototype was
      reverted.

### G. Lua runtime — state sync & handler overhead

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
      surface rebuild re-evaluates _every_ embedded/local component's
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
      Progress 2026-07-05: embedded/local runtime prop sync now uses a single
      runtime-map lock for existing instances and applies props directly to a
      newly-created runtime after its render hook, instead of inserting and
      looking it up again. A release microbenchmark over 1M existing-instance
      updates measured 26.851ms for `contains_key` + second `get_mut` lock
      versus 12.362ms for one `get_mut` lock (2.2x faster). Full subtree
      memoization remains open. Progress 2026-07-05: prop-bound handler
      matching now scans borrowed event-handler maps and only clones matched
      handler entries instead of cloning each node's full handler map before
      checking for matches. A 65-node no-match release benchmark over 50k
      passes measured 1.089s for clone-then-scan versus 877.216ms for borrowed
      scan (1.2x faster). Full subtree memoization remains open.
- [ ] **Display payload text still clones string attributes.** Display-entry
      comparison and paint-node creation still clone/deep-compare per-entry
      strings (`content`/`value`/`src`/`name`). Consider sharing node text via
      `Arc<str>` between `WidgetNode` and display entries, or introducing a
      compact interned attribute payload, after auditing the `WidgetNode` and
      renderer payload contract. Progress 2026-07-05: display primitive
      signatures now hash paint payload attributes by tag instead of hashing
      text/input/icon/slider attributes for every node. This also avoids
      display-list churn when irrelevant payload-like attributes change on
      generic nodes. A mixed 512-node release benchmark over 20k signature
      passes measured 925.336ms for all-payload-attrs hashing versus
      219.760ms for tag-aware hashing (4.2x faster). Actual string sharing
      remains open.
- [ ] **Storage reads clone per Lua access.** `self.storage.key` reads lock
      the storage mutex and clone the JSON value per access
      (`scripting/storage.rs:275-307`); render hooks that read storage pay
      this per frame. Minor today; becomes visible once handlers use
      storage more. Consider caching the storage table Lua-side and
      invalidating on write. Rejected experiment 2026-07-04: a private
      Lua-table cache for scalar values measured 50.238ms versus 39.202ms for
      the existing lock/clone/convert path (0.8x); the extra Lua lookup cost
      outweighed the saved Rust work, so the prototype was reverted. A future
      attempt should target shared immutable JSON values or lock avoidance
      without adding another Lua table lookup.
### J. Algorithmic complexity — quadratic hot-path patterns (fourth pass)

Targeted scan for accidentally-super-linear loops. These compound with each
other: an uncoalesced motion event multiplied by an O(depth × n) hover dispatch
multiplied by O(n) tree clones is where interaction latency actually goes.

- [ ] **Runtime key paths make deep trees O(n × depth).** Every node's key
      is the full slash-joined ancestor path built with
      `format!("{key}/{index}")` and FNV-hashed from scratch per node per
      frame (`runtime_tree.rs:616-622,281-291`) — a 10-deep list row hashes
      ~40-byte strings for every row every frame, and key length grows with
      depth. Derive ids by hash-chaining `(parent_id, child_index)` — O(1)
      per node, no string at all — and keep the string path only for debug
      builds / diagnostics. Progress 2026-07-04: runtime node IDs now hash-chain
      `(parent_id, child_index)` instead of rehashing each full ancestor path.
      IDs remain deterministic, nonzero, and sibling-distinct. A 10-level
      release microbenchmark over 500k iterations measured 36.394ms for full
      path hashing versus 5.755ms for parent chaining (6.3x faster). String
      paths are still built because interaction state and refs currently use
      them as public runtime keys; removing those allocations remains open.
- [ ] **`finalize_tree` closing-popover pass: O(closing-keys × tree)**
      `find_node_by_key_mut` per closing key (`rendering.rs:273-279`).
      Trivial count in practice; fold into the fused annotation walk (D)
      rather than fixing separately. Rejected experiment 2026-07-05: replacing
      the per-key searches with one full-tree inherited-state traversal was
      slower for the realistic small-key case (855.098ms existing per-key
      search vs 1.035s one-walk over 2k broad-tree iterations), so the
      prototype was reverted.
- [ ] **Slider drag worst case = every quadratic above at once.** Each
      uncoalesced motion during a drag runs slider-value tree walks ×3, a
      handler call (Lua + full `sync_state_from_lua`), then
      `invalidate_script_state()` → full template rebuild + restyle + layout + paint (`input/mod.rs:163-186`). With motion coalescing (above) plus
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
      cache _misses_ rasterize on the paint path. Route one-shot IO through
      `spawn_blocking` with a completion event (the loop already wakes on
      eventfd), and make icon-cache misses paint a placeholder frame and
      fill in on the next wake instead of stalling the frame.
### L. Live performance debugging — design

Goal: see hotspots _live_ while interacting with the shell, with cause
attribution (which rule, which component, which invalidation), without the
measurement tool perturbing what it measures. Builds on what already exists:
`ProfilingStage` accumulators + `ProfilingSnapshot` (`runtime/profiling.rs`),
`ProfilingInvalidationSnapshot` (per-paint rebuild/retained/narrow/damage
counts), the `DebugOverlay` painter, `mesh.debug.*` IPC, and the
debug-inspector's profiling start/stop. Tiered by effort:

- [ ] **Tier 1 — in-shell perf HUD painted by the renderer, not a module.**
      A HUD that is itself a `.mesh` surface would pollute the numbers with
      its own rebuild/restyle cycle at every update. Instead extend the
      existing `DebugOverlay` (which already paints layout bounds directly
      into the buffer post-paint, `frontend/render/src/surface/debug_overlay.rs`)
      with a profiling mode, toggled by the existing `CoreRequest` debug
      path: - **frame waterfall strip**: last ~120 frames as stacked bars (script /
      build / restyle / layout / display-list / paint / SHM / present),
      color-coded, 16.6 ms budget line — the data is already in
      `ProfilingSurfaceSnapshot.recent_samples`, it just needs a ring
      buffer keyed by frame rather than by stage; - **live counters**: FPS, presents vs skipped, damage area % of
      surface, retained-path vs full-rebuild ratio, narrow-path hits — all
      already in `ProfilingInvalidationSnapshot`, currently only visible in
      the inspector module; - **paint flashing** (the Chrome/KWin repaint debugger): translucent
      colored overlay on each frame's damage rects, decaying over ~300 ms.
      This makes "we repainted the whole bar for a clock tick" _visible_
      instantly, and is the single best tool for the repaint-suppression
      work in K. Trivial to add: the damage rects are already in
      `last_present_damage_rects` when the overlay paints.
      HUD paint cost must be excluded from the recorded stages (paint it
      after `PaintTraversal` is recorded) and its damage must not feed back
      into the damage stats (flag its rects).
- [ ] **Tier 2 — cause attribution (top-N tables).** Stages say _what phase_
      is slow; attribution says _why_: - per-style-rule cumulative restyle time + match count (time
      `apply_declaration` per rule id in the cached index; report top 10
      selectors); - per-component-instance build time (wrap `render_import`/embedded
      instance eval — directly measures the memoization win in I); - per-node paint time bucketed by command kind (text/shadow/blur/
      gradient/icon) — the painter already returns `PaintMetrics` with
      shaping/raster micros, extend to per-kind totals; - wasted-work counters: rebuilds whose retained diff was empty,
      restyles with zero changed styles, service deliveries whose payload
      was identical (K), motion events coalesced vs dispatched (J).
      Surface these in the HUD's second page and in the IPC snapshot.
- [ ] **Tier 3 — streaming + offline analysis.** - `mesh.debug.profiling_stream`: push per-frame profiling records over
      the existing IPC bus so an external `mesh-tools-cli perf top`
      TUI can show live tables without any in-shell UI (and without the
      HUD's paint cost); - Chrome-trace/Perfetto JSON export of a captured window (the
      `ProfilingSample` ring buffers already hold timestamps+durations) for
      offline flamegraph comparison before/after each A–K fix; - wire the existing `DebugBenchmarkSnapshot`/`BenchmarkScenarioSnapshot`
      types to the canonical-workload profiles item (v1.21): scripted
      scenarios (idle 10 s, pointer sweep, slider drag, popover open/close,
      theme switch) that run headless and emit a JSON summary — this is the
      regression harness that keeps the wins from A–K from rotting.
      Compare runs in CI against a stored baseline with a tolerance band.

### M. Component composition & template evaluation — 2026-07-04 deep dive

Focused trace of compile → `build_tree_with_state` → `build_widget_node` →
`FrontendCompositionResolver::render_import` → finalize (see
`PERFORMANCE_SECTIONS.md` §1 for the section map). New findings not covered by
passes A–L; `file:line` as of this scan.

Performance:

- [ ] **Full layout per embedded instance per rebuild.** `build_tree_with_state`
      always ends with `LayoutEngine::compute_with_measurer`
      (`frontend/compiler/src/lib.rs:203`) and `render_embedded_instance` calls
      it per embedded module instance mid-build; `finalize_tree` then re-lays-out
      the whole tree (`shell/component/rendering.rs:460`). Embedded subtrees get
      ≥3 layout passes per rebuild (+1 per nesting level). Verify nothing reads
      `node.layout` between build and finalize, then skip the build-time layout
      for `FrontendRenderMode::Embedded` (and likely the surface build too).
- [ ] **`{#for}` deep-clones the whole items array every rebuild.**
      `store.get(&for_node.iterable)` (`frontend/compiler/src/render.rs:429`)
      uses owned `get` although borrowed `get_ref` exists and is already used by
      `eval_path`. Switch to `get_ref`; trivial diff.
- [ ] **Post-hoc full-subtree walks per embedded instance.**
      `namespace_event_handlers` re-`format!`s every handler string on every
      rebuild (`ui/interaction/src/hit_test.rs:359`) even though
      `build_widget_node` already receives `instance_key` — namespace during
      `parse_attributes` instead and the walk disappears.
      `apply_prop_handler_calls` clones each node's whole `event_handlers` map
      and does an O(handlers × props) scan per node
      (`shell/component/composition.rs:213-239`).
- [ ] **Per-rebuild prop churn.** `ensure_runtime`/`ensure_local_component_runtime`
      re-`set` every prop into script state per instance per rebuild with 2–3
      runtimes-mutex acquisitions (`shell/component/runtime.rs:408-415`);
      `render_import` rebuilds `props_json` maps and `format!`s instance keys per
      frame (`composition.rs:25-38,90-98`); host+component style-rule slices are
      re-cloned into a merged `Vec` per instance per rebuild
      (`render.rs:266-278`) — cacheable per (host, alias).
- [ ] **Per-node build allocations.** `attach_module_id` inserts a fresh
      `_mesh_module_id` String on every node; `TrackingVariableStore` pushes two
      fresh Strings per dotted read per node; `resolve_event_handler_value` does
      an owned `store.get` per handler attribute. Folds into v1.23 interning but
      listed because composition keeps adding string attributes.

Structure / correctness:

- [ ] **`and`/`or` template expressions diverge from Lua semantics.**
      `eval_compiled` returns literal `"true"`/`"false"` for `And`/`Or`
      (`frontend/compiler/src/expr.rs:193-204`) instead of the operand values —
      `{name or "Anonymous"}` renders `true`/`false`; only the exact
      `cond and a or b` ternary shape is special-cased to work. Also
      `is_truthy` treats `"0"`/`""` as falsy (Lua does not), and
      `a or b and c` parses with inverted precedence (`and` split before `or`).
      Fix as part of the typed expression-value enum (section A, "stringly-typed
      template expression values") — that item is now correctness work, not just
      an optimization. Doc example using unsupported C-style `?:` fixed in
      `docs/frontend/mesh-syntax.md` 2026-07-04.
- [ ] **Build is not a pure function — prerequisite for render memoization.**
      `render_import` mutates shell state during build via RefCells
      (`pending_surface_states`, `portal_hidden_bindings`,
      `has_promoted_popover_wrappers`, live `bind:this` installation —
      `composition.rs:74-131`). Component-level memoization (section I) would
      silently skip these side effects when serving a cached subtree; make them
      explicit build outputs (a `BuildEffects` struct the caller applies) first.
- [ ] **Typed handler-call linkage matches by value equality.**
      `apply_prop_handler_calls` maps typed args onto child handlers by
      comparing resolved handler *values* to prop values
      (`composition.rs:221-235`); two props bound to the same handler name get
      the wrong args. Link by prop name through the child build instead.
- [ ] **Remove the legacy JSON handler-descriptor path.** `unpack_handler_args`
      still parses `{"h":…,"a":…}` strings (`shell/component/runtime.rs:644-664`)
      after typed `EventHandlerCall` landed (section G). Per the
      no-backward-compat project rule, verify nothing produces them and delete.
- [ ] **`{#if}`/`{#for}` always wrap children in a synthetic `column` node**
      (`render.rs:394,423`) — one extra node per conditional/loop paying layout,
      style, hash, and paint, and it forces column flow inside row parents.
      Needs a fragment/transparent-container concept.
- [ ] **No keyed list diffing.** `{#for}` identity is positional (`_mesh_key`
      paths), so any reorder/insert re-styles and re-hashes every following row.
      Add a `key=` attribute; pairs naturally with component memoization
      (section I) and the retained-tree diff work (v1.27).
- [ ] **Magic-string protocol at the composition boundary.**
      `__mesh_embed__::`, `__mesh_binding_*`, `__mesh_bind_this`,
      `_mesh_module_id`, the promoted-popover marker — stringly-typed channels
      between compiler and shell causing prefix parsing and false attribute
      dirtiness. The composition-boundary instance of v1.23 typed fields.
- [ ] **Verify dynamic `class={expr}` bindings participate in build-time style
      resolution.** `parse_attributes` only feeds `classes` from Static values
      (`render.rs:760-767`); a fully-dynamic class lands in `resolved` and is
      skipped by the build-path `resolve_node_style_for_module_indexed` call,
      relying on the finalize restyle to correct it. Confirm and either resolve
      dynamic classes at build or document the two-pass behavior.
- [ ] Minor: `render_import`'s local-component branch does its catalog lookups
      twice (gate in `composition.rs:22-23`, again inside
      `render_local_component`, `runtime.rs:435-440`).

### N. Retained tree, render objects & display list — 2026-07-04 deep dive

Focused trace of annotate → `RetainedWidgetTree::update` → `RenderObjectTree`
→ `RetainedDisplayList` → damage (see `PERFORMANCE_SECTIONS.md` §2). New
findings beyond the D/I/J items; `file:line` as of this scan.

Performance:

- [x] **`ordered_entries` is built per display-list rebuild but consumed only in
      debug builds.** `collect_display_entries` pushes every `(key, entry)` pair
      into a Vec (`render/src/display_list.rs:770-774`) whose sole consumer is
      `compute_batch_metrics` behind `#[cfg(debug_assertions)]`
      (`display_list.rs:891-894`). Release builds pay a full per-entry Vec push
      every rebuild frame for nothing. Gate the collection itself (pass
      `Option<&mut Vec<_>>` or a debug-only sink). Free win.
      Completed 2026-07-05: release builds now compile out the ordered-entry
      scratch buffer and pass no debug sink during entry collection. Two
      release runs over 9.842 million collected entries measured 2.943s versus
      2.881s and 2.941s versus 2.930s (0.4-2.1% faster), with identical damage
      map entry counts.
- [ ] **`RenderObjectTree` allocates per node per dirty frame.** `text_slot`
      clones the text `content` String (`render/src/render_object.rs:307`),
      `accessibility_slot` clones the label, `child_id_slot` allocates a fresh
      `Vec<NodeId>` per node (`render_object.rs:263-271`; the retained tree
      already uses an inline `SmallVec` for the same data), `geometry_slot`
      string-parses six `_mesh_scroll_*`/`_mesh_content_*` attributes per node
      (`render_object.rs:296-301`), and `update_inner` allocates two fresh
      `HashSet`s per update (`render_object.rs:97-98`) instead of scratch-reuse.
      This file predates the D-item optimizations and never got them.
- [ ] **Triple full-tree fingerprinting on every dirty frame.** Three parallel
      diff systems each walk the whole tree and hash/compare overlapping data:
      `RetainedWidgetTree` snapshots (layout/style/attrs/children/state,
      `runtime_tree.rs:102-170`), `RenderObjectTree` paint-data slots
      (transform/clip/geometry/material/primitive/text, `render_object.rs:90-124`),
      and `RetainedDisplayList` per-(node, slot) entry signatures — which
      `collect_display_entries` recomputes for **every** node on any dirty frame
      (`display_list.rs:1384-1433`) even when the dirty set names one node.
      The retained-tree generation gates the clean-frame case only. Unify:
      make `RetainedWidgetTree` the single fingerprint pass and have the render
      object tree and display entries consume its per-node dirty flags,
      re-signing entries only inside dirty subtrees (plus scrolled/moved
      ancestors). This is the §2 complement of the v1.27 generation-aware diff.
      Progress 2026-07-05: display-list batch signatures now hash only the
      material fields relevant to each primitive slot, and entries that already
      carry a batch barrier skip batch-signature hashing entirely because the
      metric never compares them. A 512-node release benchmark over 50k
      background-slot signature passes measured 804.926ms for the previous broad
      material hash versus 69.806ms for the slot-aware hash (11.5x faster).
- [x] **Reused paint subtrees are cloned twice per clean node.**
      `build_paint_subtree`'s reuse path does `previous.clone()` then
      `next_subtrees.insert(id, reused.clone())`
      (`display_list.rs:1488-1491`) — Arc bumps plus span/kind vec copies for
      every clean node on every incremental rebuild. Insert once and return a
      cheap handle/index instead.
      Completed 2026-07-05: retained subtree maps now own whole-subtree `Arc`
      handles, so clean-node reuse clones one handle rather than cloning each
      shared command/kind/order field plus metadata. A release benchmark over
      10 million reuse clones measured 180.455ms fieldwise versus 28.002ms for
      the whole-subtree handle (6.4x faster).
- [ ] **Two more full passes per display-list rebuild.**
      `build_command_spans(root, &subtrees)` walks the tree and
      `count_effect_overflow_commands` scans all commands
      (`display_list.rs:895-896`) on every rebuild; both derivable
      incrementally from the subtree reuse bookkeeping.
- [ ] **Scroll state round-trips float→string→float three times per node per
      frame.** Written as `"{:.2}"` strings in `annotate_runtime_tree`
      (`runtime_tree.rs:819-832`), re-parsed in `collect_display_entries`
      (`display_list.rs:1417-1426`), `build_paint_node` scrollbars (six
      `attr_f32` calls), and `geometry_slot` (six more). Also quantizes offsets
      to hundredths. The concrete §2 instance of the v1.23/v1.27 typed
      `WidgetNode` fields item.
- [ ] **Handler-call args re-serialize to JSON strings per fingerprint.**
      `attributes_fingerprint` does `arg.to_string()` per pre-bound arg per
      node per frame (`runtime_tree.rs:479`). Hash the `serde_json::Value`
      structurally instead.

Structure:

- [x] **The primitive-aware hasher improvement never reached the render crate.**
      `RuntimeTreeHasher` got word-at-a-time `write_*` methods (D, 1.9x), but
      `DisplaySignatureHasher` (`display_list.rs:1305-1325`) and
      `RenderObjectHasher` (`render_object.rs:51-70`) are still byte-at-a-time
      FNV copies. Either port the primitive methods or — better — share one
      hasher type; three hand-rolled FNV implementations is the maintenance
      smell that let this drift.
      Completed 2026-07-05: both render hashers now mix primitive values in one
      operation while preserving byte-wise hashing for strings and slices. A
      release benchmark over a representative primitive field mix measured
      3.072ms for the byte fallback versus 2.232ms word-at-a-time (1.4x
      faster across 5 million iterations).
- [ ] **No `NodeId` collision detection.** Runtime ids are FNV/chained hashes
      of key paths (`runtime_tree.rs:346-365`) used as identity keys by all
      three retained systems and the display-list keys; a collision silently
      aliases two nodes (wrong reuse, wrong damage) with no diagnostic. Add a
      debug-build assertion where `node_keys` is populated.
- [ ] **Identity travels as a string attribute.** `annotate_runtime_tree`
      writes `_mesh_key` into `attributes` (`runtime_tree.rs:711`) purely so
      interaction/refs/metrics can read identity back out of a string map,
      which in turn forced the `_mesh_key` hash-exclusion special case in
      `attributes_fingerprint`. Typed field on `WidgetNode` (v1.23) retires
      both.
- [ ] Minor: display-list `update_inner` is ~220 lines mixing diff, damage,
      and a ~30-field metrics struct assembly (`display_list.rs:742-961`);
      split when next touched.

### N addendum — 2026-07-04 second pass (display-list subtree internals)

- [ ] **Every rebuilt ancestor copies its entire descendant command list.**
      `PaintSubtreeBuilder::append_child` does
      `extend_from_slice(&child_subtree.commands)`
      (`display_list.rs:586-600`), so each ancestor's flat buffer holds copies
      of all descendant `DisplayPaintCommand`s, and `next_subtrees` retains a
      full flattened copy per node — O(n × depth) command storage and re-copy
      on every ancestor rebuild. This is the retained-memory face of the v1.21
      segment/rope item; fixing v1.21 should make per-node subtrees hold spans
      into shared storage, not owned flattened copies.
- [ ] **A dirty node rebuilds its entire subtree's paint segments.**
      `build_paint_subtree` passes `force_rebuild || node_is_dirty` down to all
      children (`display_list.rs:1563`), so a style-only change on a container
      (hover background) rebuilds every descendant's commands even though
      their geometry and content are unchanged. Only the dirty node's own
      commands need rebuilding when its layout/scroll/clip didn't change;
      children could be re-appended from the previous subtree.
- [ ] **`DisplayPaintCommand` embeds a full cloned `DisplayPaintNode` per
      command.** `paint_node.clone()` per Node command
      (`display_list.rs:1524`), with the same node reused for the Scrollbars
      command — each clone copies text/placeholder Strings and the style
      block. Share via `Arc<DisplayPaintNode>` per node with per-command kind.

### O. Style system & theming — 2026-07-04 deep dive

Focused trace of CSS parse → `StyleRuleIndex` → `StyleResolver` →
`ComputedStyle` (build, restyle, and diagnostics paths) plus theme defaults.
See `PERFORMANCE_SECTIONS.md` §3. `file:line` as of this scan.

Performance:

- [ ] **Hidden second full restyle with per-node index construction on every
      rebuild frame.** `record_runtime_style_diagnostics` runs whenever a
      diagnostics sink is attached — which is always in production
      (`shell_component.rs:60`) — on every `"rebuild"`-trigger finalize
      (`rendering.rs:429-431`). It walks the whole tree re-resolving every
      node through the diagnostics path, which builds `StyleRuleIndex::new(rules)`
      **per node** (`resolve.rs:546`) — the exact O(nodes × rules) pattern the
      E-item fixed on the build path — plus a fresh `Vec<String>` classes clone
      and a fresh variables HashMap per node (`rendering.rs:584-590`,
      `resolve.rs:614`). Rebuild frames are the most common invalidation class
      (every service update / handler write). Fix in stages: thread the cached
      index through the diagnostics path; gate the pass on (rules generation,
      tree-structure generation) instead of every rebuild; long-term validate
      declarations once per rule at compile time and delete the runtime pass.
- [ ] **Per-declaration static validation re-runs per node per pass.**
      `apply_declaration_no_diagnostics` runs `style_profile_status`,
      `is_supported_css_property`, `contains_deprecated_token_reference` (a
      string scan of the value), and `is_strict_animation_property` for every
      declaration of every matched rule on every node on every restyle
      (`resolve.rs:916-950`). All are pure functions of the declaration;
      precompute them once per rule into a validated/compiled declaration at
      rule-build time. Cheap first step toward the v1.23 typed-declarations
      item.
- [ ] **`seed_module_theme_variables` allocates two Strings per module token
      per node per pass** — `format!("--{}", name.replace('.', "-"))`
      (`resolve.rs:857-876`). Precompute the CSS-variable-keyed token map once
      per theme load per module and seed by reference.
- [ ] **`seed_prop_variables` clones every prop key+value per node**
      (`resolve.rs:599-603`) even though props are per-instance constants for
      the whole pass. Seed once per pass or resolve through a layered lookup
      (props map consulted after scratch) instead of copying.
- [ ] **`theme_reference_to_token_name` allocates and canonicalizes per
      `var()` reference per declaration per node** (`resolve.rs:1916-1922` +
      `css_custom_property_to_token_name` prefix tables). Double-key theme
      tokens by their CSS custom-property name at theme load, or intern the
      mapping, so hot lookups are a single hash probe.
- [ ] Confirmed mechanism for the existing "pre-bake per-tag prototypes" item:
      `apply_theme_defaults_map_no_diagnostics` re-clones each default's
      property String and re-classifies its value per node per pass
      (`resolve.rs:901-914`), for "base" + tag + module-base + module-tag maps.

Structure / correctness:

- [ ] **Theme component defaults apply in nondeterministic order.**
      `ComponentDefaults = HashMap<String, String>`
      (`foundation/theme/src/lib.rs:12`) and `apply_theme_component_defaults`
      iterates it per node. A theme declaring an overlapping shorthand +
      longhand pair (e.g. `background` and `background-color`) on the same
      component resolves in random order per process run, and theme-CSS source
      declaration order is lost entirely at parse. Store defaults as an
      ordered `Vec<(String, String)>` preserving source order (CSS last-wins).
- [ ] **The diagnostics/no-diagnostics path duplication caused the drift.**
      Four near-identical function pairs (`resolve_node_style_with_attrs*`,
      `apply_theme_defaults_map*`, `apply_declaration_*`) exist so the
      diagnostics path could stay separate; that duplication is exactly where
      the per-node index rebuild survived. When fixing the first item, fold
      diagnostics into a sink parameter (`Option<&mut Vec<StyleDiagnostic>>`)
      on one path so the two cannot diverge again.
- [ ] Design note (fine, but document): selector matching has no CSS
      specificity — candidate rules apply in source-index order (last wins),
      and descendant combinators are rejected at parse with a diagnostic
      (`ui/component/src/style.rs:100`). Worth one paragraph in
      `docs/spec/04-styling.md` so authors don't expect specificity semantics.

### P. Rendering & paint — 2026-07-04 deep dive

Focused trace of `paint()` → damage assembly → `paint_pixel_regions` →
display-list replay → Skia session → text/glyph/icon caches → buffer. See
`PERFORMANCE_SECTIONS.md` §4. `file:line` as of this scan.

Performance:

- [ ] **File-backed icon draws stat() the filesystem every paint, even on
      cache hits.** Every draw computes `raster_file_key` → `file_freshness`
      → `std::fs::metadata` (`render/src/surface/icon.rs:134-145,179-190`),
      and SVG sources add a second freshness check via `svg_file_cacheability`
      (`icon.rs:211`). Freshness is part of the raster cache key, so a hit
      still pays the syscall; `cached_file_resource_opacity` (opaque-region
      derivation) stats again per present (`icon.rs:297-331`). A bar with ~10
      file icons at 60 Hz is 600–1800 blocking syscalls/s on the paint path,
      and a slow filesystem stalls the frame. Fix: TTL the freshness probe
      (re-stat at most every ~1s) or make invalidation event-driven through
      the shell's existing inotify hot-reload watcher, so steady-state paints
      do zero filesystem calls. Named-icon *font glyph* draws are unaffected
      (glyph caches key by path hash + axes).
- [ ] **Child popup surfaces bypass the whole retained pipeline.**
      `paint_child_surface` (`shell/component/shell_component.rs:992-1027`)
      clears the entire child buffer and repaints the popover subtree through
      the immediate-mode `paint_frontend_tree_at_for_module` on every present,
      plus two full-tree walks (`find_node_by_key`, `find_node_bounds_by_key`)
      per child per frame. An open hover menu or quick-settings popover
      full-repaints at frame rate with no display list, no damage, no partial
      present. Route child targets through the same retained display-list +
      damage path as the parent (subtree-scoped), which also deletes the
      duplicate immediate-mode painter (structure item below).
- [ ] **Any non-clean frame bypasses all generation shortcuts.**
      `use_generation_shortcuts` requires `dirty_types.is_empty()`
      (`shell_component.rs:529-537,560-581`), so every interaction/animation/
      script frame runs `RenderObjectTree::update` and display-list entry
      collection as full-tree passes. This is the shell-side counterpart of
      the §N triple-fingerprint item — fixing §N must include widening this
      gate to per-node dirty scoping, not only the fully-clean case.
- [ ] **Rotation transforms allocate a temp `PixelBuffer` per node per
      frame** and recursively repaint the subtree into it before the rotated
      blit (`render/src/surface/painter/tree.rs:380-410`). Any animated
      rotation pays an allocation + full subtree repaint per frame; reuse a
      cached temp buffer keyed by size class. Low priority until rotation is
      used in shipped surfaces.
- [ ] **Minor inner-loop allocations in the Skia backend.**
      `execute_commands_on_canvas` allocates clip/layer stacks per batch
      (`painter/backend.rs:479-480`); the gradient shader cache key includes
      absolute rect position (`backend.rs:18`), so an animated/moving gradient
      re-creates its shader every frame and can thrash the 64-entry LRU — key
      by size only and translate the canvas, or accept and document.

Structure:

- [ ] **Every widget is painted by two parallel implementations.** The
      immediate-mode path (`render_tree*`/`render_node_with_filter`,
      `render_input_node`, `render_slider_node`, `render_icon_node`,
      `render_scrollbars`) duplicates the display-list path
      (`render_display_*` twins in `painter/widgets.rs`, `painter/tree.rs`)
      for input, slider, icon, scrollbar, and text painting. Same
      pair-duplication hazard as §O's diagnostics split — behavior drift
      between parent surfaces (display list) and child popups/tooltips
      (immediate mode) is silent. Converge on the display-list path (unblocked
      by the child-surface item above) and delete the immediate-mode twins.
- [ ] Text stack is healthy (layout LRU + glyph atlas + ellipsis cache with
      `Cow` fast path); remaining text work is the cache-pressure visibility
      + locale-sensitive workload items already tracked from
      `TEXT_RENDERING_TODO.md`. No new text findings.

### Q. Interaction & input — 2026-07-04 deep dive

Focused trace of `handle_component_input` (pointer/scroll/keyboard),
hover/tooltip transitions, element actions, and focus/scroll helpers. This
section already absorbed the B/J optimization passes; findings below are what
remains. `file:line` as of this scan.

Performance:

- [ ] **Keyboard input reads and JSON-parses settings files from disk on
      every key event.** `current_keyboard_settings()` calls
      `load_shell_settings()` (`input/keyboard.rs:340-344`), which does up to
      two `fs::read_to_string` + JSON parse + merge (`config/src/lib.rs:374-390`)
      — invoked per `KeyPressed`, per `KeyReleased`, and per `Char`
      (`keyboard.rs:41,167,516,531`, `input/mod.rs:343`). Typing in a launcher
      input costs 2–4 file reads + parses per keystroke, blocking the shell
      thread. Cache `KeyboardSettings` on the component (or shell) and
      invalidate through the existing settings hot-reload/inotify path — the
      same infra module settings reloads already use.
      `resolved_surface_shortcuts` (rebuilt per keypress with locale lookups)
      becomes cacheable the same way.
- [ ] **Click press/release still runs ~5–8 separate full-tree walks.**
      Press: `selectable_text_target_key`, `pointer_event_target_key`,
      `find_node_bounds_by_key`, `find_focusable_at`, then per-kind probes
      (`is_slider_key`/`is_option_key`/`is_radio_key`/
      `is_checkable_choice_key`); release: `pointer_event_target_key`,
      `find_click_handler`, `build_click_event` (`input/mod.rs:52-179`).
      Clicks are rare so this is latency (not throughput), but on large trees
      it's the same pattern the motion path already fixed — extend
      `pointer_hit_test` to also return focusable/selectable/kind/handler info
      in its single traversal.
- [ ] **Scroll events do two extra walks** — `find_scrollable_at` then
      `find_node_by_key` for limits (`input/mod.rs:307-309`); fold the
      scrollable ancestor + limits into the fused hit-test result.
- [ ] Minor: `apply_element_actions` clones the whole `ref_node_keys`
      HashMap per action batch (`interaction_state.rs:91`); hover-change path
      clones `Vec<String>` paths (`input/mod.rs:214-240`) — both retire with
      the string-key → `NodeId` migration (§N / J open item).
- [ ] Confirmation for the tracked slider-drag item (J): the unconditional
      `invalidate_script_state()` per coalesced drag motion is at
      `input/mod.rs:193-200` with a comment explaining why state-dirty
      detection was insufficient — the fix needs slider knob position painted
      from shell-owned `slider_values` via the STATE path plus a paint-only
      text update, exactly as the J item describes.

Structure:

- [ ] Interaction identity is string-keyed end to end (`hovered_path:
      Vec<String>`, `focused_key`, `scroll_offsets`, `input_values`,
      `slider_values` all keyed by `_mesh_key` strings). This is the consumer
      side that keeps the §N "identity travels as a string attribute" problem
      alive; the NodeId migration should convert these maps together with the
      metrics/refs publication so the string keys can finally disappear.
- [ ] Otherwise healthy: pointer-motion is fused single-traversal with
      coalescing, hover dispatch resolves all transitioning nodes in one walk,
      scroll animations early-out when idle, stale-target pruning is
      probe-based. No further structural findings.

### R. Script runtime & Lua boundary — 2026-07-04 deep dive

Focused trace of `call_handler` → `sync_state_from_lua` → `ScriptState` →
`refresh_module_object`, plus the VM pool and backend runtime. The G-item
optimizations (write-log discovery, side-channel flag, cached self table,
proxy seen-cache) are confirmed in place; findings below are what remains.
`file:line` as of this scan.

Performance:

- [ ] **`refresh_module_object` re-serializes the entire component state per
      handler call for every proxy-bearing component.** Any component that
      `require`s a service interface registers state proxies, so
      `has_proxies()` is true and the generation skip never applies
      (`context/runtime.rs:1777-1793`). Every handler and render hook then
      pays: `state.snapshot()` with proxies — which **bypasses the snapshot
      cache and deep-clones every variable's JSON** plus invokes every proxy
      getter (`context/state.rs:222-231`) — followed by a full JSON→Lua
      conversion and a `module.state` table write. And per
      `docs/modules/frontend/core/README.md:64`, `module.state` is a legacy
      v1.12 compatibility lane; no shipped module reads it. Verify no internal
      consumer remains, then delete the refresh (and the lane) per the
      no-backward-compat rule — likely the single largest remaining boundary
      cost for service-connected components.
- [ ] **The sync "fast path" still round-trips every known global per
      handler.** For each user global: env read + `from_value` Lua→JSON
      conversion + `state.set` deep-compare, changed or not
      (`context/runtime.rs:1678-1687`). The write log fixed discovery only.
      Because Luau `__newindex` does not fire for existing table keys, a true
      per-write log needs `_ENV` to become a forwarding proxy (empty table
      with `__index`/`__newindex` to a backing store) — or invert ownership:
      keep values in Rust and expose globals through the proxy so there is no
      sync at all. Measure script read-through cost first; pairs with the
      v1.17 per-thread-VM work.
- [ ] **`ScriptState::snapshot()` with proxies has no caching.** The
      non-proxy branch caches by generation; the proxy branch rebuilds and
      deep-clones everything on every call (`state.rs:196-231`). Even after
      the `module.state` deletion, remaining `snapshot()` callers pay this —
      cache the variables portion by generation and overlay proxy getters.
- [ ] Minor: `sync_module_exports_from_lua` runs per sync (module table read
      + `from_value` + `set`) even for components that export nothing
      (`runtime.rs:1765-1775`); record "has exports" once at script load and
      skip.

Structure:

- [ ] **Legacy `module.state` / `module.exports` lanes.** Documented as
      compatibility-only (`docs/modules/frontend/core/README.md`), but they
      still drive per-handler work (items above). Audit consumers and remove
      per the no-backward-compat rule; if `module.exports` is still the
      mechanism behind component exports, rename/keep that half explicitly
      and document it as current, not compat.
- [ ] Healthy: `LuaVmPool` sandboxing with baseline-global capture, cached
      lifecycle self table, flag-gated side channels, storage read tracking,
      interface-proxy seen-field cache, backend snapshot only on emit paths.
      No further findings.

### S. Events, services & backends — 2026-07-04 deep dive

Focused trace of `broadcast_service_event` → dedup/validation → delivery,
the `InterfaceRegistry`, and the backend service loop. `file:line` as of this
scan.

Performance:

- [ ] **`InterfaceRegistry::resolve` deep-clones the entire interface catalog
      on every call.** `resolve()` goes through `catalog()`, which clones the
      full contracts map **and** providers map (every contract's state fields,
      events, and commands for every interface)
      (`extension/service/src/interface.rs:54-56,86-91`), then clones the
      matched contract again (`interface.rs:126-133`). It is called per
      accepted service state update (`validate_service_state_shape`,
      `shell/runtime/service_state.rs:228`), per named interface event
      (`service_state.rs:243`), and per service command dispatch
      (`shell/runtime/request.rs:774,814`). Every audio update and every
      volume command deep-clones every registered contract. Fix: resolve
      directly under the read lock and return `Arc<InterfaceContract>`;
      keep `catalog()` for the debug/discovery paths that genuinely want a
      snapshot.
- [ ] **Contract validation re-derives typed information per event.**
      `json_value_matches_contract_type` allocates a lowercased String per
      field per update (`service_state.rs:401-415`), and named-event payloads
      re-parse the inline schema **string** on every event
      (`parse_inline_object_schema`, `service_state.rs:345,375-395` — also
      hand-rolled string parsing, which project policy treats as migration
      debt). Precompile contract field types and event schemas into typed
      enums at contract-registration time; validation becomes match arms with
      zero allocation.
- [ ] Minor: `canonical_interface_name` / `service_name_from_interface`
      allocate fresh Strings 2–3× per event across normalize/record/profiling
      (`service_state.rs:44,92`, `interface.rs:95-118`); thread the canonical
      name through instead of re-deriving, or intern interface names (v1.23).

Structure:

- [ ] Concrete citation for the tracked "eliminate service-specific Rust
      branches" item: the hardcoded `mesh.audio` optimistic-mute merge lives
      in `normalize_service_event` (`service_state.rs:66-75`) and
      `apply_optimistic_audio_muted_state` (`service_state.rs:137-165`).
      The generic replacement is an optimistic-state declaration in the
      interface contract (field + command linkage) so core stays
      service-agnostic.
- [ ] Healthy/confirmed: shell-boundary payload dedup before delivery,
      wake-level coalescing with barriers, backend-side dedup
      (`publish_changed_update` + `last_payload`), stream line batching per
      program, `Arc<Event>` bus. The open C items (shell-side subscription
      index, push-based host API primitives) remain the section's structural
      backlog.

### T. Layout — 2026-07-04 deep dive

Focused trace of `compute_incremental` → retained style sync → Taffy compute →
text measurement. Confirms the F-item paint-only fast path is in place
(`layout.rs:347-355`). `file:line` as of this scan.

Performance:

- [ ] **Unconditional `set_style` per node defeats Taffy's internal caching on
      every layout-dirty frame.** `update_retained_node_styles` converts all
      ~60 style fields (`taffy_style_for_node`) and calls
      `state.tree.set_style` for **every** node whenever layout is dirty
      (`ui/elements/src/layout.rs:811-855`), and `set_style` invalidates that
      node's Taffy layout cache — so one changed node forces Taffy to
      recompute as if everything changed. The retained tree already computes
      per-node STYLE/LAYOUT dirty flags (§N); feeding them here so only dirty
      nodes get converted + `set_style` is the mechanism that makes the
      existing "dirty-node-only sync" item (F) pay off twice: skips the
      conversion walk *and* preserves Taffy's caches for clean subtrees.
- [ ] **Text measurement clones the content String twice per node per pass.**
      `update_text_context`/`build_taffy_tree` clone every text node's
      `content` into `TextMeasureData` per layout-dirty and structural pass
      (`layout.rs:857-884,580-596`), and `TextMeasureKey::new` clones it
      **again** per measure probe — including on cache hits, since the owned
      key is built just to probe the LRU (`layout.rs:119-130`). Fix: share
      content as `Arc<str>` (the §N `Arc<str>` payload item's layout face) and
      probe the intrinsic cache with a borrowed/hashed key instead of an owned
      one.
- [ ] **Structural reconcile is string-keyed and clone-heavy.**
      `reconcile_retained_taffy_node` clones each node's `_mesh_key` String
      (`layout.rs:773-810`), `collect_mesh_keys` clones every key into a
      `HashSet<String>` per structural pass (`layout.rs:901-908`), and the
      stale sweep clones + length-sorts keys (`layout.rs:706-722`).

Structure:

- [ ] **The LAYOUT-03 string-keying rationale is obsolete.**
      `PerSurfaceLayoutState.node_map` is keyed by `_mesh_key` String with a
      comment "NOT ephemeral NodeId per LAYOUT-03" (`layout.rs:144-146`) — but
      runtime NodeIds are no longer ephemeral: they are stable hash-chained
      ids derived from the same key paths (§J progress, `runtime_tree.rs`).
      Re-keying `node_map` by `NodeId` removes every string clone above and
      the per-node hash of long key strings in `retained_taffy_id`
      (`layout.rs:910-915`). Do together with the §Q interaction-map NodeId
      migration so `_mesh_key` strings have no remaining hot consumers.
- [ ] Healthy/confirmed: paint-only frames skip all layout sync; fresh
      `node_map`/`text_nodes` maps per pass were measured (scratch reuse
      rejected 2026-07-04); intrinsic text cache is LRU-bounded; Taffy
      diagnostics are report-gated.

### U. Presentation & memory — 2026-07-04 deep dive

Focused trace of `present_with_damage` → SHM pool copy → `attach_shm_buffer` →
commit, plus popup promotion, scale/blur/input-region protocol handling, and
input normalization in `crates/core/presentation`. Existing H items (direct
Skia-into-SHM paint, size-class pools, `copy_bgra_to_canvas` cites) still
stand; findings below are additional. `file:line` as of this scan.

Performance:

- [ ] **Per-buffer pending damage is a single bounding rect, which forces the
      SHM copy to be a union.** `SurfaceShmBuffer.pending_damage` is
      `Option<DamageRect>` (`presentation/src/wayland_surface/backend.rs:73-76`),
      accumulated via `union_damage` (`backend.rs:270-283`), and
      `present_with_damage` folds the frame's multi-rect damage into one union
      before the copy (`backend.rs:1174-1183`, the "Pitfall 1" comment). Two
      small disjoint changes on one surface — clock text at the left of a bar
      plus a volume icon at the right — memcpy the entire span between them
      every frame, even though the `damage_buffer` protocol calls downstream
      are correctly per-rect. Making `pending_damage` a small bounded rect list
      (same 16-rect cap as `MAX_PROTOCOL_DAMAGE_RECTS`) lets the copy loop run
      per rect and shrinks steady-state SHM traffic to the actual changed
      pixels. Pairs with the H direct-paint item; whichever lands first should
      carry the rect-list change.
- [ ] **kde_blur region is re-created and re-committed on every present while
      blur is active.** `present_with_damage` unconditionally creates a fresh
      `Region`, calls `set_region` + `commit` per frame whenever
      `entry.blur_region` is `Some` (`backend.rs:1192-1215`) — the shell-side
      gate (`last_region_state` in `runtime/render.rs:900-930`) only gates
      `update_blur_region`, not the per-present protocol churn, because the
      backend re-commits from stored state each frame. A surface with an
      animated element and a static backdrop-blur pays wl_region create +
      2 protocol requests per frame for a region that never changes. Track the
      last-committed rect on `SurfaceEntry` and skip when unchanged (the
      `input_region_dirty` pattern two blocks below it is the right shape —
      blur is the one region type that didn't get it).
- [ ] **Input normalization allocates a String per event via a linear surface
      scan.** Every pointer/keyboard event calls `surface_id_for_wl_surface`,
      which iterates all surfaces comparing `wl_surface` handles and clones the
      id String (`wayland_surface/state.rs:314-322`, called from
      `handlers.rs:217` per pointer-frame event, `handlers.rs:438,461` for
      keyboard focus). Motion events then carry that String into
      `DevWindowEvent`, the shell re-allocates it again in `dispatch_wayland`
      (`event_surface_id(&event).to_string()`, `runtime/wayland.rs:24`), and
      key repeat clones surface-id + key name per synthesized event
      (`state.rs:19-29`). Coalescing caps what reaches the shell but every raw
      Wayland event still pays the scan + clone. Store the surface id as
      `Arc<str>` on `SurfaceEntry` (or a numeric id + side table) so the lookup
      is a pointer clone; `keysym_name`/`normalize_keysym_name` String
      allocation per key event (`handlers.rs:561-585`) and the lowercase alloc
      in `is_non_repeating_key` (`state.rs:336-348`) fold into the same pass.
- [ ] **Child popup targets force a full-buffer present every frame.**
      `paint_and_present_child_surface` sets `force_full_present = true`
      unconditionally after each child paint (`shell/runtime/render.rs:789-791`),
      so even if the popover subtree gained retained damage tracking (§P child
      item), presentation would still upload the full buffer. This is the
      presentation-side half of the §P "child popups bypass the retained
      pipeline" item — fix them together, otherwise the display-list work
      shows no SHM win.
- [ ] **`wait_for_surface_configure` runs up to 10 blocking roundtrips on the
      shell thread.** Called from `present_with_damage` (`backend.rs:1130`)
      and `surface_size` (`backend.rs:1324`) whenever the surface is not yet
      configured (`backend.rs:1405-1432`). Fine for first map, but a
      compositor that delays configure (or a dead popup) stalls the whole
      frame loop for 10 round trips; every other surface's present waits
      behind it. Bound it by deadline instead of roundtrip count, or return
      not-ready and let the render loop retry on the next Wayland wake (the
      loop already wakes on the connection fd).
- [ ] Minor per-present allocations: `attach_shm_buffer` builds two
      `Vec<DamageRect>` per present (`backend.rs:334-343`) and
      `protocol_damage_rects` re-allocates via `to_vec` even in the ≤16
      passthrough case (`backend.rs:569-582`) — smallvec/iterate-in-place;
      `surface_config_fingerprint` is a fourth hand-rolled byte-at-a-time FNV
      hasher (`backend.rs:142-161`), the presentation face of the §N
      hasher-drift item.

Structure:

- [ ] Healthy/confirmed: SHM pool reuse with per-buffer pending-damage
      refresh and the busy-buffer overflow slot (`backend.rs:265-316`);
      surface config fingerprint gating with the keyboard-only reconfigure
      carve-out (`backend.rs:198-227,454-469`); popup reconcile gated on
      `PopupConfig` equality shell-side (`runtime/render.rs:629-649`); opaque/
      input/blur region *updates* gated by display-list generation shell-side
      (`runtime/render.rs:900-930`); input region applied lazily with a dirty
      flag so it survives configure/remap ordering (`backend.rs:1220-1239`);
      frame-callback wait treated as a hint with a 50 ms escape hatch
      (`backend.rs:63,401-406,1132-1146`); `wait_for_events` blocks on Wayland
      fd + shell eventfd together with no spin (`backend.rs:1510-1602`);
      pointer/scroll coalescing at the engine boundary
      (`presentation/src/lib.rs:427-481`). The dev-window backend is dev-only
      and was not audited for hot-path cost.

### V. Shell orchestrator, threading & startup — 2026-07-04 deep dive

Focused trace of `Shell::run` (event loop, wake scheduling, message
coalescing), `render_components` orchestration, `dispatch_wayland`, reload
gating, and the discovery → catalog → mount startup path. The K threading
items (parallel paint, pipelining, blocking IO off-thread) and H startup item
(parallel module compile) still stand; findings below are additional.
`file:line` as of this scan.

Performance:

- [ ] **Every top-level surface gets a deep clone of the entire compiled
      frontend catalog at startup.** `FrontendCatalog` is a plain `Clone`
      struct holding every `CompiledFrontendModule` (parsed templates, styles,
      scripts for *all* frontend modules; `shell/component/catalog.rs:11-21`),
      and `load_frontend_components` passes `frontend_catalog.clone()` to
      each `FrontendSurfaceComponent` (`shell/discovery.rs:366-377`) after
      `top_level_surfaces()` has already cloned every matching entry once more
      (`catalog.rs:148-163`). With N surfaces the shell holds N+1 full copies
      of every compiled module for the life of the process — startup time and
      resident memory both scale as catalog × surfaces. Wrap the catalog in
      `Arc<FrontendCatalog>` (it is read-only after build; hot reload can
      rebuild-and-swap the Arc). Same call site also hands each component its
      own deep `interfaces.catalog()` clone (`discovery.rs:375`,
      `extension/service/src/interface.rs:86-91`) — the startup face of the
      §S resolve-clone item; fix both with the same `Arc` treatment.
- [ ] **`interfaces.resolve()` catalog deep-clones also fire from the command
      dispatch path.** `service_command_is_supported` and
      `service_command_is_coalescable` each call `self.interfaces.resolve()`
      (`runtime/request.rs:773-779,813-820`) — two full catalog clones per
      `ServiceCommand` request (every slider drag tick that passes the
      throttle, every button command), and `flush_throttled_commands` resolves
      again per flushed command. Already covered mechanically by the §S
      "resolve under the read lock" item; listed here so the orchestrator-path
      call sites get retired with it.
- [ ] **Startup is fully serial on the main thread.** Confirmed the H item:
      `discover_modules` scans + parses manifests dir-by-dir
      (`discovery.rs:124-136,209-304`), then `FrontendCatalog::from_modules`
      compiles every frontend module one at a time (`catalog.rs:45-69`),
      then backends spawn. Manifest load, `.mesh` parse, and compile are pure
      per-module — rayon over `module_ids` in `from_modules` is the smallest
      first cut. (Graph load is cached via
      `load_installed_module_graph_cached`; its `clone()` uses at
      `discovery.rs:365` and `backend/spawn.rs:18` are startup-only and fine.)
- [ ] **Per-event allocations in `dispatch_wayland`.** Each dispatched event
      allocates the physical surface id String (`runtime/wayland.rs:24`),
      clones the routed target id (`wayland.rs:53`), calls
      `surface_size_changed` per event even when the size cannot have changed
      (`wayland.rs:180`), and wraps every emitted request in its own
      single-element `VecDeque` (`wayland.rs:216-219`). Bounded by the
      32-events-per-frame cap and input coalescing, so this is allocation
      hygiene, not a hot bug — retire together with the §U `Arc<str>`
      surface-id change so ids stop being re-allocated at each layer.
- [ ] Minor idle-loop hygiene in `render_components`: the surface id String
      is cloned for every component before the `wants_render` gate
      (`runtime/render.rs:23`), and `component.id().to_string()` runs per
      rendering component per frame (`render.rs:66`); `reconcile_child_surface
      _requests` rebuilds `requested_keys`/`closing_keys` HashSets and
      re-clones entering-key sets per frame while any popover is open
      (`render.rs:432-499,524-527,669-673`). All small; fold into the v1.23
      interning pass.

Structure:

- [ ] **`legacy_backend_candidates_from_discovery` is a compat lane.** The
      graph-load failure fallback spawns backends from discovery-time module
      scanning (`backend/spawn.rs:48-59`, `backend/candidates.rs:300+`),
      duplicating the graph-driven candidate logic. Per the
      no-backward-compat rule: decide whether a missing/broken
      `config/module.json` should be a hard startup error (matching the
      manifest migration-diagnostics stance) and delete the legacy lane, or
      document why a degraded-mode boot is a product requirement. Currently it
      is a second candidate-selection implementation that can drift.
- [ ] Healthy/confirmed: the event loop is deadline-driven end to end —
      `next_runtime_sleep` computes exact deadlines from reload checks,
      command throttles, closing surfaces, popover hides, and component ticks
      (`runtime/mod.rs:76-151`) and blocks on Wayland fd + eventfd
      (`mod.rs:254-287`); all four reload checks park for 24 h when the
      inotify watcher is active and wake via `FilesystemChanged`
      (`reload.rs`, `theme.rs`, `mod.rs:391-397`); shell messages are drained
      with a 256 cap and coalesced with correct barrier semantics for
      lifecycle/interface-event ordering (`mod.rs:225-241,425-475`);
      `component_target_for_surface` rebuilds its index lazily on miss only
      (`mod.rs:46-66`); backend event bridges are per-provider Tokio tasks
      that wake the loop via eventfd writes (`backend/spawn.rs:100-241`);
      `flush_wayland` is TRACE-gated (`wayland.rs:225-244`). The remaining
      structural gap is the K phase-split (serial VM phase / parallel paint
      phase), unchanged by this pass.

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
