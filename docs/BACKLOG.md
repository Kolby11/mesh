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

- [ ] Implement the Material reference shell (`08-material-desktop`): editable desktop widget grid, widget picker, editing toolbar, side rail and dock, with a composition/profile and token-based theme. [Design](../.planning/todos/pending/2026-10-27-reference-shells.md).
- [ ] Give `@mesh/material` its own composition once `08-material-desktop` lands; the theme recommends `@mesh/desk` until then.
- [ ] Update the Settings and style-fixture tests that still expect glass panels under the solid themes (`clicking_a_theme_card_repaints_settings_in_the_new_theme_in_one_frame`, `settings_glass_surfaces_follow_the_active_theme_palette`, two `style::tests::fixtures` colours); the themes moved to Material surfaces on 2026-10-29 and the expected looks are undecided.
- [ ] Make liquid-glass Settings readable over applications with a light, translucent outer panel and calmer inner controls; the shared glass palette is too transparent.
- [ ] **Styleable control parts:** add scrollbar (`::track`, `::thumb`) and input (`::placeholder`, `::caret`, `::selection`) parts; the renderer still hardcodes their looks, and neither fits the child-node model. [Design](../.planning/todos/pending/2026-10-29-control-parts.md).
- [ ] Animate the slider value itself (thumb glide on click-to-jump and keyboard steps), as `switch` glides with its thumb's `margin` transition; a drag must not trail the pointer, so the glide needs to skip pointer-driven changes.
- [ ] Isolate hyprglass/compositor GPU cost during widget resize; native-namespace replay confirms plugin draws and removes parent rebuilds, but plugin stage timers remain disabled and display latency is unmeasured.
- [ ] Continue the whole-codebase audit: sections 01–09 have review reports; sections 10–16 plus cross-section review and final inventory reconciliation remain.
      Resume the three review layers and file coverage in [the continuation inventory](../.planning/codebase/audits/2026-10-29-whole-codebase/00-coverage.md); [section 10](../.planning/codebase/audits/2026-10-29-whole-codebase/sections/10-frontend-compiler-and-host.md) is in progress.
