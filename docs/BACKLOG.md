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

Section letters (A–V) refer to
[`.planning/log/sections.md`](../.planning/log/sections.md); `→ vX.Y` markers
are from the retired milestone scheme and are kept only as rough sequencing.

---

## Change-proportional frame pipeline — next implementation priority

Frame cost scales with tree size times about ten passes, not with what
changed; scoping single passes buys only 3–10% each. Do these in order before
other performance work. Evidence, the Qt Quick / GTK4 comparison, exit
criteria and the items folded in here:
[design](../.planning/todos/pending/2026-10-21-change-proportional-pipeline.md).

- [ ] **P0/live:** The live glass dock repaints continuously with no input (296–439 frames per
      8 s, 17–21 ms each, ~1,600–1,877 retained-discovery visits); find what keeps it rendering.
      `tools/check-performance --live` reproduces it.
- [ ] **P0/live:** Live dock hover frames walk the whole tree (about 40 ms vs 0.85 ms p90 in the CPU
      fixture); the scoped interaction path is not reached live. The `--live` structural assertion
      fails until this is fixed.
- [ ] Live `total_surface_render` reads ~100 ms on every surface during dashboard spectrum
      updates; check cross-surface attribution before optimizing the dashboard from live numbers.
- [ ] The live replay reports CPU raster even in a `gpu` build; use the render-path diagnostic to
      find why. Its first `mesh.apps` injection before the dock's first paint is lost.
- [ ] Give the live replay its own layer-namespace suffix so it can run beside a desktop shell.
- [ ] **P2:** Complete the shared dirty-set pipeline beyond the animation, interaction and
      paint-only patch scopes. Patch frames that change layout (dashboard `style:height`) still
      run annotation, layout discovery, retained discovery, validation and animation sampling over
      the whole tree; display-list ancestors, previous-style capture and child surfaces also walk it.
- [ ] **P3:** Bindings compile to typed setters (finishes D1; with D4 typed patches).
      Fixed-shape native keyed lists already patch items; key validation and binding-target
      discovery still scan. Free-form `style:*` bindings are classed STYLE|A11Y, so only a style
      diff can tell whether they reach layout; typed setters should classify them.
- [ ] **P4:** Narrow GPU scene preparation by retained revision (each frame records the full
      selection; design inputs are in the pipeline note), after W3. Presentation no longer waits
      for the worker, but a surface is gated while its frame is in flight.
- [ ] Shell tests build without the `gpu` feature, so nothing in the shell suite exercises
      `PresentStatus::Submitted` or `reconcile_scene_completions`; add a GPU-feature shell gate.
- [ ] A scene frame that completes unpresented because the compositor holds every dmabuf slot
      is repainted in full; retry on buffer release, keeping damage, instead.

## 2026-09-30 block review and module-architecture direction

Ordered by implementation priority: finish a tier before starting the next
unless an item says otherwise. IDs point into the
[block review](../.planning/codebase/audits/2026-09-30-block-review/REPORT.md)
(evidence and tests); `A2`–`A8` point into the
[module architecture direction](../.planning/todos/pending/2026-09-30-module-architecture-direction.md).

### Priority 7 — Customization and ecosystem

- [ ] **A6 / RES-05:** Ship bar widgets as separate `mesh.navigation.item`
      modules with per-instance props; share volume presentation in a library.
- [ ] **A2:** Optional named features inside contracts with `supports()` checks.
- [ ] **A8:** Move UPower/NetworkManager state reads from `exec.argv` to scoped
      `dbus.call` grants; scope `dbus.*` subscriptions per sender and `fs.*` per path;
      ext-workspace protocol data; plain-language permissions.
- [ ] **A4:** `module fork` / `diff` / `rebase` with recorded upstream.

### Priority 8 — Structural mechanics

- [ ] **SH-08:** Extract `shell/component/**` into a `mesh-core-frontend-runtime`
      crate.
- [ ] **UI-09:** Per-element behaviour table in `mesh-core-interaction` (after
      SH-08), starting with input caret keys/selection and slider Home/End/Page.
- [ ] **RT-06:** Per-context budget counters and heap attribution on the shared
      frontend VM.
- [ ] **RT-04:** Stop blocking tokio workers in `mesh.exec`; later an async exec
      host API.

## Glass shell follow-ups (found 2026-10-20)

- [ ] **Paint-time effects cannot reach the shell:** `mesh.shell.*` and published events
      from effects that settle during paint are lowered only by the next handler call.
