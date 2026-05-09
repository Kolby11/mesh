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

`v1.3` shipped on 2026-05-09.

The project now also has performance instrumentation and benchmark coverage with:

- Debug-only profiling snapshots for shell stage timing
- Per-surface and backend-stage attribution
- A `.mesh`-rendered debug inspector
- Canonical benchmark scenarios for hover, surface open/close, slider drag, keyboard traversal, and backend-driven updates
- A retained widget-tree foundation with stable `_mesh_key` runtime node IDs, retained style-only mutation, and widget-layer dirty summaries

## Current Milestone: v1.4 Major Performance Fixes

**Goal:** Continue the retained-rendering performance work beyond the completed widget-layer foundation by adding dirty-type invalidation, incremental style/layout propagation, retained paint data, and damage tracking before any GPU backend work.

**Target features:**
- Dirty-type invalidation for script/state, style, layout, paint, text, accessibility, metrics, and surface configuration
- Incremental style propagation over stable retained widget identities
- Incremental layout consumers that avoid full layout work when changes are scoped
- A retained render-object/scene-graph layer separate from the widget tree
- Retained display list and damage tracking so unchanged regions avoid unnecessary repaint/present work
- Text shaping and glyph cache work sequenced after retained paint/damage foundations
- Typed attribute/style slots, interned identifiers, and selector indexing to reduce repeated lookup and restyle cost
- Display-list batching groundwork while preserving the rule that GPU backend work waits until retained rendering and damage tracking are in place

## Requirements

### Validated

- `v1.1`: Backend plugin MVP is stable enough to host real service providers and surface diagnostics.
- `v1.2`: The renderer supports practical CSS-like styling, interaction reactivity, selection, keyboard navigation, and animation on shipped shell surfaces.
- `v1.3`: Canonical benchmark scenarios, profiling snapshots, debug inspector views, and retained widget-tree identity/dirty summaries are available for measuring real responsiveness work.

### Active

- Use Qt Quick-style retained rendering principles: separate item/widget state from retained render nodes, synchronize changed items into render nodes, retain geometry/resources, and batch renderable primitives.
- Extend widget-layer dirty summaries into typed invalidation that downstream style, layout, paint, text, accessibility, metrics, and surface configuration stages can consume.
- Avoid full tree rebuild/layout/paint work when stable node IDs and dirty types prove the change is scoped.
- Establish damage tracking and retained display data before pursuing GPU backend work.
- Keep performance proof tied to the existing debug inspector and canonical benchmark scenarios.

### Out of Scope

- Implementing the GPU backend itself — `v1.4` prepares the retained rendering and damage/batching foundations first.
- Parallel paint/layout — this waits until ownership boundaries and retained render data remove the current bottlenecks.
- Broad shell UI redesign — existing shipped surfaces and benchmarks remain the proof targets.
- Full trace capture/replay or external telemetry — profiling remains live/debug-oriented unless needed to prove this milestone.

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
| Retained rendering should follow Qt-style scene graph principles more than browser-engine architecture | MESH is a shell UI toolkit with controlled primitives, so item-to-render-node synchronization, retained geometry, dirty regions, and batching fit better than full browser pipeline complexity | Locked for v1.4 |
| GPU backend work waits until retained rendering, dirty invalidation, retained display data, and damage tracking exist | Uploading brand-new paint data every frame would waste much of the GPU benefit | Locked for v1.4 |

<details>
<summary>Archived milestone framing</summary>

## Previous Milestone Framing

### v1.2 Rendering System Upgrade

The `v1.2` milestone centered on making MESH rendering expressive enough for distinctive shell UI without turning the renderer into a browser engine. It focused on practical CSS coverage, container and interaction reactivity, text selection and copy, keyboard navigation, animation support, and the navigation bar as the proof surface.

### v1.3 Performance Instrumentation and Responsiveness

The `v1.3` milestone centered on measuring and proving real shell responsiveness through debug-only profiling snapshots, per-surface/backend attribution, a `.mesh` inspector, fixed benchmark scenarios, and the first retained widget-tree foundation.

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
*Last updated: 2026-05-09 after starting milestone v1.4*