- [ ] Project documented icon axis custom properties into computed icon styles; generic custom-property handling currently bypasses their lowering. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/sections/08-ui-element-core.md).
- [ ] Bound custom-property and prop-reference resolution depth and work; long acyclic chains recurse without a limit. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/sections/08-ui-element-core.md).
- [ ] Include keyboard shortcuts in semantic snapshot diffs; shortcut-only changes currently produce no field change. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/sections/08-ui-element-core.md).
- [ ] Align transition shorthand property selection with longhand; names such as `padding-left` and `row-gap` are currently dropped. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/sections/08-ui-element-core.md).
- [ ] Preserve retained layout when an existing unkeyed node gains a key; structural reconciliation can delete its reused Taffy node and panic. [Audit](../.planning/codebase/audits/2026-10-29-whole-codebase/sections/08-ui-element-core.md).
- [ ] Preserve independent row and column gaps through style resolution and layout; both currently share one value. [Audit](../.planning/codebase/audits/2026-10-29-whole-codebase/sections/08-ui-element-core.md).
- [ ] Exclude shapes with invalid geometry from hit testing; they currently retain a rectangular hit region with no shape content. [Audit](../.planning/codebase/audits/2026-10-29-whole-codebase/sections/08-ui-element-core.md).
- [ ] Preserve quoted asset filenames during CSS reference expansion; `var(` or `prop(` in a filename is currently substituted. [Audit](../.planning/codebase/audits/2026-10-29-whole-codebase/sections/08-ui-element-core.md).
- [ ] Handle non-finite accessibility range attributes locally; accepted values such as `NaN` currently invalidate the frame snapshot. [Audit](../.planning/codebase/audits/2026-10-29-whole-codebase/sections/08-ui-element-core.md).
- [ ] Propagate supported inherited text properties such as font style and direction; child styles currently fall back to their defaults. [Audit](../.planning/codebase/audits/2026-10-29-whole-codebase/sections/08-ui-element-core.md).
- [ ] Invalidate cached style rule indexes when rules change in place; pointer and length reuse can retain old declarations. [Audit](../.planning/codebase/audits/2026-10-29-whole-codebase/sections/08-ui-element-core.md).
- [ ] Derive popover trigger expansion even when a closed popup is hidden; semantic publication currently leaves conflicting authored state in place. [Audit](../.planning/codebase/audits/2026-10-29-whole-codebase/sections/08-ui-element-core.md).
- [ ] Convert scroll-to-view distances into the scroll owner's coordinates; transformed containers can overshoot or move in the wrong direction. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/sections/09-interaction-and-motion.md).
- [ ] Hit-test overlapping scrollbars in paint order; reverse insertion order can select a lower z-index sibling. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/sections/09-interaction-and-motion.md).
- [ ] Keep reverse and alternate stepped keyframe wake deadlines from missing changes immediately after boundaries. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/sections/09-interaction-and-motion.md).
- [ ] Make fork creation format-independent and transactional; valid minified manifests fail after leaving partial copies. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Classify removal of optional positional service arguments as breaking; existing callers can still supply them. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Preserve validated stylesheet image paths through style lowering; resolved relative URLs are rejected as absolute. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Make keyboard focus traversal reach offscreen scroll descendants and reveal the focused control. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Bound command pipe capture and shutdown when descendants outlive the direct child; retained pipes can block worker cleanup. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Retry Wayland flush after transient write backpressure; `WouldBlock` currently propagates as an error. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Preserve distinct, resolvable diagnostic issue identities when codes exceed the size limit; truncation currently breaks lookup and can merge issues. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Preserve error severity when diagnostics overflow; a full warning table can hide a later error. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Bind debug snapshot payloads to the generations they captured; a concurrent diagnostic update can leave stale data cached under a newer key. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/sections/01-core-foundation-contracts.md).
- [ ] Review and consolidate the unused public config diagnostic model and redundant settings accessors; repository consumers use the newer settings contracts. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/sections/01-core-foundation-contracts.md).
- [ ] Use bounded, descriptor-relative catalog reads in authoring diagnostics; the current separate path check and unbounded read can race and exhaust memory. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Make fork rebases recoverable across partial writes and baseline replacement; failures can leave edits and upstream records out of sync. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Preserve explicitly added UI roots in composition-backed profiles; composition resolution currently treats them as orphaned overrides. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Bound authoring source-scan traversal and rejection records; source byte limits do not limit directory depth or entries. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Rewire forked root instance identities and dependent references; the current profile retains the upstream key and fails validation. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Bound and contain frontend entrypoint reads during contribution indexing; props extraction bypasses the source scan budget. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Verify the winning activation snapshot after a publication race; a different same-generation snapshot can currently be accepted. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Correct extension contribution isolation documentation; its per-VM claim conflicts with the shared-realm platform contract. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Make manifest normalization idempotent; runtime conversion duplicates binary dependencies and contribution lists. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Share the canonical settings merge between composition and profile resolution; the current duplicate implementations can drift. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Enforce provenance trust when resolving required module dependencies; closure activation can re-enable a trust-rejected module. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Account for tracked-fork mappings during uninstall reference checks and forced cleanup; dangling mappings can prevent profile loading. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Validate profile candidates before revision-checked persistence; invalid CLI settings namespaces can otherwise make a saved profile unloadable. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Remove orphaned node-slot overrides during profile prune; the command currently reports success without deleting them. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Check executable availability consistently with runtime resolution; regular non-executable files currently satisfy backend binary dependencies. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Validate extension-point host version requirements and fail closed on invalid ranges; malformed ranges currently admit contributions. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Enforce single-contribution extension points; overfill currently produces a diagnostic while all entries remain renderable. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Scope automatic extension contribution instance identities by source module; equal local IDs can collide in component state and memoization. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Bound and contain external interface contract reads during graph loading; oversized or replaced files bypass source-scan limits. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Resolve slot replacements to the target's actual contribution and metadata; changing only the source module can produce missing entries or stale props. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Measure per-module signature verification cost during graph loading; signed modules currently launch serial verifier processes. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Build an author-facing component test harness after SH-08, with deterministic services, time and input plus semantic, request and render assertions; current reusable fixtures are shell-private Rust tests. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Add repeatable component profiling scenarios and run metadata on existing profiling tools; arbitrary components lack a reusable workload and comparable samples. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Correlate input and service changes through surface preparation, GPU completion, commit and available presentation feedback; existing per-stage samples do not show end-to-end latency. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Support deactivating inherited profile roots through CLI commands and distinguish resetting an override from removing a root. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Recompute active dependency reachability after blocking roots; the resolver can retain dependencies reached only from a failed root. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Use exclusive no-follow temporary files for store pointer writes; predictable PID-based temporary paths can follow existing symlinks. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Validate extension-point prop type expressions and reject malformed declarations; resolution currently accepts parse failures. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Isolate detached-signature load failures to their modules; one malformed signature currently aborts graph loading. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Propagate interface-compatibility blocking through module dependency closure and retire no-longer-reachable dependencies. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Generate contract-valid service mocks or report unsupported schemas; current defaults and method returns can violate declared types. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Resolve service member collisions with proxy `state` and `events` namespaces; accepted names can be inaccessible to Luau consumers. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Classify required named-record field additions according to input/output compatibility; existing method inputs can fail after an additive classification. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Validate positional optional argument ordering or arity by position; an optional before a required argument can bypass the proxy's count check. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Ensure service symbols generate valid Luau for keywords and punctuation, with parser or runtime tests for generated artifacts. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Preserve correct rule provenance when module theme layers merge; appended rules currently reuse earlier indices. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Bound theme token alias and fallback resolution; long acyclic chains can recurse deeply and repeat linear cycle checks. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Share theme style-rule lowering between global and module scopes; duplicated token, default and selector logic can drift. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Resolve scoped theme tokens for dotted module IDs; first-dot splitting misidentifies valid module names. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Align core translation point and bulk lookup fallback with argument-aware lookup; their English last-resort behavior differs. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Reject unmatched closing braces throughout localized messages; validation currently misses braces before later placeholders. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Align icon coverage advice with live alias, dash fallback and owner-scoped resolution; advice can report gaps for icons that resolve. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Reject duplicate glyph names in JSON icon maps; parsing currently collapses duplicates before validation. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Resolve icon overrides through MESH pack-qualified mappings; current override lookup treats targets as file or XDG assets. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Preserve user resource precedence when XDG data home is absent; filtering currently lets a system directory occupy its ordering slot. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Include override-only font roles in renderer alias projection; registry resolution and rendered `font-family` can disagree. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Ignore template braces and directives inside XML comments while retaining accurate spans and editor recovery. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Use scoped expression references in standalone component validation; local function parameters can be reported as unknown globals. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Evaluate `style:` directive expressions once; lowering currently repeats stateful expressions during value checks. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Preserve accepted numeric prop values in CSS projection; large integral floats currently saturate during integer formatting. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Distinguish Luau `--[` line comments from valid long-comment openers in template expression scanning. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Decode Luau string literals consistently during component import analysis; escaped paths can differ from runtime values. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).
- [ ] Distinguish generated expression markers from authored attribute literals; marker-shaped text can become a binding. [Audit](../.planning/codebase/audits/2026-10-27-whole-codebase/INTERIM.md).

