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

## 2026-08-31 read-only audit findings — Section 1

The following Section 1 findings remain open; IDs preserve their audit evidence
for a future report or rerun.

- [ ] **S01-DEAD-007 (P3):** Remove unused direct `tracing`,
      `tracing-subscriber`, and `thiserror` dependencies from
      `mesh-core-diagnostics`, verified by focused check/test and dependency tree.
- [ ] **S01-DEAD-008 (P3):** Remove unused diagnostics `ModuleMetrics` or
      explicitly migrate its intended role into the current debug profiling DTOs.
- [ ] **S01-DEAD-009 (P3):** Remove or formalize the test-only
      `LifecycleErrorRecord` compatibility projection so the canonical diagnostics
      snapshot is the sole lifecycle representation.
- [ ] **S01-DEAD-010 (P3):** Either connect `Lifecycle`, `Configuration`, and
      `Resource` diagnostic categories to their real producers or document them as
      reserved wire values before considering removal.
- [ ] **S01-DEAD-012 (P3):** Remove legacy `DebugTab`, `active_tab`, and
      `from_legacy_tab` state after downstream compatibility review; the shell uses
      `DebugInspectorView`.
- [ ] **S01-DEAD-013 (P3):** Review downstream use, then prune unused public
      capability introspection accessors that have no in-repository production
      consumers.
- [ ] **S01-DEAD-015 (P3 candidate):** Confirm whether `FieldKind::Int32` is a
      supported extension point or unused reserved vocabulary before retaining or
      removing it.
- [ ] **S01-LOGIC-007 (P3):** Give lifecycle diagnostics an explicit module or
      generation identity instead of attaching them to the first registered
      instance; test multi-instance record and resolution behavior.

## 2026-09-01 whole-codebase audit — new open tasks

New findings from the 2026-09-01 audit synthesis, sorted by audit section and
grouped where one implementation should resolve several findings. Detailed
evidence and test workloads are in the linked reports.

### Section 01 — Core foundation contracts

- [ ] **S01-LOGIC-003 / S01-DEAD-003:** Route debug-inspector access through
      the closed capability catalog and make graph/surface schema ownership
      canonical, with parity fixtures for activation and validation. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/01-core-foundation-contracts.md)
- [ ] **S01-LOGIC-004 / S01-LOGIC-005:** Reject invalid array members without
      compacting sparse settings, and resolve tooltip state from effective
      settings rather than durable storage. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/01-core-foundation-contracts.md)

### Section 02 — Module system and installation

- [ ] **S02-LOGIC-001 / S02-LOGIC-002:** Preserve omitted versus explicit
      profile overlays so inactive roots stay inactive and explicit empty
      icon/font/language chains clear inherited values. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/02-module-system-and-installation.md)
- [ ] **S02-LOGIC-003 / S02-LOGIC-008 / S02-LOGIC-011:** Bind graph diffs and
      activation candidates to the same store, manifest/content revision, and
      lock identity so same-version edits or mismatched objects cannot publish. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/02-module-system-and-installation.md)
- [ ] **S02-LOGIC-004 / S02-LOGIC-005 / S02-LOGIC-006 / S02-LOGIC-007:** Make
      forced slot removal, Git/profile rollback, lock paths, and active-profile
      pointers fail closed and recover against the intended generation. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/02-module-system-and-installation.md)
- [ ] **S02-LOGIC-009 / S02-LOGIC-010 / S02-LOGIC-012:** Resolve the required-
      provider rule conflict and isolate invalid or unreadable modules with
      durable diagnostics instead of aborting or silently shrinking discovery. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/02-module-system-and-installation.md)
- [ ] **S02-LOGIC-013:** Add generation-aware package garbage collection that
      retains active, rollback, and in-flight journal objects while reclaiming
      only unreferenced immutable content. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/02-module-system-and-installation.md)
- [ ] **S02-DEAD-002:** Centralize install capability/trust review in the module
      core and make shell and CLI consume the same typed result. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/02-module-system-and-installation.md)
- [ ] **S02-PERF-001 / S02-PERF-002 / S02-PERF-004 / S02-PERF-005:** Move
      blocking package/Git preparation off the shell request path, avoid broad
      no-op backups, share parsed manifests/passes, and enforce measured source
      size budgets. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/02-module-system-and-installation.md)
- [ ] **S02-PERF-003:** Replace whole-catalog authoring refresh hashing with
      watcher/content-index revisions and retain full hashing as a recovery
      fallback. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/02-module-system-and-installation.md)

### Section 03 — Service contracts

- [ ] **S03-PERF-001 / S03-PERF-002 / S03-PERF-003 / S03-PERF-004:** Publish immutable service catalogs once per graph
      generation, reuse compiled dispatch/schema data and per-turn service
      views, and bound pending calls/events; add the specified fan-out/load
      measurements before changing representations. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/03-service-contracts.md)
- [ ] **S03-DEAD-001 / S03-DEAD-003:** Make compiled contracts the canonical
      source for runtime, Luau, and documentation projections, retaining raw
      declarations only behind explicit compatibility/tooling adapters. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/03-service-contracts.md)

