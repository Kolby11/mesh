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

Phase 39 of v1.7 is complete. Extension points are now inspectable through
typed installed-graph contribution records, explicit interface relationship
metadata, backend-only provider indexing, graph-driven shell interface/provider
registration, non-fatal resource/settings diagnostics, and manifest-driven
tests that prove new interface/provider/library/resource behavior without
service-specific Rust branches.

Phase 40 of v1.7 is complete. Legacy manifest names now produce explicit
author-facing migration diagnostics and documentation guidance, canonical
`module.json` examples are reinforced across docs, and paused v1.6 keybind
declaration/resolution data is preserved through typed contribution records,
manifest-first shortcut resolution, modifier validation, and settings override
boundaries.

`v1.7` shipped on 2026-05-18.

The project now also has a consolidated module and extensibility model with:

- Locked canonical vocabulary for modules, frontend surfaces, backend providers, interfaces, libraries, resource packs, contributions, capabilities, and dependencies
- Canonical `module.json` normalization with explicit legacy manifest migration diagnostics
- Typed installed-graph contribution records for frontend, resource, keybind, interface, provider, settings, and library data
- Graph-driven provider registration, active backend selection, frontend filtering, and navigation proof without service-specific production branches
- Author docs that teach the shipped navigation/audio path as the canonical workflow for extending or adding a MESH module

`v1.8` shipped on 2026-05-18.

The project now also has a rendering engine architecture direction with:

- A source-backed adopt-vs-build decision that keeps direct Blitz adoption blocked and selects a MESH-owned focused-crate path.
- Comparable prototype evidence across the shared navigation/audio shipped-surface slice.
- A production-adjacent focused proof adapter preserving retained identity, typed invalidation, damage/profiling, diagnostics, selection, and AccessKit-compatible boundaries.
- A phased, reversible renderer migration roadmap with explicit ownership classification, build/CI/Linux/Nix/binary-risk gates, rollback expectations, and an author-facing `.mesh` renderer contract.

## Current Milestone: v1.9 Renderer Library Integration

**Goal:** Move the selected renderer libraries from prototype/proof evidence into production renderer paths behind reversible adapter boundaries, while preserving retained MESH identity, diagnostics, profiling, accessibility, and shipped navigation/audio behavior.

**Target features:**
- Add production Cargo dependencies and adapter ownership for Taffy layout, Parley text, AnyRender/Vello-style paint boundaries, and AccessKit runtime updates.
- Replace proof-only `taffy_layout`, `parley_text`, and `accesskit_node_id` evidence with real library-backed adapters where each library is ready.
- Keep the current MESH software renderer, retained tree, render-object tree, display list, and Wayland presentation as rollback authorities until each replacement path passes gates.
- Verify shipped navigation/audio surfaces, selection behavior, damage/profiling payloads, diagnostics, binary/build impact, and Linux/Nix implications before any broad defaulting.

**Next milestone direction:** v1.10 should focus on animations and motion fidelity after renderer library integration gives animation code a clearer layout/text/paint substrate.

## Last Shipped Milestone: v1.8 Rendering Engine Architecture

**Goal:** Decide and prove the next rendering architecture for MESH by evaluating Blitz as inspiration or a base, then implementing the minimum integration slice that improves rendering capability without losing shell-specific determinism, observability, and shipped-surface responsiveness.

Phase 42 of v1.8 is complete. MESH now has a source-backed renderer architecture decision matrix that defers direct Blitz production adoption behind Wayland shell fit and browser-engine-level overhead blockers, keeps Blitz as the reference path, and advances a MESH-owned focused-crate prototype path with Taffy, Parley, AnyRender, and AccessKit as preferred standalone candidates.

Phase 43 of v1.8 is complete. MESH now has comparable renderer prototype evidence against the shared navigation/audio shipped-surface slice: Blitz produced a concrete reproducible blocker through the high-level crate, while the MESH-owned focused-crate path produced retained layout, text, paint, interaction, and AccessKit-boundary evidence.

Phase 44 of v1.8 is complete. The selected MESH-owned focused proof path now has a production-adjacent render adapter and shell integration that preserves retained node identity, typed dirty categories, selected paint evidence, damage/profiling payloads, non-fatal diagnostics, text selection proof, and an AccessKit-compatible update boundary while navigation/audio shipped-surface regression tests continue to pass.

Phase 45 of v1.8 is complete. MESH now has a phased and reversible broad renderer migration roadmap, source-backed renderer ownership classification, explicit build/CI/Linux/Nix/binary-risk adoption gates, and an author-facing `.mesh` renderer contract that keeps browser-engine, Blitz, Winit, DOM, and proof-snapshot behavior out of the public authoring promise.

**Target features:**
- Adopt-vs-build decision for Blitz renderer, including whether MESH should reuse Blitz directly, fork/adapt parts, or keep a MESH-owned pipeline.
- Library evaluation for Skia, Stylo, Taffy, Parley, AnyRender, Winit, AccessKit, Muda, html5ever, and xml5ever against MESH's retained rendering, Wayland shell, accessibility, and plugin-surface needs.
- Prototype rendering architecture that preserves MESH's existing invalidation, profiling, diagnostics, module rendering, and shipped navigation/audio surfaces.
- Concrete migration plan for replacing or extending current render/layout/text/style/windowing pieces in phases rather than as a high-risk rewrite.

## Requirements

### Validated