- [ ] **Sibling keyed loops collide:** two `{#for}` blocks under one parent with overlapping key
      values get the same `@loop:` NodeId; the retained update asserts and panics. Add a per-loop discriminator.
- [ ] **`contributes.keybinds` reads as undeclared:** `config doctor` reports subscribed
      actions of the glass overlays as undeclared; check that runtime Escape bindings resolve.
- [ ] **`corner-shape: scoop` gaps:** borders are not stroked along a scoop's curve, and
      `overflow: hidden` clips descendants to the rounded shape, not the scooped one ([04 §12.1](spec/04-styling.md#121-corner-shapes-corner-shape)).
- [ ] **Retire the tooltip overlay reserve:** tooltips that leave their surface are now
      `xdg_popup`s, so the 352×200 reserve every layer surface paints could go; measure buffer/damage savings first.
- [ ] **Component styles leak across a surface:** every component's `<style>` joins one
      source-ordered cascade, so a parent's or sibling's `.pill` overrode `status.mesh`'s
      `.pill.battery`. Decide per-component scoping (and migrate modules) or document the global model.
- [ ] **`border-radius` shorthand with `var()`:** `var(--radius-lg) 0 0 var(--radius-lg)` left the
      settings sidebar square live, while the longhands work; find where the mixed shorthand drops.
- [ ] **Settings theme switch may not repaint:** reported 2026-10-21 as needing a tab change; not
      reproduced on `1d50170a` (layer and window roles, file and click paths). Capture a repro.

## Glass shell core gaps (found 2026-10-17)

- [ ] **Full-colour icons:** XDG/hicolor resolutions and `<icon src>` are always
      `multicolor: false`; `<image>` has no paint arm and `url()` rejects absolute paths, so docks cannot show app icons in colour.
- [ ] **Host APIs:** `net.http` and `exec.launch-app` are catalogued but unimplemented
      (providers shell out to `curl` / `hyprctl dispatch`). A StatusNotifier tray still needs
      a watcher/host provider; `mesh.dbus_serve` now owns names and exports one object each.
- [ ] **Icon chains during live switch:** newly mounted modules briefly resolve with an empty pack
      chain (`effective_chain=[]`), logging missing-icon warnings until the chain is rebuilt.
- [ ] **Backdrop placement beyond MESH:** refraction predicts surface origins from MESH's own
      surfaces; another client's exclusive zone (e.g. a quickshell bar) misaligns it. Multi-output backdrops are untested.
      A foreign wallpaper layer above MESH's (`quickshell:background`) makes glass refract a different
      image than the one on screen.

## Authoring model

Goal: components written as plain Luau, with the framework owning dependency
tracking, validation and scheduling.
[Review and evidence](../.planning/todos/pending/2026-10-10-authoring-model.md).

- [ ] The debug-inspector views still compute their empty-state and section
      `hidden` flags in script, and bubble-options builds rotation class names
      in functions; move them to `{#if}` and per-item data when next touched.
- [ ] Narrow compound public inputs and captured template-helper dependencies;
      tables and function calls still use conservative invalidation.
- [ ] Optimistic pending values on proxy methods, reconciled with emitted state
      through contract coalescing keys, replacing the hand-written
      set-then-reconcile pattern in sliders.
- [ ] A top-level `local x = require("mesh.*")` becomes a static interface
      import bound for the whole surface, so a service update invalidates every
      memoized sibling component; settings keeps `pcall(require)` on
      `mesh.audio` and `mesh.debug` to avoid it. Bind imports per component.
- [ ] Plain `.luau` libraries shareable between modules (`require` of a library
      export); helpers such as volume-icon thresholds exist in three copies.
- [ ] `self` at script top level, plus event forwarding in markup
      (`onpointerenter={self.PopoverEnter}`), replacing `init` captures and
      one-line forwarding handlers.
- [ ] Locale matching helper (`mesh.locale.is("sk")`) alongside RES-03's
      formatters.

## Runtime architecture direction

Larger changes to the runtime model under scripts; the platform model stays.
Each needs a design and a measured prototype before it is adopted.
[Direction note](../.planning/todos/pending/2026-10-10-runtime-architecture-direction.md)
(suggested order inside).

- [ ] **D4:** Providers send typed field patches instead of whole snapshots, and
      no JSON on the frame path. Validation, ranges/defaults and `.d.luau`
      generation have shipped, and LSP proxy types use the same mapping.


## 2026-09-01 whole-codebase audit — new open tasks

New findings from the 2026-09-01 audit synthesis, sorted by audit section and
grouped where one implementation should resolve several findings. Detailed
evidence and test workloads are in the linked reports.

### Section 02 — Module system and installation

- [ ] **S02-LOGIC-003 / S02-LOGIC-008 / S02-LOGIC-011:** Bind graph diffs and
      activation candidates to the same store, manifest/content revision, and
      lock identity so same-version edits or mismatched objects cannot publish. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/02-module-system-and-installation.md)
- [ ] **S02-LOGIC-013:** Add generation-aware package garbage collection that
      retains active, rollback, and in-flight journal objects while reclaiming
      only unreferenced immutable content. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/02-module-system-and-installation.md)