- [ ] Build interchangeable shell compositions, configurable `.mesh` components and independently switchable themes that can realize the diverse [shell references](reference/shells/); the full range is not yet available.
      Support live shell and theme switching and hot reload with a polished transition effect, respecting reduced-motion preferences.
      Plan, shell list and order: [reference shells](../.planning/todos/pending/2026-10-27-reference-shells.md).
- [ ] Animate appearance switches: crossfade theme/mode changes, icon-theme changes and font-family changes instead of snapping, honouring reduced motion.
      Today each switch swaps tokens, icon resolutions or glyph metrics in one frame. Design: [appearance transitions](../.planning/todos/pending/2026-10-28-appearance-transitions.md).
- [ ] Patch changed props through retained child bindings; parent-preserving child-subtree refresh is shipped, but the child template and conservative surface finalization still run.
- [ ] Extend `<shape>` beyond sectors, arcs and polylines (curves, gradient or image fill, focus outline along the shape) for the ring gauges, graphs and sky arc in the reference shells.
- [ ] Extend `mesh.weather` (sun times, high/low, humidity, wind, UV) and `mesh.system` (GPU, battery, CPU frequency, swap, network history) and their providers; reference-shell widgets need these fields.
- [ ] Finish the keybind system: a settings page for `key_priority`, hint mode and corner; inline remap and "reassign all" in the help window; `mesh-shell keys help|hints` for compositor binds; ordinal keys for repeated items; verify key hints on a live GPU surface and in promoted popups. Design and status: [keybind system](../.planning/todos/pending/2026-10-28-keybind-system.md).
- [ ] Add one shell-switch keybind that works in any profile, and require the profile switcher in every new composition; today switching relies on each shell's own bar button.
- [ ] Connect Orbit's market, activity and usage cards to real data sources and complete their reference visuals; those sources and graph/grid views are not implemented yet.
- [ ] Verify Orbit's desktop-card appearance and component interactions on a clear Wayland desktop, including narrow outputs; live activation and profile switching work, but the cards were obscured by windows.

