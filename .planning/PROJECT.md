# MESH

## What This Is

MESH is a Rust-based, Wayland-native shell framework that pushes service behavior into Luau plugins while keeping the Rust core focused on generic runtime wiring, state delivery, and diagnostics.

## Core Value

MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.

## Current State

`v1.1` shipped on 2026-05-05.

The project now has a backend plugin MVP with:

- A shell-owned installed-plugin package manifest and normalized plugin graph
- Explicit active-provider selection for backend service categories
- Deterministic backend runtime lifecycle with visible status and diagnostics
- A locked backend Luau MVP API: `mesh.exec(program, args)`, `mesh.config()`, `mesh.log(level, msg)`, `mesh.service.emit(...)`, and `mesh.service.set_poll_interval(ms)`
- Generic service state and command routing without service-specific Rust branches
- A reference backend plugin plus automated proof coverage and backend author docs

`v1.2` shipped on 2026-05-08.

The project now also has a rendering-system upgrade with:

- Practical CSS-like styling support with diagnostics, shorthands, token resolution, and author documentation
- Container-size and interaction-state restyling that keeps layout, paint, hit-testing, and accessibility state synchronized
- Selectable rendered text with theme-owned highlight colors and explicit copy routing
- Shell-owned keyboard focus traversal, focus-visible styling, and shortcut handling
- Theme animation tokens and constrained CSS animation playback for supported visual properties
- A richer shipped navigation bar that proves the milestone on a real shell surface

## Current Milestone: v1.3 Performance Instrumentation and Responsiveness

**Goal:** Make shell responsiveness measurable on real shipped interaction paths, expose that data through a debug-only live inspector built on normal `.mesh` UI primitives, and use the results to land a bounded optimization pass in the same milestone.

**Target features:**
- A debug-only profiling mode wired through the existing debug overlay/debug command path
- Live shell-wide profiling snapshots for input, runtime updates, backend work, build, style, layout, paint, present, redraw, and total surface render time
- Per-surface and per-backend-provider/stage attribution instead of aggregate-only timing
- A `.mesh`-rendered inspector with overview, surface, backend, and benchmark views
- A fixed benchmark suite covering hover, surface open/close, slider drag, keyboard traversal, and backend-driven state updates
- At least one demonstrated before/after responsiveness improvement based on the new measurements

## Requirements

### Validated

- `v1.1`: Backend plugin MVP is stable enough to host real service providers and surface diagnostics.
- `v1.2`: The renderer supports practical CSS-like styling, interaction reactivity, selection, keyboard navigation, and animation on shipped shell surfaces.

### Active

- Measure real shell interaction latency and render cost instead of relying on qualitative “feels faster” judgments.
- Keep profiling overhead bounded and disabled unless the debug profiling path is active.
- Attribute backend cost by provider/service stage and frontend cost by surface/stage so hotspots are actionable.
- Prove responsiveness work on real shipped surfaces and canonical interactions rather than synthetic microbenchmarks only.
- Land targeted optimizations in the same milestone and demonstrate at least one measurable improvement.

### Out of Scope

- GPU renderer replacement or renderer architecture rewrite — `v1.3` is about observability and bounded tuning on the current stack.
- Backend architecture redesign — backend work should only change when profiling proves visible responsiveness impact.
- Full trace capture, replay, or external tracing infrastructure — the first profiler is live/rolling and debug-only.
- Broad visual redesign or unrelated surface rewrites — existing shipped surfaces remain the proof targets.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Backend plugins use Luau for service logic | Keeps Rust core as wiring and makes services extensible | Locked |
| Rust core must stay generic across services | Prevents audio/network/power special cases from becoming architecture | Locked |
| Package graph comes before backend lifecycle | A unified installed-plugin interface should drive which backend providers exist and which one is active | Shipped in v1.1 |
| Backend runtime failure does not auto-fallback | Deterministic cleanup and visible status are safer than hidden provider switching | Locked |
| `mesh.exec_shell` is outside the backend MVP host API | Structured argv execution avoids shell parsing ambiguity | Shipped in v1.1 |
| Backend MVP comes before remote distribution and LSP | Runtime stability and local package semantics are prerequisites for tooling and package workflows | Still true |
| MESH renderer should support practical shell CSS, not full web compatibility | Plugin authors need expressive styling without inheriting browser-engine scope | Decided in v1.2 |
| Profiling mode is debug-only and entered through the existing debug path | Instrumentation and inspector UI should help developers without expanding end-user settings surface | Locked for v1.3 |
| The first profiler is live and rolling, not capture/replay | The milestone needs actionable observability with bounded overhead before trace persistence complexity | Locked for v1.3 |
| Responsiveness acceptance is based on fixed benchmark scenarios on shipped surfaces | Prevents vague performance claims and keeps optimization work measurable | Locked for v1.3 |

<details>
<summary>Archived milestone framing</summary>

## Previous Milestone Framing

### v1.2 Rendering System Upgrade

The `v1.2` milestone centered on making MESH rendering expressive enough for distinctive shell UI without turning the renderer into a browser engine. It focused on practical CSS coverage, container and interaction reactivity, text selection and copy, keyboard navigation, animation support, and the navigation bar as the proof surface.

### v1.1 Backend Plugin MVP

The `v1.1` milestone centered on making backend plugins stable enough for MVP by first introducing a unified shell-owned plugin package manifest, then using it to drive backend lifecycle, provider selection, host APIs, service contracts, and diagnostics.

Target features included the central plugin package manifest, frontend-to-backend dependency declaration, backend lifecycle control, the backend host API contract, service provider contracts, runtime diagnostics, and a fresh reference plugin proving the authoring path.

</details>

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `$gsd-transition`):
1. Requirements invalidated? -> Move to Out of Scope with reason
2. Requirements validated? -> Move to Validated with phase reference
3. New requirements emerged? -> Add to Active
4. Decisions to log? -> Add to Key Decisions
5. "What This Is" still accurate? -> Update if drifted

**After each milestone** (via `$gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check -> still the right priority?
3. Audit Out of Scope -> reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-05-08 after starting milestone v1.3*