- [ ] **S02-PERF-001 / S02-PERF-002 / S02-PERF-004:** Move blocking
      package/Git preparation off the shell request path, avoid broad no-op
      backups, and share parsed manifests/passes. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/02-module-system-and-installation.md)
- [ ] **S02-PERF-003:** Replace whole-catalog authoring refresh hashing with
      watcher/content-index revisions and retain full hashing as a recovery
      fallback. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/02-module-system-and-installation.md)

### Section 03 — Service contracts

- [ ] **S03-PERF-001 / S03-PERF-002 / S03-PERF-003 / S03-PERF-004:** Publish immutable service catalogs once per graph
      generation, reuse compiled dispatch/schema data and per-turn service
      views, and bound pending calls/events; add the specified fan-out/load
      measurements before changing representations. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/03-service-contracts.md)
- [ ] **S03-DEAD-001 / S03-DEAD-003:** Make compiled contracts the canonical
      source for the Luau proxy, operation authorization and LSP; shell
      validation already uses them. Keep raw declarations only behind explicit
      compatibility/tooling adapters. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/03-service-contracts.md)

### Section 04 — Themes

- [ ] **S04-PERF-002 / S04-PERF-003 / S04-DEAD-003:** Share one bounded CSS,
      token, and keyframe representation across theme and component paths, then
      measure typed token dependency resolution and reload reuse. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/04-themes.md)
- [ ] **Theme coverage and pinning in the UI:** `mesh-shell theme coverage` and
      `set_theme { scope }` exist, but the theme selector shows neither coverage
      nor a "pin to this shell" control.
- [ ] **`mesh.theme` snapshot exceeds the 64 KiB command budget:** full
      tokens/provenance plus every host font family are published together and
      warn at startup; split or page the host font list.
- [ ] **LSP completion for module and family tokens:** diagnostics accept them,
      but `var(--` completion offers only contract roles and local properties.

### Section 05 — Localization and i18n

- [ ] **S05-PERF-001 / S05-PERF-002 / S05-PERF-003:** Measure catalog
      parsing and bulk projection at realistic catalog sizes. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/05-localization-i18n.md)

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