- [ ] Recorded-scene panel GPU parity exceeds its pixel budget on the available
      driver; reproduced on unchanged `f6a68c5a` and the scene-retry candidate.

- [ ] Investigate intermittent fractional retained-layout parity: the full shell matrix can disagree
      with full layout at `fractional/1024/1024/0`, while an isolated rerun passes.
      Also diagnose forced-reference collapse at `grow/1024/1024/0` in debug; release passes.

- [ ] Prevent scheduler contention from exhausting the frontend callback wall-clock budget before
      a component initializes; the inspector can render an error placeholder under CPU contention.

## Change-proportional frame pipeline — next implementation priority

Frame cost scales with tree size times about ten passes, not with what
changed; scoping single passes buys only 3–10% each. Do these in order before
other performance work. Evidence, the Qt Quick / GTK4 comparison, exit
criteria and the items folded in here:
[design](../.planning/todos/pending/2026-10-21-change-proportional-pipeline.md).

- [ ] **P3 / D1 (deferred pending runtime tracking):** Record compound-table
      writes instead of comparing every compound global as JSON. Indirect list
      reads still compare every item.

## 2026-09-30 block review and module-architecture direction

Ordered by implementation priority: finish a tier before starting the next
unless an item says otherwise. IDs point into the
[block review](../.planning/codebase/audits/2026-09-30-block-review/REPORT.md)
(evidence and tests). The `A*` module-architecture items have landed; see the
October log.

### Priority 8 — Structural mechanics

- [ ] **SH-08:** Extract `shell/component/**` into a `mesh-core-frontend-runtime`
      crate. [Design](../.planning/todos/pending/2026-10-27-frontend-runtime-extraction.md).
- [ ] **UI-09:** Per-element behaviour table in `mesh-core-interaction` (after
      SH-08), starting with input caret keys/selection and slider Home/End/Page.
- [ ] **RT-06:** Per-context budget counters and heap attribution on the shared
      frontend VM.

## Glass shell follow-ups (found 2026-10-20)

- [ ] **Settings theme switch may not repaint:** reported 2026-10-21 as needing a tab change; not
      reproduced on `1d50170a` (layer and window roles, file and click paths). Capture a repro.
- [ ] **Initial queries on differing outputs:** before Wayland reports placement,
      a new surface has no unambiguous output-size hint. Resolve first-paint
      container queries without guessing an output or sizing content from a placeholder.

