---
milestone: v1.0
milestone_name: Scripting API Stabilization
status: planning
current_phase: 1
current_phase_name: Backend Host API Contract
progress:
  phases_total: 6
  phases_complete: 0
  plans_total: 0
  plans_complete: 0
updated: 2026-05-01
---

# State: MESH v1.0

## Project Reference

See: `.planning/PROJECT.md` (updated 2026-05-01)

**Core value:** A developer with zero MESH knowledge can write a working top panel plugin and backend service in one sitting, guided only by the API documentation.
**Current focus:** Phase 1: Backend Host API Contract

## Current Position

Phase: 1 of 6 — Backend Host API Contract
Plan: —
Status: Defining phase plan
Last activity: 2026-05-01 — Milestone v1.0 started

## Decisions

- Backend plugins use Luau for service logic; Rust core remains the wiring layer.
- `require('@mesh/service')` is the frontend/backend interface.
- Runtime correctness and documentation are in scope before LSP support.
- Phase numbering starts at 1 because no prior ROADMAP.md exists in this planning setup.

## Blockers

- `gsd-sdk` is not currently available on `PATH`, so this milestone was initialized inline instead of through SDK helper commands.

## Pending Todos

- Run `$gsd-plan-phase 1` to create the executable plan for Phase 1.

## Artifact Index

| Artifact | Path |
|----------|------|
| Project context | `.planning/PROJECT.md` |
| Requirements | `.planning/REQUIREMENTS.md` |
| Roadmap | `.planning/ROADMAP.md` |
| State | `.planning/STATE.md` |
| Codebase map | `.planning/codebase/` |

---
*State reset: 2026-05-01 after milestone v1.0 start*
