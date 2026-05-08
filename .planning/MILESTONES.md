# MESH Milestones

## v1.3 Performance Instrumentation and Responsiveness

**Status:** active
**Started:** 2026-05-08

**Goal:** Make shell responsiveness measurable on real interaction paths by adding a debug-only live profiler, a `.mesh`-rendered inspector, canonical benchmark scenarios, and a bounded optimization pass driven by those measurements.

**Planned scope:**
- Profiling runtime model and stage timing hooks
- Per-surface and per-backend-provider/stage attribution
- Debug-overlay profiling mode with live inspector views
- Canonical benchmark flows for hover, open/close, slider drag, keyboard traversal, and backend-driven updates
- At least one demonstrated before/after responsiveness improvement

**Planned phases:** 14-18

**Active artifacts:**
- `.planning/PROJECT.md`
- `.planning/REQUIREMENTS.md`
- `.planning/ROADMAP.md`
- `.planning/STATE.md`
- `.planning/research/v1.3-performance-instrumentation-and-responsiveness.md`

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
