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
_(detail: …)_ name the section to search for there. Section letters (A–V) refer
to [`.planning/log/sections.md`](../.planning/log/sections.md); `→ vX.Y` markers
are from the retired milestone scheme and are kept only as rough sequencing.

---

## Section audit feature checklist

These are the feature-level items extracted from the section improvement audits.
The audit files retain the process maps, evidence, design detail, and regression
matrices; this checklist is the canonical place to track implementation.

### 1. Core foundation contracts

The six primary findings in this audit shipped on 2026-08-20 and are recorded in
the monthly log, so they are intentionally absent from the open backlog.
[Audit](../.planning/log/sections/01-core-foundation-contracts/improvements.md)

### 2. Module system and installation

[Audit](../.planning/log/sections/02-module-system-and-installation/improvements.md)


### 3. Service contracts

[Audit](../.planning/log/sections/03-service-contracts/improvements.md)


### 4. Themes

[Audit](../.planning/log/sections/04-themes/improvements.md)

### 5. Localization and i18n

[Audit](../.planning/log/sections/05-localization-i18n/improvements.md)

### 6. Host resources and icon packs

[Audit](../.planning/log/sections/06-host-resources-and-icon-packs/improvements.md)

### 7. Component language

[Audit](../.planning/log/sections/07-component-language/improvements.md)

### 8. UI element core

[Audit](../.planning/log/sections/08-ui-element-core/improvements.md)

### 9. Interaction and motion

[Audit](../.planning/log/sections/09-interaction-and-motion/improvements.md)

### 10. Frontend compiler and host

[Audit](../.planning/log/sections/10-frontend-compiler-and-host/improvements.md)

### 11. Luau runtime and sandbox

[Audit](../.planning/log/sections/11-luau-runtime-and-sandbox/improvements.md)

### 12. Rendering and paint

[Audit](../.planning/log/sections/12-rendering-and-paint/improvements.md)

### 13. Surface policy and configuration

[Audit](../.planning/log/sections/13-surface-policy-and-configuration/improvements.md)


### 14. Wayland platform and presentation

[Audit](../.planning/log/sections/14-wayland-platform-and-presentation/improvements.md)

### 15. Shell core and orchestration

[Audit](../.planning/log/sections/15-shell-core-and-orchestration/improvements.md)

### 16. Developer and authoring tools

[Audit](../.planning/log/sections/16-developer-and-authoring-tools/improvements.md)


## Foundation contracts

## Module system

The 2026-06-18 redesign largely shipped: canonical `module.json` with
`mesh.uses` / `mesh.provides` / `mesh.implements`, the graph as single source of
truth, typed graph diagnostics, library modules, and resource packs. Remaining:

## Service contracts

## Themes

## Host resources and icon packs

## Component language

## UI element core

## Interaction and motion

- [ ] Give the `transition-*` longhands their own comma lists. The animator now
      runs one instance per entry, but `transition-duration`, `-delay`, and
      `-timing-function` still write a single value into entry 0 through
      `first_transition_mut`, so longhand authors cannot give two properties
      different timing the way the shorthand can.
- [ ] Implement or safely gate the public `box-shadow` parser and add the
      Section 9 interaction/render/animation regression matrix.

## Frontend compiler and host

- [ ] Compile, validate, watch, invalidate, and reload primary and
      extension-point roots as one atomic frontend catalog revision, including
      contribution interface checks and contribution-only dependencies. [Audit](../.planning/log/sections/10-frontend-compiler-and-host/improvements.md).
- [ ] Gate service payload publication on capabilities and replace the
      revision-light host effects with a coherent, typed frontend frame/effect
      boundary that rejects stale catalog/runtime requests.
- [ ] Complete frontend runtime lifecycle and recovery: dispatch mount/unmount,
      make prop publication transactional, and preserve truthful typed,
      source-located diagnostics on failures.
- [ ] Unify template expression semantics and root-scope validation around the
      real Luau parser/runtime, then enforce imported public-prop and
      child-surface contracts.
- [ ] Split the renderer/Wayland/package/debug policy out of the
      compiler-facing frontend host ABI.

## Shell core and orchestration

- [ ] Replace split profile/runtime mutation with one revisioned activation
      coordinator: immutable candidate graph/interfaces/resources, full root and
      provider identities, ready hidden replacements, atomic commit, and
      post-commit retirement. [Audit](../.planning/log/sections/15-shell-core-and-orchestration/improvements.md).
- [ ] Reconcile every live graph delta, including backend module enable/disable;
      buffer provider readiness state and generation-tag all runtime messages,
      events, results, and restart deadlines.
- [ ] Add explicit frontend unmount and graceful backend stop/join, then route
      normal shutdown and every shell-loop error through one bounded lifecycle
      supervisor that owns workers, IPC, eventfd, storage, and surfaces.