- `v1.1`: Backend plugin MVP is stable enough to host real service providers and surface diagnostics.
- `v1.2`: The renderer supports practical CSS-like styling, interaction reactivity, selection, keyboard navigation, and animation on shipped shell surfaces.
- `v1.3`: Canonical benchmark scenarios, profiling snapshots, debug inspector views, and retained widget-tree identity/dirty summaries are available for measuring real responsiveness work.
- `v1.4`: The renderer has typed invalidation, retained render objects, retained display data, damage tracking, text caching, selector indexing, and batching metrics on the software path.
- `v1.5`: The CPU renderer has profiling attribution, visibility pruning, incremental retained paint-command updates, damage-indexed paint execution, raster cache hardening, and shipped-surface smoothness proof.
- `v1.7 Phase 37`: The canonical module vocabulary is locked: old public names are replacement debt, temporary old loaders are internal migration details, and v1.1 provider plus v1.6 keybind decisions are reconciled into the module/interface/provider/contribution model.
- `v1.7 Phase 38`: Canonical `module.json` normalization is implemented in Rust, old manifest forms produce explicit migration diagnostics, checked-in root/module fixtures use canonical paths, and v1.1 provider plus v1.6 keybind data survive normalization.
- `v1.7 Phase 39`: Interface relationships, backend provider declarations, frontend requirements, host capabilities, and typed contributions are indexed as separate graph concepts with source metadata and manifest-driven extension proof.
- `v1.7 Phase 40`: Migration diagnostics and author docs now point legacy manifest shapes toward canonical `module.json`, and v1.6 keybind declarations remain available through manifest, installed-graph, shell resolution, and settings override paths.
- `v1.7 Phase 41`: The shipped navigation/audio module path proves canonical manifests, typed installed-graph records, provider registration, frontend filtering, and author documentation on a real bundled module/provider workflow.
- `v1.8 Phase 42`: The renderer architecture decision matrix compares Blitz direct adoption, Blitz-inspired borrowing, and a MESH-owned focused-crate path with source-backed candidate outcomes and a Phase 43 dual-prototype handoff.
- `v1.8 Phase 43`: Comparable renderer prototype evidence proves the shared navigation/audio slice, records a concrete Blitz blocker, and selects the MESH-owned focused-crate path for Phase 44.
- `v1.8 Phase 44`: The selected MESH-owned focused proof path is integrated behind current renderer/shell ownership with retained identity, invalidation, damage/profiling, diagnostics, text selection, AccessKit-compatible boundary, and navigation/audio shipped-surface evidence.
- `v1.8 Phase 45`: Broad renderer migration is documented as phased and reversible, existing renderer boundaries are classified as authoritative, adapter-owned, or replacement candidates, and author-facing `.mesh` behavior is bounded by a renderer contract with explicit adoption gates.

### Active

- Productionize the selected renderer libraries from v1.8: Taffy, Parley, AnyRender/Vello-style paint boundaries, and AccessKit runtime updates.
- Keep animation and motion-fidelity polish scoped to the following milestone unless a minimal regression fix is required to preserve existing behavior.

### Out of Scope

- Compositor-global shortcuts via XDG desktop portals or compositor-specific APIs — module/surface-scoped keybinds come first.
- Broad shell UI redesign, marketplace/distribution service work, remote package signing, or installer UX.
- Compositor-global shortcuts via XDG desktop portals or compositor-specific APIs.
- Replacing keyboard focus traversal, text-input behavior, or shipped widget activation semantics.
- Skia-backed rendering investigation — still a future rendering backlog candidate, but not the active v1.7 scope.
- Finishing all paused v1.6 keybind runtime behavior; this milestone only preserves and migrates the declaration/resolution model where it intersects modularity.
- Animation system redesign, transition polish, and richer keyframe behavior — planned for the milestone after renderer library integration.

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
| v1.7 prioritizes conceptual coherence over new feature breadth | MESH now has manifests, providers, interfaces, capabilities, resources, keybinds, docs, and diagnostics grown across multiple milestones; consolidating those contracts reduces future extension friction | Shipped in v1.7 |
| Interface/provider/resource extensibility routes through the installed graph | Typed graph records make extension points inspectable and prevent frontend requirements, backend provider identity, and host capabilities from collapsing into one concept | Shipped in v1.7 Phase 39 |
| Resource and settings compatibility gaps are diagnostics, not graph-load failures | Missing packs, unmapped semantic icons, and duplicate settings namespaces should be visible to tools/settings UI without blocking unrelated modules | Shipped in v1.7 Phase 39 |
| Legacy manifest names are migration inputs, not public aliases | Authors need concrete replacement/removal guidance without reopening old terminology as supported vocabulary | Shipped in v1.7 Phase 40 |
| Module keybind declarations remain canonical while settings only override effective shortcuts | Future dispatch/conflict/accessibility work needs manifest-owned keybind data and user settings must not become a declaration source again | Shipped in v1.7 Phase 40 |
| Direct Blitz production adoption remains blocked | Wayland shell model fit, browser-engine-level overhead, and high-level crate compile evidence make direct adoption too risky for MESH's shell-owned renderer path | Shipped in v1.8 Phase 42 |
| MESH-owned focused-crate path is the selected renderer direction | The focused path preserved retained MESH-shaped evidence across layout, text, paint, interaction, and accessibility without replacing the production renderer wholesale | Shipped in v1.8 Phase 43 |
| Focused renderer proof is adapter-owned, not public API | Phase 44 proof snapshots validate migration boundaries while current renderer/shell ownership remains authoritative for production behavior | Shipped in v1.8 Phase 44 |
| Broad renderer migration must be phased and reversible | Future adoption needs author-contract, ownership-classification, build/CI/release, and rollback gates before becoming broad production behavior | Shipped in v1.8 Phase 45 |

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
*Last updated: 2026-05-18 after starting v1.9 milestone*
