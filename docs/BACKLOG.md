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

## 2026-09-30 block review and module-architecture direction

Ordered by implementation priority: finish a tier before starting the next
unless an item says otherwise. IDs point into the
[block review](../.planning/codebase/audits/2026-09-30-block-review/REPORT.md)
(evidence and tests); `A1`–`A8` point into the
[module architecture direction](../.planning/todos/pending/2026-09-30-module-architecture-direction.md).

### Priority 3 — Wrong output users see now

- [ ] **RND-09 (fixes RND-01, RND-08):** One final damage stage that collects all
      producers, converts once to device pixels, makes rects disjoint, and feeds
      clear, paint, SHM copy and `damage_buffer`.
- [ ] **RND-06 / RND-07:** Use the output size only for an unspecified configure
      axis; keep buffer scale consistent with buffer size without a viewport.
- [ ] **RES-03 / RES-04 / RES-14:** ICU date formatting in the local zone plus
      `format_time`/`format_percent`; localize the clock and quick settings;
      apply `en` after a module's `defaultLocale`.
- [ ] **RES-07 (env option):** Exec children now run in `LC_ALL=C.UTF-8`; add an
      explicit, capability-checked env option for tools that need another locale.

### Priority 4 — Contracts and syntax to settle before third-party modules

- [ ] **A1:** Extract `mesh.wm`, `mesh.power`, `mesh.brightness` into interface
      modules; reserve `mesh.*` by provenance; review `mesh.wm` against niri/Sway.
- [ ] **A7:** Publish the semantic design-token vocabulary as a versioned
      contract, with LSP/doctor warnings for tokens outside it.
- [ ] **A5:** Merge `frontend`/`component` into one UI kind with optional default
      placement; evaluate one mixed resource-pack kind.
- [ ] **RT-09:** Contract-declared coalescing keys instead of guessing targets
      from field names like `device` and `sink`.
- [ ] **CMP-02 / CMP-08:** Handlers and callbacks as Luau function values, parsed
      with full_moon, replacing global-name strings and the hand-written splitter.
- [ ] **CMP-07 / CMP-06:** `<props>` as a statically parsed Luau table shared with
      future backend props; remove the legacy `import X from` scanner.
- [ ] **RES-06 / RES-12:** One default theme with dark/light modes and quick
      settings using `set_mode`; remove core's hard-coded inspector tab enum.

### Priority 5 — Control-plane consolidation

- [ ] **MOD-05:** Package transactions take the control-plane lock while
      protecting or restoring profile files, so rollback cannot erase a
      concurrent profile commit.
- [ ] **MOD-06:** One provider-eligibility function shared by the activation
      closure and graph provider selection.
- [ ] **MOD-09 / MOD-10 / SH-10 / RES-08:** Move install/uninstall planning,
      `effective_profile_settings`, config paths and the control-plane lock into
      core; make every CLI config/locale command profile-aware.
- [ ] **MOD-07 / MOD-08:** Removal deltas for composition services and
      providers; inline the resolved composition before force-uninstalling it.
- [ ] **MOD-12:** Retain a last-valid manifest only while the rest of the module
      tree is unchanged, and compare the manifest name in discovery mode.

### Priority 6 — Dead code, duplication and small fixes

- [ ] **SH-07:** Remove the crate-level `allow(dead_code)` in the shell, delete
      the 43 dead items, gate test wrappers, drop the default-identity wildcard.
- [ ] **UI-10 / RT-08 / CMP-05 / CMP-12:** Delete legacy translation-only
      hit-test paths and `EventDispatcher`, duplicate runtime supervision and
      tier types, the test-only expression evaluators, and fold `frontend-host`.
- [ ] **MOD-11:** Remove snake_case serde aliases, TOML `ShellConfig`, test-only
      `mesh.toml` discovery and the dead `merge_root` branch.
- [ ] **SH-09 / RES-13:** One icon-pack validator and one contained file reader
      shared by shell, LSP, theme, locale and resources.
- [ ] **RES-09 / RES-10:** Derive LSP manifest keys and capability completion
      from core; complete services from contracts only.
- [ ] **RES-11:** Move manifest-less backend fixtures out of `modules/` and delete
      the path-rewrite shim.
- [ ] **RT-07:** Drop the per-command JSON `state` rollback; publish snapshots
      only from successful callbacks.
- [ ] **UI-12:** Resolve percent/`auto` lengths against layout before
      interpolating, and shorten reversed transitions by their progress.
- [ ] **RND-05:** Take shell visual-damage rects from the display list's world
      bounds (ancestor transforms and scroll) and delete the parallel geometry.
- [ ] **RT-11 / CMP-11 / CMP-13:** Allow finite recursive contract types; span
      `prop()` diagnostics and check inline styles; resolve child settings once
      per revision.

### Priority 7 — Customization and ecosystem