- [ ] Replace the static startup watcher with a healthy, generation-aware watch
      set covering graph/profile/catalog/contribution/resource/import changes,
      with immediate bounded-poll fallback and last-known-good reloads.
- [ ] Centralize CoreRequest effects in one fair bounded scheduler and isolate
      component callback/tick/build failures into errored placeholders instead
      of allowing cycles or one module to terminate the shell.
- [ ] Make shell control-plane propagation coherent: publish provider
      unavailable/recovery transitions, settings revisions, theme/locale effects,
      and invalid graph/profile diagnostics through the committed generation.

## Developer and authoring tools

- [ ] Move CLI install/update/rollback/uninstall/profile mutations behind one
      journaled, path-contained package transaction with typed live-activation
      acknowledgements and exact-generation recovery. [Section 16 audit](../.planning/log/sections/16-developer-and-authoring-tools/improvements.md).
- [ ] Derive and refresh one canonical graph-authoring snapshot for CLI, doctor,
      and LSP; eliminate duplicated manifest/schema validation and stale or
      silently lossy module/service indexes. [Section 16 audit](../.planning/log/sections/16-developer-and-authoring-tools/improvements.md).
- [ ] Make LSP parsing and protocol boundaries correct and syntax-aware:
      UTF-16 positions, workspace folders, Unicode JSON, versioned updates,
      secure definitions, and recoverable Luau/component AST diagnostics.
      [Section 16 audit](../.planning/log/sections/16-developer-and-authoring-tools/improvements.md).

## Settings

The single sparse store shipped 2026-07-30: one `config/settings.json`
namespaced by `shell` / module id / interface id, replacing `shell-settings.json`,
`settings-default.json`, and the per-module `config/settings.json` files.

## Popovers

