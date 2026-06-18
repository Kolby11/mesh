# Project Retrospective

*A living document updated after each milestone. Lessons feed forward into future planning.*

## Milestone: v1.5 - CPU Rendering Performance Improvement

**Shipped:** 2026-05-13
**Phases:** 6 | **Plans:** 10 | **Sessions:** multiple live implementation and UAT sessions

### What Was Built

- CPU render profiling attribution for canonical shipped-surface scenarios.
- Viewport, visibility, and clip-aware retained paint pruning.
- Incremental retained paint-command updates and damage-indexed paint execution.
- Raster cache hardening for SVG, bitmap, icon, text, and glyph paths.
- Repaint-policy tuning and shipped-surface smoothness proof, including live audio popover UAT fixes.

### What Worked

- Canonical benchmark scenarios kept performance claims tied to reproducible shell interactions.
- Retained rendering boundaries from v1.4 gave later phases clean ownership for culling, command retention, and damage filtering.
- Live UAT exposed interaction regressions that benchmark counters alone would not have caught.

### What Was Inefficient

- Some live audio-surface behavior required several retest/fix loops after the initial smoothness proof.
- Phase 26 and Phase 30 passed verification but did not leave `VALIDATION.md` artifacts, creating archive-time metadata debt.

### Patterns Established

- Treat visible smoothness as a joint benchmark plus live-UAT acceptance condition.
- Keep repaint-policy thresholds conservative unless shipped-surface proof shows a clear reason to widen them.
- Record deferred polish as explicit pending todo files before milestone archive.

### Key Lessons

1. Performance wins are not complete until shipped controls still feel correct under immediate pointer and backend updates.
2. Stateful popovers need a single source of truth for hover, focus, command, and backend reconciliation paths.
3. Future renderer migrations should consume the retained display-list pipeline rather than bypass it.

### Cost Observations

- Model mix: not tracked.
- Sessions: multiple.
- Notable: the most expensive work was not raw implementation, but live interaction convergence around the audio popover.

---

## Milestone: v1.8 - Rendering Engine Architecture

**Shipped:** 2026-05-18
**Phases:** 4 | **Plans:** 14 | **Sessions:** multiple planning, research, implementation, and verification sessions

### What Was Built

- Source-backed Blitz adopt-vs-build decision matrix with explicit candidate crate outcomes.
- Comparable renderer prototype evidence for Blitz reference and MESH-owned focused-crate paths.
- Retained MESH-shaped focused evidence using Taffy, Parley, AnyRender-style paint, and AccessKit-compatible boundaries.
- Production-adjacent focused proof integration behind current renderer and shell ownership.
- Phased renderer migration roadmap, ownership classification, and author-facing `.mesh` renderer contract.

### What Worked

- Running decision, prototype, integration, and migration-contract phases in sequence kept the renderer choice grounded in evidence.
- The shared navigation/audio slice made Blitz and focused-crate prototype results comparable instead of abstract.
- Treating focused proof snapshots as adapter-owned evidence avoided accidentally exposing migration internals as author API.

### What Was Inefficient

- The milestone close still surfaced old Phase 31 debug/todo artifacts, so cross-milestone deferred work needs periodic cleanup.
- The codebase drift gate hit a non-blocking SDK Node `EPERM`, which reduced confidence in that advisory signal.
- The archive SDK created useful archive files but still needed manual ROADMAP, PROJECT, and milestone-entry cleanup.

### Patterns Established

- Renderer candidates must preserve or replace retained `NodeId`, typed invalidation, damage, profiling, diagnostics, theme-owned selection, and AccessKit-compatible evidence before promotion.
- Broad renderer migration needs feature flags or local bypasses, rollback paths, Linux/Nix impact notes, binary/build risk notes, and exact CI gates.
- `.mesh` remains a bounded shell UI authoring surface, not browser DOM/HTML/CSS compatibility.

### Key Lessons

1. Architecture choices should move from source-backed matrix to comparable prototype to constrained production proof before migration planning.
2. A concrete blocker is acceptable prototype evidence when it is reproducible and compared under the same criteria.
3. Migration contracts should name non-goals as explicitly as guarantees, especially when browser-engine crates are involved.

