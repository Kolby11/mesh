# MESH

## What This Is

MESH is a Rust-based, Wayland-native shell framework that pushes service behavior into Luau plugins while keeping the Rust core focused on generic runtime wiring, state delivery, and diagnostics.

## Current State

`v1.1` shipped on 2026-05-05.

The project now has a backend plugin MVP with:

- A shell-owned installed-plugin package manifest and normalized plugin graph
- Explicit active-provider selection for backend service categories
- Deterministic backend runtime lifecycle with visible status and diagnostics
- A locked backend Luau MVP API: `mesh.exec(program, args)`, `mesh.config()`, `mesh.log(level, msg)`, `mesh.service.emit(...)`, and `mesh.service.set_poll_interval(ms)`
- Generic service state and command routing without service-specific Rust branches
- A reference backend plugin plus automated proof coverage and backend author docs

## Current Milestone: v1.2 Rendering System Upgrade

**Goal:** Make MESH frontend rendering expressive and interactive enough for distinctive shell UI without turning the renderer into a full browser engine.

**Target features:**
- Practical CSS coverage for common shell styling: box model, layout, typography, borders, overflow, visual states, selectors, tokens, and documented unsupported properties.
- Container-size and interaction reactivity so components can restyle on surface/container changes, hover, focus, active, and related state transitions. ✓ Complete in Phase 9.
- Selectable text with mouse drag highlighting and copy support.
- Keyboard navigation and shortcuts for focus movement, activation, and configured key-driven workflows.
- Theme animation tokens and custom CSS animation support for shell-specific motion.
- Navigation-bar migration that proves the new styling, reactivity, keyboard, selection, and animation behavior in an existing core surface.

**Phase 9 complete (2026-05-05):** Container query invalidation, post-restyle layout synchronization, and state preservation through restyles are all implemented and tested. `FrontendSurfaceComponent` now tracks rendered dimensions, re-evaluates container queries on size changes, runs layout after every restyle, and prunes stale hover/focus/active targets — without losing script context, service state, or input values.

## Next Milestone Goals

- Use the rendering upgrade to make plugin authoring feel closer to normal CSS while preserving MESH-specific limits.
- Treat navigation-bar migration as the milestone proof instead of leaving new renderer capabilities as isolated engine tests.
- Keep deferred `v1.1` validation cleanup visible as backlog, but do not mix it into the rendering milestone scope.

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

<details>
<summary>Archived v1.1 milestone framing</summary>

## Previous Milestone Framing

The `v1.1` milestone centered on making backend plugins stable enough for MVP by first introducing a unified shell-owned plugin package manifest, then using it to drive backend lifecycle, provider selection, host APIs, service contracts, and diagnostics.

Target features included the central plugin package manifest, frontend-to-backend dependency declaration, backend lifecycle control, the backend host API contract, service provider contracts, runtime diagnostics, and a fresh reference plugin proving the authoring path.

</details>

---
*Last updated: 2026-05-05 after Phase 9 completion*