In-tree `<popover>` nodes are promoted to `xdg_popup` child surfaces, with core
owning the hover bridge, one-open-per-trigger exclusivity, and compositor
dismiss sync. _(detail: "Embeddable popovers via `<popover>` surface
promotion")_

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
- [ ] Continue widening generation shortcuts to per-node dirty scoping and
      unify changed-node fingerprints across the retained, render, and display
      layers; geometry-only retained snapshots are split out now.
- [ ] Display-list segment/rope command storage → v1.21. Command arrays are
      still flattened per ancestor. Replay must consume segments directly
      instead of eagerly re-flattening them — an eager reconstruction was tried
      and reverted (see log).
- [ ] Close the Section 12 correctness gaps before further paint optimization:
      unify paint fingerprints and topology generations, implement complete
      transforms/effect compositing/border lowering, and revision text/icon
      caches. [Section 12 audit](../.planning/log/sections/12-rendering-and-paint/improvements.md)

### Style

- [ ] Typed style declarations end-to-end: resolve theme tokens to typed values
      once per theme load; `apply_declaration` consumes typed values, strings
      only for diagnostics (E). Static literals now pre-lower; typed property
      values and one-time token lowering remain. _(detail: "P2 — typing &
      interning")_
- [ ] Interaction frames still re-apply string style declarations per node —
      folds into typed declarations and narrower invalidation.
      _(detail: "P2 — architecture")_
- [ ] A tree-rebuild frame restyles memo-reused subtrees too. Component memo
      entries are stored pre-restyle (position-independent by design), so a
      reused page pays a full style walk and copy-on-write anyway — 2.8ms of an
      8.6ms Appearance service frame. Needs styled memo entries, or a
      "styles still valid" mark the restyle walk can skip.

### Typing and interning

- [ ] Interned `Symbol` / `TagId` types and a typed `WidgetNode`. Attributes,
      module ids, and element tags are done; widget-tree **tags**, attribute
      **values**, and the broader symbol types remain. Profiling now puts the
      dominant remaining build cost in style resolution, not further attribute
      work. _(detail: "P2 — typing & interning")_

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

- [ ] Apply one authoritative sandbox/resource policy to every Luau realm:
      enforce instruction, memory, output, queue, storage, and child-process
      budgets, with timeout cleanup and quarantine. [Section 11 audit](../.planning/log/sections/11-luau-runtime-and-sandbox/improvements.md).
- [ ] Replace backend task aborts and early-return cleanup with an idempotent
      lifecycle supervisor that stages provider generations, flushes storage,
      reaps streams, and publishes one truthful terminal result. [Section 11 audit](../.planning/log/sections/11-luau-runtime-and-sandbox/improvements.md).
- [ ] Move `mesh.exec` and stream handling behind bounded cancellable workers
      with stable stream IDs, exit/reap events, output limits, and executable
      path/argument policy. [Section 11 audit](../.planning/log/sections/11-luau-runtime-and-sandbox/improvements.md).
- [ ] Make backend commands and events typed, correlated, generation-aware,
      transactional, and bounded; define explicit coalescing keys and terminal
      overflow/timeout results. [Section 11 audit](../.planning/log/sections/11-luau-runtime-and-sandbox/improvements.md).
- [ ] Move default runtime storage to secure durable XDG state with user-only
      permissions, quotas, recovery, and single-writer/revision semantics.
      [Section 11 audit](../.planning/log/sections/11-luau-runtime-and-sandbox/improvements.md).
- [ ] Make reload transactional and backend callback handles generation-stable:
      replace stale Lua environments, preserve one backend `self`, and prevent
      pre-ready or old-generation updates from reaching consumers. [Section 11 audit](../.planning/log/sections/11-luau-runtime-and-sandbox/improvements.md).

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
      _(detail: "P1 — boundary & dispatch")_
- [ ] Storage reads still clone per Lua access. Needs shared immutable JSON
      values or lock avoidance — two cache designs were measured and reverted
      (I; see log).

### Surface policy and configuration

- [ ] Validate manifest surface enums and role-specific fields before
      resolution; invalid declarations must produce diagnostics instead of
      silently falling back. [Section 13 audit](../.planning/log/sections/13-surface-policy-and-configuration/improvements.md).
- [ ] Protect the author-only `promotable` contract and route settings-driven
      role changes through the same transactional transition path as explicit
      promotion requests. [Section 13 audit](../.planning/log/sections/13-surface-policy-and-configuration/improvements.md).
- [ ] Unify role-field metadata, settings/ejection semantics, and manifest
      diagnostics so inert window/layer fields are handled consistently.
      [Section 13 audit](../.planning/log/sections/13-surface-policy-and-configuration/improvements.md).
- [ ] Replace split surface-config field lists with a revisioned semantic
      policy diff that includes blur, decorations, padding, geometry, keyboard
      mode, and role transitions. [Section 13 audit](../.planning/log/sections/13-surface-policy-and-configuration/improvements.md).
- [ ] Make unmeasured/content/padded/physical surface extents typed at the
      shell/presentation seam, with checked geometry and transactional reload
      regressions. [Section 13 audit](../.planning/log/sections/13-surface-policy-and-configuration/improvements.md).

### Rendering and paint

- [ ] Establish one canonical render-frame snapshot and transform/clip model;
      unify invalidation, display-list reuse, damage, blur regions, and hit
      testing around cumulative affine transforms. [Section 12 audit](../.planning/log/sections/12-rendering-and-paint/improvements.md).
- [ ] Complete paint semantics for opacity layers, four-edge borders,
      four-corner radii, text physical scaling, and stable equal-z ordering;
      add retained and pixel regressions. [Section 12 audit](../.planning/log/sections/12-rendering-and-paint/improvements.md).
- [ ] Replace path-only font/glyph/text caches and synchronous icon/font decode
      with generation-aware bounded resources and an asynchronous paint-safe
      resource broker. [Section 12 audit](../.planning/log/sections/12-rendering-and-paint/improvements.md).
- [ ] Make partial-present capability, layer balance, diagnostics, and
      backend fidelity explicit contracts; derive compositor blur and uploaded
      damage from the validated frame spans. [Section 12 audit](../.planning/log/sections/12-rendering-and-paint/improvements.md).

### Presentation

- [ ] Make presentation lifecycle transactional and observable: typed
      create/configure/present/lost results, last-known-good role replacement,
      and one idempotent close/dismiss/destroy teardown path. [Section 14 audit](../.planning/log/sections/14-wayland-platform-and-presentation/improvements.md).
- [ ] Replace presentation fingerprints and warm region caches with typed diffs
      and object/configure/frame generations so state-only changes and recreated
      objects commit compositor state. [Section 14 audit](../.planning/log/sections/14-wayland-platform-and-presentation/improvements.md).
- [ ] Make popup promotion capability- and identity-safe: carry click seat/serial,
      gate reposition by xdg-shell version, validate role/parent/reparenting, and
      correlate reactive configures. [Section 14 audit](../.planning/log/sections/14-wayland-platform-and-presentation/improvements.md).
- [ ] Unify resolved logical/physical surface extents and make SHM buffer-release
      backpressure, callback generations, output membership, and input ownership
      explicit without hot retries or stale routing. [Section 14 audit](../.planning/log/sections/14-wayland-platform-and-presentation/improvements.md).
- [ ] Replace the recording-only presentation test backend with a deterministic
      lifecycle simulator and a small live compositor conformance matrix covering
      close, popup, scaling, occlusion, and multi-output behavior. [Section 14 audit](../.planning/log/sections/14-wayland-platform-and-presentation/improvements.md).
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