### Cost Observations

- Model mix: not tracked.
- Sessions: multiple.
- Notable: the broadest command cost was the full workspace regression at Phase 45 closeout; it stayed green after docs-only migration planning.

---

## Milestone: v1.7 - Rethink Modularity and Extensibility Concepts

**Shipped:** 2026-05-18
**Phases:** 5 | **Plans:** 17 | **Sessions:** multiple planning, implementation, review, and verification sessions

### What Was Built

- Canonical module vocabulary and `module.json` manifest normalization.
- Typed installed-graph contribution records for frontend, resources, keybinds, interfaces, providers, settings, and libraries.
- Interface/provider/resource validation that keeps frontend requirements, backend provider identity, and host capabilities separate.
- Author-facing migration diagnostics and docs for legacy manifest names and canonical module workflows.
- Shipped navigation/audio proof that exercises canonical manifests and installed graph behavior without service-specific Rust branches.

### What Worked

- The vocabulary-first phase reduced ambiguity before manifest, graph, diagnostic, and proof work.
- Keeping old public names as replacement debt prevented compatibility paths from becoming renewed public API.
- The real navigation/audio proof forced docs, tests, and runtime behavior to converge on one workflow.

### What Was Inefficient

- v1.7 closed without a milestone audit artifact, so the archive records accepted audit debt.
- Several open artifacts remained from earlier milestones and had to be explicitly deferred at close.
- MILESTONES.md still contained stale planned v1.7 framing that needed cleanup after the SDK archive.

### Patterns Established

- Treat installed-graph records as the inspectable boundary for extension behavior.
- Preserve manifest-owned declarations as canonical data; settings should override effective behavior, not become declaration sources.
- Migration diagnostics should be author-facing and concrete, while temporary loaders remain internal implementation details.

### Key Lessons

1. Cross-cutting terminology work should be completed before adding more extension features.
2. Real shipped proof modules are better acceptance tests than isolated manifest fixtures alone.
3. Future renderer work should include an explicit adopt-vs-build decision before implementation phases.

### Cost Observations

- Model mix: not tracked.
- Sessions: multiple.
- Notable: the main cost was architectural reconciliation across previous milestone decisions.

---

## Milestone: v1.11 - Surface Keybind Completion

**Shipped:** 2026-05-23
**Phases:** 5 | **Plans:** 5 | **Sessions:** autonomous planning, implementation, audit, and closeout

### What Was Built

- Manifest-owned surface keybind actions now dispatch through focused-surface runtime subscribers.
- Resolution precedence is locked across user overrides, exact locale, parent locale, generic defaults, and no-binding fallback.
- Invalid, conflicting, unresolved, and unsafe keybinds produce non-fatal component diagnostics.
- Accessibility annotations and structured `mesh.debug.keybinds` payloads expose resolved focused-surface keybind metadata.
- Navigation and audio surfaces now prove the completed keybind system, including audio-popover mute access-key dispatch.

### What Worked

- Resuming the paused v1.6 work after canonical module records existed kept declarations, overrides, diagnostics, and dispatch aligned to one manifest-owned source.
- Focused shell regression tests made precedence rules concrete: shell-global shortcuts, text input, selection copy, focus traversal, and default activation remained protected.
- Proving the final behavior on shipped navigation/audio surfaces prevented the feature from staying fixture-only.

### What Was Inefficient

- Older open debug/todo artifacts still surfaced at milestone close, so deferred artifact cleanup remains a recurring closeout cost.
- The keybind work needed several small focused tests because dispatch, resolution, diagnostics, accessibility, and real-surface proof all touched the same keyboard path.
- Existing validation metadata still has partial Nyquist frontmatter even where validation artifacts exist.

### Patterns Established

- Surface keybinds are manifest-owned declarations; settings remain override-only and cannot create new action ids.
- Author/runtime keybind mistakes should degrade component health and emit diagnostics rather than crashing or stealing input.
- Debug metadata should mirror resolved runtime state while accessibility stays attached to subscribed controls.

### Key Lessons

