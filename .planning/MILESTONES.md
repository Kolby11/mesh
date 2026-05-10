# MESH Milestones

## v1.5 CPU Rendering Performance Improvement

**Status:** active
**Started:** 2026-05-10

**Goal:** Research Qt Quick renderer patterns and current MESH CPU bottlenecks, then implement the retained pipeline, culling, and caching improvements needed to make shipped shell surfaces feel visibly smoother on the software renderer.

**Planned scope:**

- Attribute CPU render cost across canonical benchmark scenarios and shipped proof surfaces
- Prune offscreen, hidden, and clip-excluded content earlier in the retained paint pipeline
- Replace whole-tree retained paint-command recollection with dirty-subtree updates
- Restrict partial-damage paints to commands that actually intersect the changed region
- Cache expensive SVG, bitmap, icon, text, and glyph raster work more aggressively
- Tune repaint heuristics for visible smoothness while keeping GPU and parallel rendering out of scope

**Planned phases:** 26-31

**Active artifacts:**

- `.planning/PROJECT.md`
- `.planning/REQUIREMENTS.md`
- `.planning/ROADMAP.md`
- `.planning/STATE.md`
- `.planning/research/STACK.md`
- `.planning/research/FEATURES.md`
- `.planning/research/ARCHITECTURE.md`
- `.planning/research/PITFALLS.md`
- `.planning/research/SUMMARY.md`

## v1.4 Major Performance Fixes

**Status:** shipped
**Started:** 2026-05-09
**Archived:** 2026-05-09

**Goal:** Continue retained-rendering performance work by adding dirty-type invalidation, incremental style/layout propagation, retained render/display data, damage tracking, and batching foundations before GPU backend work.

**Delivered:**

- Typed invalidation for style, layout, paint, text, accessibility, metrics, and surface configuration changes
- Incremental style/layout propagation over retained widget identity
- Retained render objects synchronized from stable widget nodes
- Retained display list plus damage tracking and partial-present metrics
- Text layout caching, selector indexing, typed slots, and batching barrier counters
- GPU-readiness guardrails without implementing a GPU backend

**Stats:** 7 phases, 7 plans, retained CPU rendering foundations shipped

**Archive artifacts:**

- `.planning/milestones/v1.4-phases/`

## v1.3 Performance Instrumentation and Responsiveness

**Status:** shipped
**Started:** 2026-05-08
**Archived:** 2026-05-09

**Goal:** Make shell responsiveness measurable on real interaction paths by adding a debug-only live profiler, a `.mesh`-rendered inspector, canonical benchmark scenarios, and a bounded optimization pass driven by those measurements.

**Delivered:**

- Profiling runtime model and stage timing hooks
- Per-surface and per-backend-provider/stage attribution
- Debug-overlay profiling mode with live inspector views
- Canonical benchmark flows for hover, open/close, slider drag, keyboard traversal, and backend-driven updates
- Retained widget-tree foundation with stable node identity and dirty summaries

**Stats:** 5 phases, 16 plans, retained widget-tree foundation shipped

**Archive artifacts:**

- `.planning/milestones/v1.3-ROADMAP.md`
- `.planning/milestones/v1.3-REQUIREMENTS.md`

## v1.2 Rendering System Upgrade

**Status:** shipped
**Started:** 2026-05-05
**Archived:** 2026-05-08

**Goal:** Make MESH frontend rendering expressive and interactive enough for distinctive shell UI without turning the renderer into a full browser engine.

**Delivered:**

- Practical CSS coverage with diagnostics, shorthands, token resolution, and author docs
- Container-size and interaction-state reactivity without losing runtime or service state
- Selectable rendered text with copy support and theme-owned highlight rendering
- Keyboard navigation, focus-visible behavior, and configurable shortcuts on shipped surfaces
- Theme animation tokens and constrained CSS animation playback with diagnostics
- Navigation-bar migration as the milestone proof surface

**Stats:** 6 phases, 26 plans, 26 requirements shipped

**Deferred at close:** 4 acknowledged items recorded in `.planning/STATE.md`

**Archive artifacts:**

- `.planning/milestones/v1.2-ROADMAP.md`
- `.planning/milestones/v1.2-REQUIREMENTS.md`

## v1.1 Backend Plugin MVP

**Status:** shipped
**Started:** 2026-05-03
**Archived:** 2026-05-05

**Goal:** Make backend plugins stable enough for MVP: core backend concepts work predictably, plugins can run service logic, emit state, receive config, log, execute host commands, expose service contracts, and surface failures clearly.

**Delivered:**

- Shell-owned installed-plugin package manifest and normalized plugin graph
- Deterministic backend runtime lifecycle with explicit active-provider ownership
- Locked backend Luau MVP host API centered on structured `mesh.exec(program, args)`
- Generic service state publication and command routing
- Backend diagnostics hardening, reference backend proof plugin, and author docs
- Milestone planning reconciliation so the archive matches the shipped contract

**Stats:** 6 phases, 21 plans, 63 tasks, 28 requirements shipped

**Deferred at close:** 3 validation/verification cleanup items recorded in `.planning/STATE.md`

**Archive artifacts:**

- `.planning/milestones/v1.1-ROADMAP.md`
- `.planning/milestones/v1.1-REQUIREMENTS.md`

## v1.0 Scripting API Stabilization

**Status:** archived by reset
**Archived:** 2026-05-03
**Reason:** User reset active roadmap to focus on backend plugin MVP fundamentals before continuing frontend documentation or distribution work.

**Archive artifacts:**

- `.planning/milestones/v1.0-reset-2026-05-03-ROADMAP.md`
- `.planning/milestones/v1.0-reset-2026-05-03-REQUIREMENTS.md`
- `.planning/milestones/v1.0-reset-2026-05-03-STATE.md`
- `.planning/milestones/v1.0-reset-2026-05-03-phases/`
