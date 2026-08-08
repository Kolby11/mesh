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
- [ ] Per-widget promotion — promote a widget *embedded in another surface* into
      its own window, rather than a whole surface. Depends on the shared surface
      VM (the widget does not own a VM today) and on multi-instance frontend
      modules. Whole-surface promotion shipped 2026-07-30. Design:
      [`.planning/todos/pending/2026-07-28-toplevel-window-surfaces.md`](../.planning/todos/pending/2026-07-28-toplevel-window-surfaces.md).

## Module system

The 2026-06-18 redesign largely shipped: canonical `module.json` with
`mesh.uses` / `mesh.provides` / `mesh.implements`, the graph as single source of
truth, typed graph diagnostics, library modules, and resource packs. Remaining:

- [ ] Move the remaining built-in debug and theme/locale service behavior
      behind generic providers. Startup sounds and backend profiling use the
      generic contract/runtime path; core-owned service state still branches.
      *(detail: "Module system — remaining open follow-ups")*
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
## Popovers

In-tree `<popover>` nodes are promoted to `xdg_popup` child surfaces, with core
owning the hover bridge, one-open-per-trigger exclusivity, and compositor
dismiss sync. *(detail: "Embeddable popovers via `<popover>` surface
promotion")*

## Performance

Full history, baselines, and the **rejected-experiments table** are in
[`.planning/log/performance-log.md`](../.planning/log/performance-log.md).
Check it before starting: several of the obvious approaches below have already
been measured and reverted.

Every optimization lands with a representative benchmark, and a checked relative
gate where the win is structural.

### Render pipeline

- [ ] **The narrow service path never engages for real modules.** It requires a
      template to interpolate the service field directly, but every shipped
      component reads services in Luau and binds derived variables — so
      `narrow_nodes` is empty, invalidation falls back to `TREE_REBUILD`, and
      every poll is a full rebuild plus 100%-of-surface damage. Measured on the
      navigation bar 2026-08-08: 240/240 frames full-surface, ~4 such frames per
      second at rest. Needs script-level service reads to feed
      `service_field_reads`, or a different narrowing signal.
- [ ] A root-level `backdrop-filter` collapses all partial damage to the whole
      surface (`expand_damage_for_blur_regions` unions with the full blurred
      region), and `.nav-shell` carries one. Latent today because damage is
      already full-surface for the reason above; it becomes the next ceiling as
      soon as that is fixed.
- [ ] Stop building the focused proof snapshot on production paints. It runs on
      every paint, allocates several `String`s per node (two `node.id`
      stringifications, an AccessKit id `format!`, cloned `role`/`aria-label`,
      and a `parley_text::…` format per text node), and is read only by tests.
      Worth ~19% of an Appearance paint-only frame and ~13% of a scroll frame;
      measured 2026-08-08.
- [ ] Continue widening generation shortcuts to per-node dirty scoping and
      unify changed-node fingerprints across the retained, render, and display
      layers; geometry-only retained snapshots are split out now.
- [ ] Display-list segment/rope command storage → v1.21. Command arrays are
      still flattened per ancestor. Replay must consume segments directly
      instead of eagerly re-flattening them — an eager reconstruction was tried
      and reverted (see log).
### Style

- [ ] Typed style declarations end-to-end: resolve theme tokens to typed values
      once per theme load; `apply_declaration` consumes typed values, strings
      only for diagnostics (E). Static literals now pre-lower; typed property
      values and one-time token lowering remain. *(detail: "P2 — typing &
      interning")*
- [ ] Interaction frames still re-apply string style declarations per node —
      folds into typed declarations and narrower invalidation.
      *(detail: "P2 — architecture")*
- [ ] A live animation defeats targeted restyle. Animation invalidation raises
      `VISUAL_REPAINT`, which carries no `STATE` bit — the exact bit that
      selects the targeted interaction-restyle branch — so every frame for a
      transition's duration restyles the whole tree instead of the animating
      nodes. Costs 2.1x on an otherwise identical navigation-bar paint frame.
- [ ] A tree-rebuild frame restyles memo-reused subtrees too. Component memo
      entries are stored pre-restyle (position-independent by design), so a
      reused page pays a full style walk and copy-on-write anyway — 2.8ms of an
      8.6ms Appearance service frame. Needs styled memo entries, or a
      "styles still valid" mark the restyle walk can skip.

### Typing and interning

- [ ] Per-frame `String` keys and SipHash on the animation path. The transition
      pass allocates a `String` per node per frame (`mesh_key().to_owned()`,
      then cloned again into `live_keys`) and keys `HashMap`/`HashSet` on it,
      while every other layer already keys on `NodeId`. Part of the ~27% of
      frame cycles the sampling profile puts in the allocator.
- [ ] Interned `Symbol` / `TagId` types and a typed `WidgetNode`. Attributes,
      module ids, and element tags are done; widget-tree **tags**, attribute
      **values**, and the broader symbol types remain. Profiling now puts the
      dominant remaining build cost in style resolution, not further attribute
      work. *(detail: "P2 — typing & interning")*
### Composition


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

- [ ] The `nix develop` banner tells developers to run the **debug** build
      (`cargo run -p mesh-tools-cli --bin mesh-shell -- start`), which is
      21–28x slower at idle than the optimized profile (5.55% vs 0.20% of a
      core) and reads as a shell performance problem. Point the banner at a
      release/profiling run, and consider a startup log line naming the build
      profile so a debug shell is self-identifying.
- [ ] **A scroll over a service-backed control spawns ~60 processes/second.**
      `onVolumeScroll` → `audio.set_volume()` costs two `wpctl` launches (the
      write, then an unconditional `refresh_state()` read-back) at up to 62.5
      commands/s under the 16ms `COMMAND_THROTTLE_INTERVAL`. A `wpctl` launch is
      15.5ms of CPU, nearly all dynamic linking, so continuous scrolling pegs a
      core (measured 93.4% in children vs 3.55% for the whole shell). Three
      separable fixes: throttle service *commands* by cost rather than by frame
      budget, drop the read-back while a monitor stream is already live, and
      coalesce repeated writes to the same field. Same shape for brightness
      scroll and popover slider drags.
- [ ] Push-based backend host API primitives (D-Bus signal subscribe, fd/socket
      watch, stream adoption) so providers are event-driven and polling is the
      fallback (C). **Measured 2026-08-08:** the shipped polls fork ~3 processes
      per second (`hyprctl` 500ms, `wpctl` 1000ms, `brightnessctl` 2000ms) and
      cost 6.4% of a core continuously — 32x the whole shell render loop at rest
      (0.2%) — mostly in dynamic-linker startup, for no state change. Includes evaluating `pw-dump --monitor` as a real volume
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

### Presentation
- [ ] Direct Skia paint into the mapped SHM canvas for full-present frames,
      keeping `PixelBuffer` as the retained compare copy (H). Design:
      [`.planning/todos/pending/2026-08-02-direct-shm-paint.md`](../.planning/todos/pending/2026-08-02-direct-shm-paint.md).
- [ ] Rotation transforms allocate a temp `PixelBuffer` and repaint the subtree
      per frame. Low priority until rotation ships; scratch-buffer reuse was
      measured and rejected (see log).

### Startup and catalog

- [ ] Narrow frontend catalog index rebuilds to graph deltas. Compiled sources
      now survive live graph changes by manifest/source fingerprint, but slot
      and validation indexes still rebuild across the catalog.

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