1. Keyboard features need explicit ownership ordering before implementation; otherwise text input and shell-global shortcuts become accidental regressions.
2. Localized access keys are useful, but shortcut localization should stay conservative unless a user override explicitly asks for it.
3. Real bundled surfaces are the right proof target for module-author contracts because they exercise manifests, handlers, docs, accessibility, and debug payloads together.

### Cost Observations

- Model mix: not tracked.
- Sessions: one autonomous closeout sequence plus earlier phase execution sessions.
- Notable: the final proof phase had low code churn because earlier phases established clear declaration, resolution, and diagnostic boundaries.

---

## Milestone: v1.13 - Manifest I18n Contract

**Shipped:** 2026-05-24
**Phases:** 4 | **Plans:** 4 | **Sessions:** autonomous planning, implementation, verification, and closeout

### What Was Built

- Reusable localized text manifest values for keybind display metadata, with fallback compatibility and canonical loader migration diagnostics.
- Installed graph records preserve localized keybind and layout metadata while keeping fallback accessors for current consumers.
- Shell runtime metadata resolves localized manifest keybind text against the active locale and keeps source keys for debugging.
- The shipped navigation manifest uses the explicit localized text contract and proves parser, graph, runtime, debug, and docs behavior on the real module path.

### What Worked

- The milestone split was clean: parser contract, graph preservation, runtime resolution, and shipped proof each had a narrow boundary.
- Synthetic tests caught fallback and diagnostic behavior before shipped fixture tests proved real navigation catalog behavior.
- Keeping raw strings as literals preserved compatibility while still giving authors a diagnosable explicit localization path.

### What Was Inefficient

- The milestone closed without a dedicated `v1.13-MILESTONE-AUDIT.md`; closeout relied on completed phase verification and roadmap/requirements state.
- Older open debug/todo artifacts still surfaced at close and were deferred again.
- The SDK archive copied the full roadmap, so ROADMAP/PROJECT cleanup still required manual interpretation.

### Patterns Established

- Manifest text fields that can be localized should carry structured source metadata until the shell resolves them at the consumer boundary.
- Runtime-facing metadata should expose resolved user text as the primary field and retain source keys as additive debug metadata.
- Shipped module proof should assert both manifest/catalog authoring and runtime/debug resolution behavior.

### Key Lessons

1. Field-local localization objects are clearer than string conventions because they carry both intent and fallback.
2. Locale behavior should be proven at the real runtime metadata boundary, not only in manifest parser tests.
3. Author docs should show `mesh.i18n`, `mesh.contributes.i18n`, and field-local text declarations together so catalog ownership is explicit.

### Cost Observations

- Model mix: not tracked.
- Sessions: one autonomous milestone completion sequence plus earlier phase execution.
- Notable: the most valuable regression was the real navigation debug metadata test because it exercised shipped catalogs and runtime recreation together.

---

## Milestone: v1.20 — Compositor Integration

**Shipped:** 2026-06-18
**Phases:** 4 (101, 102, 103, 103.1 inserted) | **Plans:** 7 | **Commits:** 82

### What Was Built

- Per-region `Vec<DamageRect>` through present path with 16-rect cap and union fallback — compositor receives per-dirty-rect damage instead of full surface
- HiDPI / fractional scale: authoritative `scale: f32` on `SurfaceEntry`; physical `PixelBuffer` allocation; `wp_viewporter` for fractional ratios; damage rect logical-to-physical scaling at single attachment boundary
- KDE compositor blur offload: optional `org_kde_kwin_blur` global; `set_region`+`commit` before `wl_surface.commit`; correct clear-on-removal behavior via `blur_committed` guard
- CPU software blur removed; `backdrop-filter` data still flows for compositor protocol use
- Phase 103.1 (inserted): closed three audit gaps before archiving — blur never-cleared bug, negative coord saturation, `damage_rect_count` path verification

### What Worked

- Phase sequencing (damage → HiDPI → blur) made each phase self-contained: damage established the per-rect pipeline, HiDPI established the physical coordinate system, blur used both.
- The milestone audit caught real issues (CR-01 blur-never-cleared was a genuine correctness bug) and created a compact closure phase rather than deferring them.
- Threat model clamping (1..=3 integer, 60..=480 fractional) applied at protocol event boundaries stopped malicious compositor values before they could affect allocation math.
- Extracting `protocol_damage_rects` as a pure function made the 16-rect cap independently testable without Wayland event infrastructure.

