# MESH — Active Backlog

The single list of what is open. Specifications describe contracts
([`spec/`](spec/)); guides describe current behavior; history and measurements
live in [`.planning/log/`](../.planning/log/).

**Items here say what to do and why it is not done — nothing else.** Progress
narratives, benchmark numbers, and completed items belong in the log. When an
item lands, delete it from this file and write its record in
[`.planning/log/`](../.planning/log/README.md).

Verify an older item against the source before starting it; later work
sometimes lands without updating a checkbox.

**Detail** for items carried over from before 2026-07-28 is in
[`.planning/log/backlog-archive-2026-07-28.md`](../.planning/log/backlog-archive-2026-07-28.md)
— a verbatim snapshot of this file with the full progress history. Items marked
*(detail: …)* name the section to search for there. Section letters (A–V) refer
to [`.planning/log/sections.md`](../.planning/log/sections.md); `→ vX.Y` markers
are from the retired milestone scheme and are kept only as rough sequencing.

---

## Shell features
- [ ] Popups / overlays — transient surfaces with custom content and dismiss
      behavior → v1.22.
- [ ] Per-widget promotion — promote a widget *embedded in another surface* into
      its own window, rather than a whole surface. Depends on the shared surface
      VM (the widget does not own a VM today) and on multi-instance frontend
      modules. Whole-surface promotion shipped 2026-07-30. Design:
      [`.planning/todos/pending/2026-07-28-toplevel-window-surfaces.md`](../.planning/todos/pending/2026-07-28-toplevel-window-surfaces.md).

## Module system

The 2026-06-18 redesign largely shipped: canonical `module.json` with
`mesh.uses` / `mesh.provides` / `mesh.implements`, the graph as single source of
truth, typed graph diagnostics, library modules, and resource packs. Remaining:

- [ ] Expose typed profile/package services to the settings frontend. Named
      composition, multiple root instances, scoped preferences, and
      transactional live switching are shipped; the replaceable UI still needs
      service contracts instead of privileged file access. See [`spec/01-module-system.md`](spec/01-module-system.md).
- [ ] Mount optional `settings_ui` entrypoints and add per-instance selection
      to generated prop controls. Default controls and profile-scoped global
      writes are shipped. See [`spec/08-settings.md`](spec/08-settings.md) §5.
- [ ] Eliminate the remaining service-specific Rust branches. The startup-sound
      path still calls the `mesh.audio` handler directly; debug and profiling
      paths also branch. *(detail: "Module system — remaining open follow-ups")*
- [ ] Show each module's uses/provides graph in settings and diagnostics —
      required and optional interfaces, active provider, native binaries,
      capabilities, i18n catalogs, keybinds, health. The debug inspector renders
      this; the full settings UI still needs per-module controls.
      *(detail: same section)*
- [ ] **Deferred — unify the four contribution schemas.** Theme, icons, i18n,
      and keybinds under one `contributes` shape, only where they share honest
      structure. Revisit after profiles land. Capability inference and a
      parallel inline-interface path were both rejected: they trade conceptual
      simplicity for typing simplicity, which is the failure mode that redesign
      set out to avoid.

## Settings

The single sparse store shipped 2026-07-30: one `config/settings.json`
namespaced by `shell` / module id / interface id, replacing `shell-settings.json`,
`settings-default.json`, and the per-module `config/settings.json` files.
Remaining:

- [ ] Expose the store as the `mesh.settings` service so modules read effective
      values and subscribe to changes, instead of the shell handing each
      component a raw JSON namespace. Prerequisite for a replaceable settings
      module. See [`spec/08-settings.md`](spec/08-settings.md) §1, §6.
- [ ] Extend `mesh-shell config eject` to props once `<props>` lands — it
      materializes only `surface` today, so a module's editable knobs still have
      no discoverable block to hand-edit.
- [ ] Validate stored `props.*` values once `<props>` lands. Settings validation
      covers the `shell` namespace and `surface` blocks; prop values pass through
      unchecked because there is no declaration to check them against yet
      (`MODULE_NAMESPACE_FIELDS` marks them `Opaque`).

## Popovers