- [ ] **A3:** Style parts (`part="…"`) plus a profile-scoped user stylesheet
      targeting `::part()`.
- [ ] **A6 / RES-05:** Ship bar widgets as separate `mesh.navigation.item`
      modules with per-instance props; share volume presentation in a library.
- [ ] **A2:** Optional named features inside contracts with `supports()` checks.
- [ ] **A8:** Scoped D-Bus, file read/watch and Wayland protocol host primitives
      (extends the D-Bus item under Runtime boundary); plain-language permissions.
- [ ] **A4:** `module fork` / `diff` / `rebase` with recorded upstream.

### Priority 8 — Structural mechanics

- [ ] **SH-11:** Immutable per-frame `PaintContext` replacing thread-local render
      setters; prerequisite for the parallel-paint items under Threading.
- [ ] **SH-08:** Extract `shell/component/**` into a `mesh-core-frontend-runtime`
      crate.
- [ ] **UI-09:** Per-element behaviour table in `mesh-core-interaction` (after
      SH-08), starting with input caret keys/selection and slider Home/End/Page.
- [ ] **RT-06:** Per-context budget counters and heap attribution on the shared
      frontend VM.
- [ ] **RT-04:** Stop blocking tokio workers in `mesh.exec`; later an async exec
      host API.

### Priority 9 — Performance hypotheses to measure

Each needs its named workload from the report before any change lands.

- [ ] **RND-02:** Damage only scroll viewports whose own geometry or ancestry
      changed, not every viewport on any layout change.
- [ ] **RND-03 / RND-04:** Stop rebuilding unread frame-plan data and the
      whole-tree visual-damage map on every paint.
- [ ] **SH-05 / SH-06 / SH-13:** Font-alias cache thrash between modules, double
      `Debug` formatting per effect, idle watch-set rebuilds.
- [ ] **UI-11:** Compile keyframe rules once per animation instance and theme
      revision instead of re-resolving string declarations every frame
      (evidence for S09-PERF-002).
- [ ] **CMP-04 / CMP-09 / CMP-10 / RES-15:** Unbounded expression cache and
      per-parse thread spawns, per-call Lua allocations in template expressions,
      per-call ICU formatter construction.

## Authoring model

Goal: components written as plain Luau, with the framework owning dependency
tracking, validation and scheduling.
[Review and evidence](../.planning/todos/pending/2026-10-10-authoring-model.md).

- [ ] Move shipped modules' precomputed class strings, inline styles and
      `hidden` flags onto the shipped `class:` / `style:` directives and
      `{#if}`; only the two nav-bar popover triggers use them so far.
- [ ] Re-run `render(self)` only for instances whose tracked reads changed; today
      any dirtying service update re-runs every instance's hook on the surface.
      Then evaluate `computed` values and `mesh.watch(proxy, field, fn)`.
- [ ] Typed proxy fields from compiled contracts (with S03-DEAD-001): range-checked
      values, defaults, and a defined absent-optional-service value, so modules
      drop their `tonumber`/clamp/`pcall(require)` guards.
- [ ] One service call path: typed proxy methods only. Retire
      `mesh.events.publish("mesh.<svc>.<method>")`, which shipped modules use
      alongside the proxies.
- [ ] Optimistic pending values on proxy methods, reconciled with emitted state
      through contract coalescing keys (RT-09), replacing the hand-written
      set-then-reconcile pattern in sliders.
- [ ] Plain `.luau` libraries shareable between modules (`require` of a library
      export); helpers such as volume-icon thresholds exist in three copies.
- [ ] `self` at script top level, plus event forwarding in markup
      (`onpointerenter={self.PopoverEnter}`), replacing `init` captures and
      one-line forwarding handlers.
- [ ] Locale matching helper (`mesh.locale.is("sk")`) alongside RES-03's
      formatters, and document when `render(self)` runs.

## Runtime architecture direction

Larger changes to the runtime model under scripts; the platform model stays.
Each needs a design and a measured prototype before it is adopted.
[Direction note](../.planning/todos/pending/2026-10-10-runtime-architecture-direction.md)
(suggested order inside).

- [ ] **D1:** Compile-time fine-grained reactivity: top-level variables become
      signals, template expressions bind straight to node properties, and
      `render()` is retired. Prototype on the navigation bar first.
- [ ] **D2:** Service calls, exec, sockets and file reads that yield inside
      `task.spawn` instead of answering through events or callbacks. The
      per-instance scheduler (`task.delay`/`spawn`/`sleep`) has shipped.
- [ ] **D3:** Object capabilities for services: authorize when a service proxy
      is acquired, then calls are plain typed calls. `mesh.shell` has shipped
      this way; service methods still authorize per call.
- [ ] **D4:** Typed values end to end: contract-generated types, validators and
      `.d.luau`; providers send field patches; no JSON on the frame path.
- [ ] **D5:** Headless interaction primitives in core (popover, menu, tooltip,
      hover intent, focus scope, dismissal) that modules compose.