### What Was Inefficient

- Phase 103 was marked complete without a VERIFICATION.md or VALIDATION.md — the audit had to catch this and Phase 103.1 was needed to produce them, adding overhead that a closure step in the original plan would have avoided.
- The audit file's `gaps_found` status was preserved as stale after Phase 103.1 closed all gaps — future milestone completion should update the audit status rather than leaving the original file immutable.
- Some REQUIREMENTS.md checkboxes were not updated as verification evidence was produced, requiring a manual pass at archive time.

### Patterns Established

- Audit-inserted phases (e.g., 103.1) are a first-class mechanism for closing gaps before archiving; they keep the original phase summaries clean while recording gap closure explicitly.
- Verification and validation artifacts should be part of each plan's done criteria, not post-hoc additions.
- For Wayland protocol work: bind as `Option` + guard at call sites is the correct pattern for optional protocol support — non-KDE compositors produce zero protocol errors.

### Key Lessons

1. Protocol work has discrete completion levels: bind, wire data, handle all state transitions (including clear/remove paths). Only the third level is "done" — CR-01 showed that the `None` branch was systematically skipped.
2. Verification artifacts should be produced before a phase is declared complete, not discovered missing at milestone audit.
3. Inserting a closure phase after audit gaps are identified is faster and cleaner than leaving gaps as tech debt in the next milestone.

### Cost Observations

- Model mix: not tracked.
- Sessions: audit session plus Phase 103.1 gap closure, prior phase execution sessions.
- Notable: Phase 103.1 closure was fast (~25 min) because the audit already enumerated exactly what to fix; well-structured audit output pays off at closure.

---

## Cross-Milestone Trends

### Process Evolution

| Milestone | Sessions | Phases | Key Change |
|-----------|----------|--------|------------|
| v1.13 | autonomous plus earlier implementation sessions | 4 | Manifest i18n acceptance moved through parser contract, graph preservation, runtime resolution, and shipped-module proof. |
| v1.11 | autonomous plus earlier implementation sessions | 5 | Surface keybind acceptance moved through dispatch, deterministic resolution, diagnostics, accessibility/debug metadata, and shipped-surface proof. |
| v1.8 | multiple | 4 | Renderer architecture acceptance moved through decision matrix, comparable prototypes, focused production proof, and migration contract. |
| v1.7 | multiple | 5 | Module extensibility acceptance moved through vocabulary, manifest, graph, diagnostics, and shipped proof in one milestone. |
| v1.5 | multiple | 6 | Performance acceptance moved from counters-only proof to benchmark plus live-UAT proof. |

### Cumulative Quality

| Milestone | Tests | Coverage | Zero-Dep Additions |
|-----------|-------|----------|-------------------|
| v1.13 | Focused manifest parser, installed graph, runtime descriptor/debug metadata, shipped navigation catalog, docs grep, and shell compile checks | Requirements 16/16 | None identified |
| v1.11 | Focused shell keybind, diagnostics, debug payload, locale, navigation, and audio-popover regression suites | Requirements 19/19 | None identified |
| v1.8 | Prototype cargo checks, focused renderer proof tests, shell navigation/audio regressions, workspace test, and docs grep verification | Requirements 13/13 | None identified |
| v1.7 | Focused Rust manifest, graph, shell, diagnostic, and docs proof tests | Requirements 13/13 | None identified |
| v1.5 | Focused Rust and `.mesh` regression tests plus live UAT | Requirements 17/17 | None identified |

### Top Lessons (Verified Across Milestones)

1. Keep service-specific behavior out of Rust core while still testing shipped proof surfaces end to end.
2. Retained renderer work needs stable debug payloads so optimizations remain observable after each phase.
3. Canonical vocabulary and manifest contracts should be locked before expanding plugin-facing runtime behavior.
4. Renderer migration work should keep author contracts, rollback gates, and ownership boundaries explicit before broad adoption.
5. Focused keyboard behavior needs real-surface proof because precedence mistakes can be invisible in declaration-only tests.
6. Localized manifest text needs source metadata preservation until runtime consumers can resolve against active locale state.
