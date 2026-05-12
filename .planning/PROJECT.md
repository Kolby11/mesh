# MESH

## What This Is

MESH is a Rust-based, Wayland-native shell framework that pushes service behavior into Luau plugins while keeping the Rust core focused on generic runtime wiring, state delivery, diagnostics, and frontend rendering. The framework is aimed at authors who want distinctive shell UI and service integrations without giving up deterministic behavior or inspectable performance.

## Core Value

MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.

## Current State

`v1.1` shipped on 2026-05-05.

The project now has a backend plugin MVP with:

- A shell-owned installed-plugin package manifest and normalized plugin graph
- Explicit active-provider selection for backend service categories
- Deterministic backend runtime lifecycle with visible status and diagnostics
- A locked backend Luau MVP API centered on structured `mesh.exec(program, args)`
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

`v1.4` shipped on 2026-05-09.

The project now also has the first retained-rendering pipeline passes with:

- Typed invalidation routed across style, layout, paint, text, and configuration changes
- Incremental style/layout propagation for retained widget identities
- A retained render-object tree synchronized from stable widget nodes
- A retained display list with damage tracking and partial-present plumbing
- Text layout caching, selector indexing, and batching barrier metrics
- Debug inspector metrics showing retained rendering, damage, text cache, and batching behavior

Even with those foundations, the software CPU renderer still feels laggy on real shell surfaces. The next milestone focuses on the remaining CPU-side pipeline bottlenecks before any GPU backend work.

Skia-backed rendering is now a high-priority next-milestone candidate. After `v1.5` finishes the retained CPU pipeline work, `v1.6` should investigate whether Skia can materially improve MESH rendering performance and, if so, migrate the low-level painter behind the existing retained-rendering architecture.

## Current Milestone: v1.5 CPU Rendering Performance Improvement

**Goal:** Use Qt Quick renderer research plus live MESH profiling to identify the remaining CPU rendering bottlenecks, then implement retained pipeline improvements that make shipped shell surfaces feel visibly smoother on the software renderer.

**Target features:**
- Qt Quick scene-graph and performance guidance translated into MESH-specific CPU rendering requirements
- Benchmark-visible attribution for tree build, restyle, layout, render-object sync, retained display-list rebuild, command traversal, text shaping, glyph work, and icon/image raster work
- Viewport- and visibility-aware pruning so offscreen or hidden content stops generating unnecessary CPU paint work
- Incremental retained paint-command updates that avoid recollecting the full surface command set for local changes
- Damage-scoped paint execution that touches only commands relevant to the changed region and overlays
- Hardened SVG, bitmap, icon, text, and glyph caching for repeated CPU paints
- Heuristic tuning and proof on shipped surfaces using the existing canonical benchmark scenarios

## Requirements

### Validated

- `v1.1`: Backend plugin MVP is stable enough to host real service providers and surface diagnostics.
- `v1.2`: The renderer supports practical CSS-like styling, interaction reactivity, selection, keyboard navigation, and animation on shipped shell surfaces.
- `v1.3`: Canonical benchmark scenarios, profiling snapshots, debug inspector views, and retained widget-tree identity/dirty summaries are available for measuring real responsiveness work.
- `v1.4`: The renderer has typed invalidation, retained render objects, retained display data, damage tracking, text caching, selector indexing, and batching metrics on the software path.

### Active

- Make the CPU renderer visibly smoother on real shipped surfaces before investigating any GPU backend milestone.
- Use profiling and benchmark proof to target the highest-cost retained rendering stages instead of optimizing blindly.
- Stop whole-tree retained paint-command rebuilds and whole-command-list scans when only a local region changed.
- Prune offscreen, hidden, or clip-excluded content earlier so the CPU painter does less work.
- Retain more raster work for text, glyphs, SVGs, images, and icons so repeated paints avoid re-decoding or re-rasterizing unchanged assets.
- Prioritize a Skia-backed renderer investigation as the next milestone if v1.5 retained CPU work does not make shipped surfaces feel smooth enough.

### Out of Scope

- GPU backend implementation — the CPU retained pipeline must feel smooth first.
- Parallel paint/layout — ownership and retained CPU data boundaries should be proven before parallel work.
- Broad shell UI redesign — shipped surfaces remain the proof targets.
- A second benchmark system distinct from the canonical debug scenarios — the existing benchmarks remain the acceptance harness.
- Full trace persistence or telemetry export — milestone proof stays inspector- and benchmark-driven.

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
| CPU rendering smoothness comes before GPU work | A laggy software path would hide pipeline inefficiencies and make later GPU work harder to evaluate | Locked for v1.5 |
| Qt research should inform implementation, not force a literal Qt clone | MESH needs the same retained-rendering principles, but applied to its existing Rust software-renderer architecture | Locked for v1.5 |
| Visible smoothness on shipped surfaces outranks microbenchmark-only wins | The user reports real lag everywhere; optimization must improve lived interaction quality, not just synthetic numbers | Locked for v1.5 |
| Skia-backed rendering is the next major performance priority after v1.5 | Skia may provide a faster and more complete low-level 2D paint backend than the current custom/tiny-skia/resvg/cosmic-text/swash software path, but it must be proven against MESH's shipped surfaces before migration | Planned for v1.6 |

<details>
<summary>Archived milestone framing</summary>

## Previous Milestone Framing

### v1.4 Major Performance Fixes

The `v1.4` milestone centered on turning the retained widget-tree foundation from `v1.3` into a retained rendering pipeline: typed invalidation, incremental style/layout propagation, retained render objects, retained display data, damage tracking, text caching, selector indexing, and batching guardrails.

### v1.3 Performance Instrumentation and Responsiveness

The `v1.3` milestone centered on measuring and proving real shell responsiveness through debug-only profiling snapshots, per-surface/backend attribution, a `.mesh` inspector, fixed benchmark scenarios, and the first retained widget-tree foundation.

### v1.2 Rendering System Upgrade

The `v1.2` milestone centered on making MESH rendering expressive enough for distinctive shell UI without turning the renderer into a browser engine. It focused on practical CSS coverage, container and interaction reactivity, text selection and copy, keyboard navigation, animation support, and the navigation bar as the proof surface.

### v1.1 Backend Plugin MVP

The `v1.1` milestone centered on making backend plugins stable enough for MVP by first introducing a unified shell-owned plugin package manifest, then using it to drive backend lifecycle, provider selection, host APIs, service contracts, and diagnostics.

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
*Last updated: 2026-05-10 after starting milestone v1.5*