### Section 04 — Themes

- [ ] **S04-PERF-002 / S04-PERF-003 / S04-DEAD-003:** Share one bounded CSS,
      token, and keyframe representation across theme and component paths, then
      measure typed token dependency resolution and reload reuse. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/04-themes.md)

### Section 05 — Localization and i18n

- [ ] **S05-PERF-001 / S05-PERF-002 / S05-PERF-003 / S05-DEAD-002:** Make bulk and point translation
      lookup share one effective-catalog precedence traversal, and measure
      catalog parsing, projection, and translator identifier allocation at
      realistic sizes. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/05-localization-i18n.md)

### Section 06 — Host resources and icon packs

- [ ] **S06-PERF-001 / S06-PERF-002 / S06-PERF-003:** Measure and narrow icon/font resolution and resource
      invalidation by pack, alias, revision, and requesting owner while keeping
      deterministic fallback order. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/06-host-resources-and-icon-packs.md)

### Section 07 — Component language

- [ ] **S07-PERF-001 / S07-PERF-002 / S07-DEAD-002:** Share lexical/component
      AST work and CSS value/selector lowering between runtime and tooling,
      measuring incremental parse and dependency/style validation workloads. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/07-component-language.md)

### Section 08 — UI element core

- [ ] **S08-PERF-001 / S08-PERF-002 / S08-PERF-003:** Measure and narrow retained layout/style work,
      text measurement caching, and semantic/layout snapshot traversal for
      localized changes while preserving stale-geometry safety. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/08-ui-element-core.md)

### Section 09 — Interaction and motion

- [ ] **S09-PERF-001 / S09-PERF-002 / S09-PERF-003:** Measure repeated hit-test/dispatch traversal,
      compiled animation timelines, and reduced-motion/visibility invalidation;
      share dirty revisions with rendering. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/09-interaction-and-motion.md)

### Section 10 — Frontend compiler and host

- [ ] **S10-PERF-001 / S10-PERF-002 / S10-PERF-003:** Cache tree/style preparation and recursive imports by
      content revision, and narrow effect/state observation summaries after
      measuring rebuild and service-update workloads. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/10-frontend-compiler-and-host.md)

### Section 11 — Luau runtime and sandbox

- [ ] **S11-PERF-001 / S11-PERF-002 / S11-PERF-003:** Measure shared-realm contention, host-boundary JSON
      conversion, and stream lock/overflow behavior under bounded workloads
      before changing runtime sharing or conversion paths. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/11-luau-runtime-and-sandbox.md)
- [ ] **S11-DEAD-001:** Review repository callers and make the authorized stream
      launch function the only production entry point, retaining any wrapper
      only as an explicit test adapter. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/11-luau-runtime-and-sandbox.md)

### Section 12 — Rendering and paint

- [ ] **S12-PERF-002 / S12-DEAD-001:** Measure sparse retained display-list
      invalidation and consolidate proof, profiling, and production frame
      evidence into one bounded metrics model. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/12-rendering-and-paint.md)

### Section 13 — Surface policy and configuration

- [ ] **S13-PERF-001 / S13-PERF-002:** Compile effective surface policy by
      revision and expose field-group diffs so geometry-only changes do not
      trigger unrelated downstream work; measure merge, commit, and damage cost. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/13-surface-policy-and-configuration.md)

### Section 14 — Wayland platform and presentation

- [ ] **S14-PERF-002 / S14-PERF-003:** Separate region/geometry-only protocol
      work from paint and measure bounded input conversion/queue behavior while
      preserving ordering, damage, and acknowledgement semantics. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/14-wayland-platform-and-presentation.md)

### Section 15 — Shell core and orchestration

- [ ] **S15-LOGIC-001:** Close the post-commit control-plane failure seam so
      settings, theme, locale, graph, pointer, runtime, and diagnostics either
      commit as one generation or expose a typed degraded/recovery state. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/15-shell-core-and-orchestration.md)
- [ ] **X-LOGIC-02 / X-LOGIC-03:** Bind candidate module identities and
      `ActiveSnapshot` to committed activation/control-plane revisions, and
      refresh or replace retained roots when policy or catalog state changes. [Cross-section audit](../.planning/codebase/audits/2026-09-01-whole-codebase/cross-section-findings.md)
- [ ] **X-LOGIC-04:** Retain the newest filesystem graph revision while
      activation is pending and retry reconciliation after candidate completion
      or abort. [Cross-section audit](../.planning/codebase/audits/2026-09-01-whole-codebase/cross-section-findings.md)
- [ ] **S15-PERF-001 / S15-PERF-002:** Benchmark shell-loop fairness and
      profile/catalog preparation across module, surface, provider, and message
      loads before changing scheduling or sharing. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/15-shell-core-and-orchestration.md)

## Shell core and orchestration

- [ ] Replace split profile/runtime mutation with one revisioned activation
      coordinator: immutable candidate graph/interfaces/resources, full root and
      provider identities, ready hidden replacements, atomic commit, and
      post-commit retirement. [Audit](../.planning/log/sections/15-shell-core-and-orchestration/improvements.md).

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
