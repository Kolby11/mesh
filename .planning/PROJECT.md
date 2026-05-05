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

## Next Milestone Goals

- Decide whether the next milestone finishes validation and audit debt first or moves directly into new product scope.
- Carry forward deferred `v1.1` cleanup: finalize Nyquist metadata, document the manual live-host validation boundary, and retire obsolete verification notes.
- Use the shipped backend MVP as the baseline for any next-step work in tooling, distribution, or new shell surfaces.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Backend plugins use Luau for service logic | Keeps Rust core as wiring and makes services extensible | Locked |
| Rust core must stay generic across services | Prevents audio/network/power special cases from becoming architecture | Locked |
| Package graph comes before backend lifecycle | A unified installed-plugin interface should drive which backend providers exist and which one is active | Shipped in v1.1 |
| Backend runtime failure does not auto-fallback | Deterministic cleanup and visible status are safer than hidden provider switching | Locked |
| `mesh.exec_shell` is outside the backend MVP host API | Structured argv execution avoids shell parsing ambiguity | Shipped in v1.1 |
| Backend MVP comes before remote distribution and LSP | Runtime stability and local package semantics are prerequisites for tooling and package workflows | Still true |

<details>
<summary>Archived v1.1 milestone framing</summary>

## Previous Milestone Framing

The `v1.1` milestone centered on making backend plugins stable enough for MVP by first introducing a unified shell-owned plugin package manifest, then using it to drive backend lifecycle, provider selection, host APIs, service contracts, and diagnostics.

Target features included the central plugin package manifest, frontend-to-backend dependency declaration, backend lifecycle control, the backend host API contract, service provider contracts, runtime diagnostics, and a fresh reference plugin proving the authoring path.

</details>

---
*Last updated: 2026-05-05 after v1.1 milestone archival*