## Glass shell core gaps (found 2026-10-17)

- [ ] **Live refraction placement:** before restoring source-based refraction beyond the wallpaper,
      use authoritative geometry and a source that accounts for foreign wallpaper layers and exclusive zones;
      predicted origins cannot guarantee alignment. Multi-output source sampling remains untested.

## Authoring model

Goal: components written as plain Luau, with the framework owning dependency
tracking, validation and scheduling.
[Review and evidence](../.planning/todos/pending/2026-10-10-authoring-model.md).

- [ ] Narrow compound public inputs and captured template-helper dependencies;
      tables and function calls still use conservative invalidation.
- [ ] Optimistic pending values on proxy methods, reconciled with emitted state
      through contract coalescing keys, replacing the hand-written
      set-then-reconcile pattern in sliders.

## Runtime architecture direction

Larger changes to the runtime model under scripts; the platform model stays.
Each needs a design and a measured prototype before it is adopted.
[Direction note](../.planning/todos/pending/2026-10-10-runtime-architecture-direction.md)
(suggested order inside).

- [ ] **D4:** Providers send typed field patches instead of whole snapshots, and
      no JSON on the frame path. Validation, ranges/defaults and `.d.luau`
      generation have shipped, and LSP proxy types use the same mapping.

## Native providers and bulk data channels

Proposed direction, not yet in the spec: MESH stays a curated platform for shell
experiences, adds out-of-process native providers, and adds typed bulk channels.
[Design note](../.planning/todos/pending/2026-10-28-native-providers-and-bulk-channels.md)
(scope, language evaluation, concerns to settle first, suggested order).

- [ ] Accept or amend the direction and update 00 §7, 01 and `AGENTS.md`; the
      spec still says module execution stays in-process and no native tier is committed.
- [ ] Add a capability-gated host standard library for Luau (JSON, date/time,
      regex, fuzzy matching, HTTP) and `require` of vendored pure-Luau packages;
      Luau's library gap is the motivation for considering QuickJS + TypeScript.
- [ ] Prototype QuickJS-ng + stripped TypeScript behind a language-neutral
      script boundary only if the host library is not enough; needs benchmarks on existing gates.
- [ ] Contract-declared sample streams bound directly to elements;
      `cava-visualizer` republishes whole-array state through Luau every frame.
- [ ] Windowed keyed collections for large lists, paired with the virtualized
      list under Performance; providers currently publish whole lists as state.
- [ ] Out-of-process native provider protocol and `mesh-provider` Rust SDK,
      proven on `backlight-brightness`; contract-generated Rust bindings exist, but no
      wire protocol, SDK runtime or native tier does.
- [ ] Frame streams (dmabuf + fences, shared-memory fallback) and a live-texture element,
      starting with window previews; needs a compositor-portable capture source and core-owned consent.


## 2026-09-01 whole-codebase audit — new open tasks

New findings from the 2026-09-01 audit synthesis, sorted by audit section and
grouped where one implementation should resolve several findings. Detailed
evidence and test workloads are in the linked reports.

### Section 02 — Module system and installation

- [ ] **S02-LOGIC-003 / S02-LOGIC-011:** Bind activation candidates to the
      same store generation and independently verified source revision; source
      revision provenance remains lock-only. [Audit](../.planning/codebase/audits/2026-09-01-whole-codebase/sections/02-module-system-and-installation.md)
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

### Section 05 — Localization and i18n

- [ ] Reduce remaining localized component-mount runtime and tree setup cost;
      storage recovery filtering is bounded, but host API installation and child runtime setup remain expensive.

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