- [ ] **D6:** Immutable scene snapshot committed to a render thread with GPU
      raster; re-scope the scroll raster cache and in-surface blur plans first.
- [ ] **D7:** Styles compiled to typed form with dependency-indexed incremental
      selector matching; class/state changes restyle only matching nodes.
- [ ] **D8:** Event-driven backend host APIs (D-Bus signals, PipeWire, netlink,
      inotify) on D2 coroutines; polling becomes the fallback.
- [ ] **D9:** CI performance gates per workload: idle wakeups, hover, service
      poll, scroll, popover open, theme switch.


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
      source for runtime, Luau, and documentation projections, retaining raw
      declarations only behind explicit compatibility/tooling adapters. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/03-service-contracts.md)

### Section 04 — Themes

- [ ] **S04-PERF-002 / S04-PERF-003 / S04-DEAD-003:** Share one bounded CSS,
      token, and keyframe representation across theme and component paths, then
      measure typed token dependency resolution and reload reuse. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/04-themes.md)

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

### Scroll and animation frames

From the 2026-09-30 Settings scroll session profile; evidence and A/B numbers
in the [performance log](../.planning/log/performance-log.md). Roughly in order
of payoff.

- [ ] Scrolling repaints all visible content (one Skia draw per glyph) every
      frame. Retain a bounded content raster and shift integral device pixels,
      repainting exposed strips; [design](../.planning/todos/pending/2026-09-30-scroll-raster-cache.md).
- [ ] Unscoped restyle frames still fingerprint the whole tree
      (`retained_tree.update`). Scroll momentum no longer takes this path; what
      remains (resize, theme, window state) also relayouts, so diffing only
      restyled nodes needs layout-changed nodes reported too.
- [ ] In-component blur costs a full re-blur on every nearby repaint, and
      in-surface `backdrop-filter` has only an outline contract (04 §10: the
      no-protocol fallback). Plan: [in-surface blur engine](../.planning/todos/pending/2026-09-30-in-surface-blur-engine.md).

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
      "styles still valid" mark the restyle walk can skip — a value-level
      share cache was measured and rejected (see log).

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

- [ ] D-Bus signal subscription as a backend host primitive, so UPower and
      NetworkManager providers stop adopting monitor subprocesses (C); then
      re-measure the at-rest fork rate on a live session.
- [ ] Handler sync still reads compound table globals, because nested in-place
      mutations never assign through `_ENV`. Eliminating those reads needs
      recursively tracked tables or Rust-owned reactive values (R).
      _(detail: "P1 — boundary & dispatch")_
- [ ] Storage reads still clone per Lua access. Needs shared immutable JSON
      values or lock avoidance — two cache designs were measured and reverted
      (I; see log).
- [ ] One hover-intent mechanism: the shell defers popover hides by 180 ms,
      while six modules run their own 400 ms `task.delay` close bridge.
      [Design](../.planning/todos/pending/2026-10-10-script-timers-and-typed-shell-ops.md).
- [ ] A promoted popup's own pointer-leave never schedules its close bridge
      (`navigation_language_popover_closes_when_pointer_leaves_promoted_popup`
      fails; `navigation_hover_profile` ends with every popover open). Find
      whether the shell or only the test harness drops the leave.

### Rendering and paint

- [ ] Narrow script frames run full-tree `normalize_accessibility` (~20% of a
      navigation audio poll): memo-reused subtrees are stored pre-finalization,
      so `normalize_accessibility_dirty` has no normalized data to keep for them.
- [ ] `navigation_bar_catalog` has no `mesh.wm` provider, so every navigation
      test and `navigation_frame_cost_profile` render the start slot as error
      placeholders. Add one so the bench measures the real workspace list.
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

### Idle and scheduling

- [ ] Idle wakeups: theme, settings and frontend metadata polls keep separate
      deadlines and keep running while inotify is healthy, plus a 1 s watch
      reconcile (see SH-13). Coalesce them into one deadline, or stop polling
      when the watcher is healthy and rescan on queue overflow.
- [ ] Backend `emit` validates the payload JSON, then `publish_changed_update`
      validates it again and deep-clones it into `last_payload`; validate once
      and keep a fingerprint instead of the clone.

### Startup and catalog

- [ ] Narrow frontend catalog index rebuilds to graph deltas. Compiled sources
      now survive live graph changes by manifest/source fingerprint, but slot
      and validation indexes still rebuild across the catalog.

### Architecture

- [ ] Advance GPU rendering on two measured lanes: the conservative Skia-GL/EGL
      migration and an experimental persistent-scene/database renderer that extends
      retained identity into raster resources and tiles. Plans:
      [GPU backend](../.planning/todos/pending/2026-07-15-gpu-rendering-backend.md) and
      [persistent scene renderer](../.planning/todos/pending/2026-09-20-persistent-scene-renderer.md).