In-tree `<popover>` nodes are promoted to `xdg_popup` child surfaces, with core
owning the hover bridge, one-open-per-trigger exclusivity, and compositor
dismiss sync. *(detail: "Embeddable popovers via `<popover>` surface
promotion")*

- [ ] Derive `Overflow` child surfaces automatically, beyond explicit
      `<popover>` — if inline UI escapes its parent buffer, the shell should
      derive the surface rather than requiring manifest geometry.
- [ ] Migrate the remaining production popovers off the legacy separate-module
      path, starting with audio once drag and capture state is represented in
      core.

## Tech debt

- [ ] Converge the immediate and retained renderers.
      `render/src/surface/painter/tree.rs` still holds a parallel
      widget-specific immediate renderer beside display-list replay. Route
      parity tests through one command builder, then delete the duplicate to
      stop semantic and clipping drift.
---

## Performance

Full history, baselines, and the **rejected-experiments table** are in
[`.planning/log/performance-log.md`](../.planning/log/performance-log.md).
Check it before starting: several of the obvious approaches below have already
been measured and reverted.

Every optimization lands with a representative benchmark, and a checked relative
gate where the win is structural.

### Render pipeline

- [ ] Widen generation shortcuts to per-node dirty scoping — scope the retained
      tree's own fingerprint traversal and unify changed-node fingerprints
      across the retained, render, and display layers.
- [ ] Display-list segment/rope command storage → v1.21. Command arrays are
      still flattened per ancestor. Replay must consume segments directly
      instead of eagerly re-flattening them — an eager reconstruction was tried
      and reverted (see log).
- [ ] Replace all-runtime descendant-generation scans in memo stores with a
      parent/child runtime index carrying an aggregated subtree generation.

### Style

- [ ] Typed style declarations end-to-end: resolve theme tokens to typed values
      once per theme load; `apply_declaration` consumes typed values, strings
      only for diagnostics (E). Static literals now pre-lower; typed property
      values and one-time token lowering remain. *(detail: "P2 — typing &
      interning")*
- [ ] Interaction frames still re-apply string style declarations per node —
      folds into typed declarations and narrower invalidation.
      *(detail: "P2 — architecture")*

### Typing and interning

- [ ] Interned `Symbol` / `TagId` types and a typed `WidgetNode`. Attributes,
      module ids, and element tags are done; widget-tree **tags**, attribute
      **values**, and the broader symbol types remain. Profiling now puts the
      dominant remaining build cost in style resolution, not further attribute
      work. *(detail: "P2 — typing & interning")*
### Composition

- [ ] `{#if}` / `{#for}` always wrap children in a synthetic `column` node —
      needs a fragment / transparent-container concept.
- [ ] No keyed list diffing; `{#for}` identity is positional. Add `key=`, paired
      with component memoization.

### Threading

- [ ] Parallelize paint across surfaces: phase-split `render_components` into a
      serial VM-bound phase and a parallel paint/SHM phase (rayon) (K).
- [ ] Pipeline paint of frame N against script work of frame N+1, after the
      per-surface split.
- [ ] Tile-parallel raster for large damage, above a measured threshold only.
- [ ] Move blocking file IO off the shell thread — i18n catalog mounts,
      settings and theme reloads, and icon/SVG cache-miss rasterization on the
      paint path — via `spawn_blocking` plus completion events.

### Runtime boundary

- [ ] Push-based backend host API primitives (D-Bus signal subscribe, fd/socket
      watch, stream adoption) so providers are event-driven and polling is the
      fallback (C). Includes evaluating `pw-dump --monitor` as a real volume
      event source; `pw-mon` emits no `changed:` block for volume.
- [ ] Handler sync still reads compound table globals, because nested in-place
      mutations never assign through `_ENV`. Eliminating those reads needs
      recursively tracked tables or Rust-owned reactive values (R).
      *(detail: "P1 — boundary & dispatch")*
- [ ] Storage reads still clone per Lua access. Needs shared immutable JSON
      values or lock avoidance — two cache designs were measured and reverted
      (I; see log).
### Input

- [ ] Slider drags with `change` / `release` handlers still take script
      invalidation; closing this needs narrow invalidation (J). Handlerless
      drags already use interaction restyle.

### Layout

- [ ] Make the retained Taffy mapping authoritative — incremental layout still
      rebuilds whole-tree `node_id_to_taffy` and `text_nodes` maps before
      compute. Complete fragment/unkeyed handling and maintain it during
      reconciliation.

### Presentation
- [ ] Batch Wayland commits and event-queue progress per shell frame — the
      per-surface path flushes the queue each time, repeating connection work
      and obstructing the planned parallel present split.
- [ ] Direct Skia paint into the mapped SHM canvas for full-present frames,
      keeping `PixelBuffer` as the retained compare copy (H).
- [ ] Rotation transforms allocate a temp `PixelBuffer` and repaint the subtree
      per frame. Low priority until rotation ships; scratch-buffer reuse was
      measured and rejected (see log).

### Startup and catalog

- [ ] Incrementally rebuild the frontend catalog on graph changes.
      `activate_frontend_module` recompiles every frontend for a one-module
      enable. Cache compiled modules by manifest/source fingerprint and update
      indexes from a graph diff.

### Architecture

- [ ] GPU rendering backend after retained layout, smart invalidation, and
      damage tracking ship → v1.25. Plan:
      [`gpu-rendering-backend`](../.planning/todos/pending/2026-07-15-gpu-rendering-backend.md).
      Skia-GL (Ganesh) first — same Canvas API as the shipped raster backend,
      and EGL buffer-age partial present preserves the damage pipeline.
---

## Attack order

Updated 2026-07-30.

1. **Structural-sharing memo hits**, narrow invalidation, and affected-subtree
   re-evaluation.
2. **Runtime style-diagnostic invalidation** and typed declarations.
3. **Incremental shared frontend catalog**, single retained renderer, and the
   per-surface prepare/paint/present split with batched Wayland commits.
4. **Direct SHM paint** and fractional-scale partial damage, re-tested with
   upload instrumentation (D).
