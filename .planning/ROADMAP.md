# Roadmap: MESH v1.5 CPU Rendering Performance Improvement

**Status:** Active milestone planning
**Phases:** 26-31
**Total Phases:** 6

## Overview

`v1.5` keeps the focus on the software CPU renderer. The user reports that everything still feels laggy, so this milestone does not broaden into GPU backend work or parallel paint/layout. Instead it uses Qt Quick renderer research and the existing MESH benchmark harness to find and remove the remaining full-tree, full-command-list, and repeat-rasterization bottlenecks on real shell surfaces.

The milestone specifically targets the gaps left after `v1.4`: retained render objects and display data exist, but local changes can still trigger surface-wide command recollection, partial-damage paints still scan the entire command list, and the icon/image/SVG pipeline still repeats expensive raster work. The end goal is a software renderer that feels visibly smoother on the shipped proof surfaces before any later GPU milestone.

## Phases

### Phase 26: CPU Render Profiling and Baseline Proof

**Goal:** Attribute the remaining CPU rendering cost on shipped surfaces and canonical benchmark scenarios before implementation phases begin.
**Depends on:** Phase 25
**Requirements:** `PERF-01`, `PERF-02`

Planned work:

- Attribute CPU render cost across tree build, style restyle, layout, render-object sync, retained display-list rebuild, paint traversal, text shaping, glyph work, and icon/image raster work.
- Record baseline benchmark numbers and visible-smoothness notes for shipped surfaces such as `@mesh/navigation-bar` and `@mesh/audio-popover`.
- Surface the new counters through the existing debug inspector and benchmark payloads rather than building a new benchmark system.
- Document which stages dominate the current “everything is laggy” report so later phases target the right bottlenecks.

### Phase 27: Viewport Culling and Visibility Elision

**Goal:** Prune offscreen, hidden, or clip-excluded work earlier so the CPU renderer stops generating unnecessary paint work.
**Depends on:** Phase 26
**Requirements:** `CULL-01`, `CULL-02`, `CULL-04`

Planned work:

- Introduce viewport- and clip-aware subtree omission for scrollable and clipped content where children are fully outside the visible region.
- Short-circuit render work for nodes hidden by explicit visibility or ineffective opacity rules.
- Replace expensive per-item clipping cases with cheaper omission or elision strategies where possible.
- Keep pruning decisions observable in debug metrics so false positives can be caught early.

### Phase 28: Incremental Paint Command Retention

**Goal:** Stop local retained-tree changes from forcing whole-surface paint-command recollection.
**Depends on:** Phase 26, Phase 27
**Requirements:** `PIPE-01`, `PIPE-02`

Planned work:

- Refactor retained display data so command ownership is tracked per dirty subtree rather than as one surface-wide flat rebuild step.
- Update transform-, scroll-, and reorder-only paths so they preserve unrelated descendant paint data.
- Reduce z-order and command-signature churn for unchanged branches.
- Preserve full-surface fallbacks when dirty summaries are too broad for safe local reuse.

### Phase 29: Damage-Indexed Paint Execution and Repaint Policy

**Goal:** Make partial-damage paints proportional to the changed region instead of total surface complexity.
**Depends on:** Phase 28
**Requirements:** `PIPE-03`, `PIPE-04`, `CULL-03`

Planned work:

- Add a retained mapping from damage regions to affected command ranges, nodes, or buckets.
- Restrict partial paints to commands intersecting the damage region plus required overlays such as tooltips or scrollbars.
- Add a measured repaint-policy switch between minimal damage, bounding-rect repaint, and full-surface repaint.
- Verify ordering, clipping, and correctness when filtered execution skips unrelated commands.

Plans:

- **29-01: Damage-indexed retained paint execution and repaint-policy proof** *(Wave 1, complete 2026-05-11)* — added retained command-span metadata, routed partial paints through ordered filtered command inputs, exposed repaint-policy and filtered-execution counters, and recorded canonical benchmark evidence.
- **29-02: Debug-inspector retained paint counter readability** *(Wave 1, complete 2026-05-12)* — closed the UAT observability gap by rendering retained paint policy, filtered command, skipped command, span, and fallback counters in the shipped debug inspector.

Cross-cutting constraints:

- Preserve retained subtree ownership as the primary damage lookup authority; do not introduce a new global flat command index.
- Preserve display-list ordering, clipping, scrollbar inclusion, and tooltip overlay separation when filtering paint execution.
- Prefer bounding-rect or full-surface fallback whenever dirty summaries, clip/state ancestry, or span selection cannot cheaply prove correctness.
- Publish aggregate proof through the existing `invalidation.paint` debug payload only.