- [ ] Provide a real evaluator for production preview expressions; the current no-composition path returns null while unit tests use Luau. [Audit](../.planning/codebase/audits/2026-10-29-whole-codebase/sections/10-frontend-compiler-and-host.md).
- [ ] Tie frontend compilation fingerprints to the exact source bytes parsed; edits between parsing and later fingerprint reads can publish stale compiled content as current. [Audit](../.planning/codebase/audits/2026-10-29-whole-codebase/sections/10-frontend-compiler-and-host.md).
- [ ] Assess collision-safe equality for frontend memo fingerprints; the 64-bit hash alone decides reuse. [Audit](../.planning/codebase/audits/2026-10-29-whole-codebase/sections/10-frontend-compiler-and-host.md).
- [ ] Localize glass-bar status/workspace labels; hardcoded English bypasses the module catalogs. [Audit](../.planning/codebase/audits/2026-10-29-whole-codebase/sections/10-frontend-compiler-and-host.md).
- [ ] Reconcile advanced Settings optimistic edits with authoritative results; pending props, provider and module states currently override later snapshots indefinitely. [Audit](../.planning/codebase/audits/2026-10-29-whole-codebase/sections/10-frontend-compiler-and-host.md).
- [ ] Localize glass-widget weather and idle-media text; hardcoded English bypasses the module catalogs. [Audit](../.planning/codebase/audits/2026-10-29-whole-codebase/sections/10-frontend-compiler-and-host.md).
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

- [ ] Add opt-in recycling/virtualized lists for vertical and horizontal scrolling (`axis`), initially with fixed item sizes.
      Instantiate visible items plus overscan and reuse keyed rows, preserving full scroll extent, item state, focus and accessibility; lists currently instantiate every item.
      Benchmark launcher and Settings resource lists; defer variable-size items and two-axis grids.

### Render sweep findings

From a read-only code sweep on 2026-10-19; nothing below is measured yet.
Verify each against `navigation_hover_profile`, `appearance_frame_cost_profile`
or `gpu_frame_cost_workload` before landing. Roughly in order of payoff.

- [ ] Multi-rect damage replays the selection once per rect when it contains a
      layer scope (`painter/tree.rs:363,596`); paint once under a canvas `clip_region`,
      not the rects' union, or translucent pixels between rects composite twice.
- [ ] A scroll viewport is one opaque replay span, so damage inside it replays
      all content including offscreen rows (`display_list/build.rs:684`).
- [ ] Popover show rebuilds the retained GPU target and dmabuf ring. Keep the
      target while hidden and pool dmabuf slots per size in `GpuPresenter`.
- [ ] GPU target and dmabuf ring reallocate on every size change; round both up
      to 64 px classes like the SHM path and crop with the viewport.
- [ ] Opacity: single-primitive leaves open a `save_layer`; fold alpha into the
      paint instead. Animated opacity/transform layers have no raster cache.
- [ ] The GPU painter keeps CPU-tuned choices (A8 atlas rounded fills, shader
      clips, CPU-rastered shadows/gradients); try analytic `draw_rrect`/
      `clip_rrect` and mask-filter shadows on Ganesh behind a parity test.
- [ ] Skia context: no persistent shader cache; one flush plus fence per
      surface per frame instead of one per frame.
- [ ] Glyph pages re-upload the whole page when they grow; upload only the new cells.
- [ ] Images exceeding the 32 MiB `SKIA_IMAGE_CACHE` budget (including mipmaps)
      are copied and re-uploaded every paint; wrap the decoded `Arc` without copying.
- [ ] Service updates run Lua per message, not per frame.
- [ ] Sparse frames still pay ~10 full passes (batch index rebuild, topology
      check, backdrop regions, layer-scope collection); the subtree reuse map
      drops descendants of reused subtrees (`build.rs:433`) — add a regression.
- [ ] Hit testing visits the whole tree without bounds culling (needs retained
      subtree extents, since children may overflow); hover keys are re-parsed to `NodeId`s.

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

- [ ] Background-layer refraction still samples a static wallpaper image rather than presented frames;
      desktop and top-layer glass use compositor blur plus rim lighting without bending.
      Add a trusted live source and optional compositor glass protocol. Phases 2–4 of the [design](../.planning/todos/pending/2026-10-23-live-glass-backdrop.md).
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
