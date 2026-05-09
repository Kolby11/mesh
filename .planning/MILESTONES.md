# MESH Milestones

## v1.4 Major Performance Fixes (Shipped: 2026-05-09)

**Phases completed:** 7 phases, 7 plans, 0 tasks

**Key accomplishments:**

- (none recorded)

---

## v1.4 Major Performance Fixes

**Status:** active
**Started:** 2026-05-09

**Goal:** Continue retained-rendering performance work by adding dirty-type invalidation, incremental style/layout propagation, retained render/display data, damage tracking, and batching foundations before GPU backend work.

**Planned scope:**

- Dirty-type invalidation for script/state, style, layout, paint, text, accessibility, metrics, and surface configuration
- Incremental style and layout propagation
- Retained render-object scene graph
- Retained display list and damage tracking
- Text shaping and glyph cache
- Typed slots, interned identifiers, selector indexing, and display-list batching
- GPU and parallel rendering readiness guardrails

**Planned phases:** 19-25

**Active artifacts:**

- `.planning/PROJECT.md`
- `.planning/REQUIREMENTS.md`
- `.planning/ROADMAP.md`
- `.planning/STATE.md`
- `.planning/research/v1.4-major-performance-fixes-qt-retained-rendering.md`

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

## v1.0 Scripting API Stabilization

**Status:** archived by reset
**Archived:** 2026-05-03
**Reason:** User reset active roadmap to focus on backend plugin MVP fundamentals before continuing frontend documentation or distribution work.

**Archive artifacts:**

- `.planning/milestones/v1.0-reset-2026-05-03-ROADMAP.md`
- `.planning/milestones/v1.0-reset-2026-05-03-REQUIREMENTS.md`
- `.planning/milestones/v1.0-reset-2026-05-03-STATE.md`
- `.planning/milestones/v1.0-reset-2026-05-03-phases/`

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
