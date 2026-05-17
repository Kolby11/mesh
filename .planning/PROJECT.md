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

`v1.5` shipped on 2026-05-13.

The project now also has CPU rendering performance improvements with:

- CPU render cost attribution across the canonical shipped-surface benchmark scenarios
- Viewport, visibility, and clip-aware pruning for retained paint work
- Incremental retained paint-command updates for dirty subtrees
- Damage-indexed paint execution with measured repaint-policy thresholds
- Hardened SVG, bitmap, icon, text, and glyph cache behavior on the software path
- Smoothness proof on shipped shell surfaces, including focused live UAT on the navigation bar and audio popover

The v1.5 milestone closed the visible interaction regressions found during live UAT. One slight audio popover transition delay remains accepted polish debt by user request.

The v1.6 keybind milestone established semantic keybind declarations and locale-aware trigger resolution through phases 32 and 33. It was paused before script dispatch, conflict diagnostics, and accessibility proof so v1.7 can consolidate the broader module and extension model those features depend on.

Phase 37 of v1.7 is complete. MESH now has a canonical module vocabulary, a
hard replacement rule for old public names, author docs aligned to
`module.json`, and a runtime/future-phase inventory for manifest
normalization, contribution indexing, migration diagnostics, and shipped proof.

Phase 38 of v1.7 is complete. The runtime now treats canonical `module.json`
as the target manifest contract, routes old manifest forms through explicit
internal migration diagnostics, and loads the checked-in root module graph from
`config/module.json` while preserving active provider and keybind data.

## Current Milestone: v1.7 Rethink Modularity and Extensibility Concepts

**Goal:** Rework MESH's modularity model so frontend modules, backend providers, manifests, service contracts, capabilities, and extension points form a coherent author-facing architecture instead of a set of separate milestone-grown mechanisms.

**Target features:**
- Clarified vocabulary and boundaries for module, frontend surface, backend provider, interface contract, library, resource pack, and contribution.
- A canonical `module.json` manifest schema that unifies module identity, dependencies, capabilities, entrypoints, contributions, interface declarations, provider declarations, keybinds, assets, settings, and migration metadata.
- Extensibility contracts that let third-party modules add new interfaces, providers, UI entrypoints, resources, and libraries without service-specific Rust branches.
- Compatibility and migration handling for existing package graph, legacy manifests, v1.1 backend provider declarations, and v1.6 keybind declarations.
- Author-facing proof through docs, diagnostics, validation, and at least one real module/provider path.

## Requirements

### Validated

- `v1.1`: Backend plugin MVP is stable enough to host real service providers and surface diagnostics.
- `v1.2`: The renderer supports practical CSS-like styling, interaction reactivity, selection, keyboard navigation, and animation on shipped shell surfaces.
- `v1.3`: Canonical benchmark scenarios, profiling snapshots, debug inspector views, and retained widget-tree identity/dirty summaries are available for measuring real responsiveness work.
- `v1.4`: The renderer has typed invalidation, retained render objects, retained display data, damage tracking, text caching, selector indexing, and batching metrics on the software path.
- `v1.5`: The CPU renderer has profiling attribution, visibility pruning, incremental retained paint-command updates, damage-indexed paint execution, raster cache hardening, and shipped-surface smoothness proof.
- `v1.7 Phase 37`: The canonical module vocabulary is locked: old public names are replacement debt, temporary old loaders are internal migration details, and v1.1 provider plus v1.6 keybind decisions are reconciled into the module/interface/provider/contribution model.
- `v1.7 Phase 38`: Canonical `module.json` normalization is implemented in Rust, old manifest forms produce explicit migration diagnostics, checked-in root/module fixtures use canonical paths, and v1.1 provider plus v1.6 keybind data survive normalization.

### Active

- Module authors can rely on one coherent module and manifest model for frontend, backend, interface, library, theme, icon, font, and language modules.
- Interface contracts, provider implementations, dependency declarations, capabilities, settings, keybinds, assets, and UI contributions use consistent vocabulary and validation paths.
- Third-party modules can extend MESH by adding interfaces, providers, libraries, resources, and UI entrypoints without service-specific Rust branches.
- Existing v1.1 backend provider behavior and v1.6 keybind declaration/resolution behavior remain preserved through explicit migration diagnostics.

### Out of Scope

- Compositor-global shortcuts via XDG desktop portals or compositor-specific APIs — module/surface-scoped keybinds come first.
- Broad shell UI redesign, marketplace/distribution service work, remote package signing, or installer UX.
- Compositor-global shortcuts via XDG desktop portals or compositor-specific APIs.
- Replacing keyboard focus traversal, text-input behavior, or shipped widget activation semantics.
- Skia-backed rendering investigation — still a future rendering backlog candidate, but not the active v1.7 scope.
- Finishing all paused v1.6 keybind runtime behavior; this milestone only preserves and migrates the declaration/resolution model where it intersects modularity.

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
| v1.5 can ship with the slight audio popover transition delay deferred | Functional audio popover interaction, slider behavior, and mute state now pass; the user explicitly asked to keep the remaining delay polish for later | Accepted at v1.5 archive |
| Module keybind management takes priority for v1.6 | Plugin authors need declared, localizable keyboard actions before expanding to compositor-global shortcut plumbing or more rendering work | Active for v1.6 |
| Skia-backed rendering is deferred beyond v1.6 | The next user-requested capability is frontend module keybind management; Skia remains a future rendering investigation candidate | Deferred |
| v1.6 keybind work is paused after phases 32 and 33 | The next user request is to rethink modularity and extensibility concepts, and the remaining keybind phases depend on the broader module contract vocabulary | Paused |
| v1.7 prioritizes conceptual coherence over new feature breadth | MESH now has manifests, providers, interfaces, capabilities, resources, keybinds, docs, and diagnostics grown across multiple milestones; consolidating those contracts reduces future extension friction | Active for v1.7 |

<details>
<summary>Archived milestone framing</summary>

## Previous Milestone Framing

### v1.5 CPU Rendering Performance Improvement

The `v1.5` milestone centered on removing remaining CPU-side retained-rendering bottlenecks before any GPU backend work: profiling attribution, visibility pruning, incremental retained paint commands, damage-indexed paint execution, raster cache hardening, and shipped-surface smoothness proof.

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
*Last updated: 2026-05-17 after completing Phase 37*