The items below are local fixes. The large wins are the
[change-proportional frame pipeline](#change-proportional-frame-pipeline--next-implementation-priority)
at the top of this file; prefer it over further single-pass scoping.

### Render sweep findings

From a read-only code sweep on 2026-10-19; nothing below is measured yet.
Verify each against `navigation_hover_profile`, `appearance_frame_cost_profile`
or `gpu_frame_cost_workload` before landing. Roughly in order of payoff.

- [ ] A rounded `overflow` ancestor sends every descendant down the affine path
      with a shader clip per node (`display_list/types.rs:566`). Use a rect clip
      for nodes missing the corner squares; longer term, push/pop clip scopes.
- [ ] Multi-rect damage replays the selection once per rect when it contains a
      layer scope (`painter/tree.rs:363,596`); paint once under `clip_region`.
- [ ] A scroll viewport is one opaque replay span, so damage inside it replays
      all content including offscreen rows (`display_list/build.rs:684`).
- [ ] Popover show rebuilds the retained GPU target and dmabuf ring. Keep the
      target while hidden and pool dmabuf slots per size in `GpuPresenter`.
- [ ] GPU target and dmabuf ring reallocate on every size change; round both up
      to 64 px classes like the SHM path and crop with the viewport.
- [ ] Opacity: `opacity: 0` subtrees still rasterize into a layer, and single-
      primitive leaves open a `save_layer`; prune the first, fold alpha into the
      paint for the second. Animated opacity/transform layers have no raster cache.
- [ ] The GPU painter keeps CPU-tuned choices (A8 atlas rounded fills, shader
      clips, CPU-rastered shadows/gradients); try analytic `draw_rrect`/
      `clip_rrect` and mask-filter shadows on Ganesh behind a parity test.
- [ ] Skia context: no persistent shader cache, no `perform_deferred_cleanup`,
      one flush plus fence per surface per frame instead of one per frame.
- [ ] Text: each layout-cache hit removes and reinserts its entry (~15–25 allocs
      per text node per paint), and the shared 512-entry cache may never hit on
      Settings; check `TextCacheMetrics.layout_misses` first.
- [ ] Text draws one `draw_image_rect` per glyph; batch with `draw_atlas` per run,
      later Skia text blobs. Glyph pages re-upload the whole page on growth.
- [ ] Icon tint is in every icon raster key, so colour changes re-rasterize;
      cache A8 masks and tint at draw. File-icon misses rasterize twice, once on
      the UI thread. Missing icons re-queue resolution jobs.
- [ ] Images over 16 MB never enter `SKIA_IMAGE_CACHE` and are copied and
      re-uploaded every paint; wrap the decoded `Arc` without copying.
- [ ] An icon-pack publish advances the shared resource revision and rebuilds
      `FontSystem`; split font and icon revisions. `TextRenderer::new` does a
      system font scan that is discarded on the first frame.
- [ ] Frame pacing is per component, not per target, so popover frames wait on
      the parent's callback. Service updates run Lua per message, not per frame.
- [ ] Sparse frames still pay ~10 full passes (batch index rebuild, topology
      check, backdrop regions, layer-scope collection); the subtree reuse map
      drops descendants of reused subtrees (`build.rs:433`) — add a regression.
- [ ] Hit testing visits the whole tree without bounds culling and clones a clip
      `Vec` per node on pointer move; hover keys are re-parsed to `NodeId`s.
- [ ] Effects queued during render wait for the next wakeup; settle them or
      schedule immediate work after painting.

### Scroll and animation frames

From the 2026-09-30 Settings scroll session profile; evidence and A/B numbers
in the [performance log](../.planning/log/performance-log.md). Roughly in order
of payoff.

- [ ] Fractional momentum scroll frames, and scopes over a non-uniform backdrop
      (gradient, image, partial cover), still replay the whole viewport; a
      transparent content raster could cover them ([design](../.planning/todos/pending/2026-09-30-scroll-raster-cache.md)).
- [ ] In-surface blur re-blurs and repaints a frosted element's whole read
      region on any nearby change (a hover inside a frosted panel is ~7x a plain
      repaint). Backdrop root, clamped edges and the blurred-backdrop/filter-layer
      caches are phases 3–5 of the [in-surface blur engine](../.planning/todos/pending/2026-09-30-in-surface-blur-engine.md),
      gated on the D6 renderer prototype.

### Live glass session

From the 2026-10-20 live `glass` captures; evidence in the
[performance log](../.planning/log/performance-log.md).

- [ ] First open of a glass surface costs 100–160 ms (tree build plus first GPU
      frame); prewarm hidden surfaces or split the first frame.

### Style

- [ ] Typed style declarations: 97.5% of declarations in an Appearance rebuild
      now apply typed. `font-family`, `color`/`background` with fallbacks, and
      the per-node `transition` shorthand parse remain on the string path.
      Restyle time is now mostly node resolution and `ComputedStyle` copying,
      not declaration application. _(detail: "P2 — typing & interning")_
      _(detail: "P2 — architecture")_

### Threading

- [ ] Bring child popup surfaces into the parallel CPU raster phase and move
      the SHM copy off the shell thread; both still run inside reconcile/present.
- [ ] Tile-parallel raster for large damage, above a measured threshold only.
- [ ] Move blocking file IO off the shell thread — i18n catalog mounts,
      settings and theme reloads, and icon/SVG cache-miss rasterization on the
      paint path — via `spawn_blocking` plus completion events.

### Runtime boundary

- [ ] Handler sync still reads compound table globals, because nested in-place
      mutations never assign through `_ENV`. Eliminating those reads needs
      recursively tracked tables or Rust-owned reactive values (R).
      _(detail: "P1 — boundary & dispatch")_
- [ ] Storage reads still clone per Lua access. Needs shared immutable JSON
      values or lock avoidance — two cache designs were measured and reverted
      (I; see log).

### Rendering and paint

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

- [ ] Experimental persistent-scene/database renderer that extends retained
      identity into raster resources and tiles, measured against the Skia-GL
      path. [Plan](../.planning/todos/pending/2026-09-20-persistent-scene-renderer.md).

- [ ] **Existing default-feature test failures:** shipped-graph tests
  assume the root graph, but `config/active-profile` is `glass`, so composition is
  profile-driven. Shell real-surface popovers anchor at y 6 instead of 26.
  Triage with `cargo test --workspace --no-fail-fast`; `HEAD` baseline is in
  the log.