### Phase 30: Raster Cache Hardening for Icons, Images, and Text

**Goal:** Remove repeat rasterization, resize, and parse cost from steady-state CPU painting.
**Depends on:** Phase 26, Phase 28
**Requirements:** `CACHE-01`, `CACHE-02`, `CACHE-03`

Planned work:

- Cache rasterized SVG/icon/image variants by the visual inputs that actually affect the output.
- Eliminate repeated decode/resize churn for unchanged bitmap icons and images.
- Preserve and extend text/glyph cache reuse across hover, animation, scroll, and state-driven updates.
- Retain opaque/translucent metadata so the painter can avoid unnecessary blending and redundant background work.

### Phase 31: Smoothness Proof and CPU Render Tuning

**Goal:** Tune the new CPU rendering heuristics and prove that shipped shell surfaces feel visibly smoother.
**Depends on:** Phase 27, Phase 29, Phase 30
**Requirements:** `PERF-03`, `SMTH-01`, `SMTH-02`, `SMTH-03`

Planned work:

- Tune repaint-policy thresholds, cache behavior, clear/background rules, and culling heuristics using real benchmark data.
- Re-run canonical benchmark scenarios on the shipped proof surfaces and compare against Phase 26 baselines.
- Validate that visuals and interactions remain correct apart from smoother rendering behavior.
- Document what still belongs to future GPU or parallel milestones after the CPU path is improved.

Plans:

- **31-01: Conservative CPU smoothness tuning and shipped-surface proof** *(Wave 1, executed 2026-05-13; verification gaps)* — tuned repaint-policy escalation, added Phase 31 benchmark proof rows, kept cache capacities unchanged based on evidence, and documented deferred live visual UAT for the five canonical shipped-surface scenarios.

Cross-cutting constraints:

- Phase 31 acceptance requires mixed proof: canonical benchmark evidence plus focused manual UAT notes on shipped surfaces.
- Counter-only wins are not accepted unless visible smoothness and interaction correctness hold on `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, and `backend_update`.
- GPU backend work, parallel paint/layout, new benchmark harnesses, trace persistence, and broad shell UI redesign remain out of scope.

## Milestone Boundaries

### Included

- Research-backed CPU render hotspot attribution
- Viewport/visibility pruning for retained rendering
- Dirty-subtree retained paint-command updates
- Damage-indexed CPU paint execution
- Retained raster caches for SVG/icons/images/text where profiling justifies them
- Visible-smoothness proof on shipped shell surfaces

### Excluded

- GPU backend implementation
- Parallel paint/layout implementation
- Broad shell UI redesign
- A second benchmark/profiling system
- Trace persistence or external telemetry export

## Research Basis

This roadmap follows Qt Quick’s retained-rendering guidance rather than browser-engine architecture. The most important lessons carried forward are: retain paint-facing data separately from UI-facing state, make visibility explicit, avoid assuming clipping is a free optimization, keep repeated motion cheap through retention, and treat repaint region selection as a measured policy decision.

Primary research artifacts:

- `.planning/research/STACK.md`
- `.planning/research/FEATURES.md`
- `.planning/research/ARCHITECTURE.md`
- `.planning/research/PITFALLS.md`
- `.planning/research/SUMMARY.md`

Primary external sources:

- https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph-renderer.html
- https://doc.qt.io/qt-6/qtquick-performance.html
- https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html

## Archived Milestones

- `v1.4` Major Performance Fixes — shipped 2026-05-09.
- `v1.3` Performance Instrumentation and Responsiveness — shipped 2026-05-09.
- `v1.2` Rendering System Upgrade — shipped 2026-05-08.
- `v1.1` Backend Plugin MVP — shipped 2026-05-05.

## Backlog and Carryover

- Deferred validation/UAT cleanup from older milestones remains backlog work outside `v1.5`.
- The pending unified package/module manifest phase idea remains future planning work and is not part of CPU rendering optimization.
- Skia-backed rendering is now the high-priority next milestone candidate after `v1.5`: first as a benchmarkable painter/backend spike against the retained display-list command stream, then as a migration only if it clearly improves shipped-surface performance.
- Parallel paint/layout remains sequenced after this milestone proves the CPU retained pipeline is smooth enough.

---
*Roadmap updated: 2026-05-13 after Phase 31 execution*
