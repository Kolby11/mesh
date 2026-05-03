---
gsd_state_version: 1.0
milestone: v1.1
milestone_name: Backend Plugin MVP
status: executing
stopped_at: Completed 03-02-PLAN.md
last_updated: "2026-05-03T18:45:16.909Z"
last_activity: 2026-05-03
progress:
  total_phases: 5
  completed_phases: 2
  total_plans: 11
  completed_plans: 10
  percent: 91
---

# State: MESH v1.1

## Project Reference

See: `.planning/PROJECT.md` (updated 2026-05-03)

**Core value:** A backend plugin author can write a Luau service plugin, configure it, run it under the shell, emit state, handle commands, and understand failures without reading Rust source.
**Current focus:** Phase 03 — backend-host-api-contract

## Current Position

Phase: 03 (backend-host-api-contract) — EXECUTING
Plan: 4 of 4
Status: Ready to execute
Last activity: 2026-05-03

## Decisions

- Backend plugins use Luau for service logic; Rust core remains the wiring layer.
- `require('@mesh/service')` is the frontend/backend interface.
- Runtime correctness is in scope before LSP, distribution, or new surfaces.
- Phase numbering reset to 1 for v1.1 after archiving v1.0 planning artifacts.
- [v1.1]: Backend plugin MVP starts with a central package.json-like installed-plugin manifest that drives frontend/backend plugin installation, backend category/provider selection, and later runtime lifecycle.
- [v1.1 Phase 02]: Backend startup uses explicit installed-module graph active providers before runtime launch; legacy priority discovery is compatibility fallback only when the graph cannot load.
- [v1.1 Phase 02]: Backend runtime failures emit visible lifecycle status and diagnostics, but do not automatically switch to a fallback provider.
- [v1.1 Phase 02]: Shell owns backend runtime slots by interface and removes service command handlers before replacement or terminal cleanup.
- [Phase 02]: Service proxies are state-and-command surfaces only; callback-style bind/on_change APIs were removed from the public proxy path.
- [Phase 02]: Service update invalidation is based on tracked top-level field value changes, not whole-service emissions.
- [Phase 02]: Lookup diagnostics are recorded before InterfaceUnavailable or CapabilityDenied errors are returned, so pcall changes control flow without hiding visibility.
- [Phase 03]: Plan 02 handler failures are reported through component diagnostics handles and return non-fatal empty request lists.
- [Phase 03]: Plan 02 switch and checkbox state is tracked in shell input state so on_change receives a typed boolean.
- [Phase 03]: Plan 03 proof lives in the shipped navigation-bar volume widget with a typed onchange slider and audio:set_volume command path.
- [Phase 04]: Audio set_volume payload normalization remains in Luau providers; Rust core only verifies generic proxy publication and backend dispatch. — Preserves the Phase 4 architectural rule that service-specific command behavior stays out of Rust core.
- [Phase 04]: Bundled audio providers preserve legacy percent payload compatibility while accepting normalized volume payloads. — Keeps existing command callers working while quick settings moves to the finalized proxy payload shape.
- [Phase 04]: Quick settings audio uses the finalized direct proxy call `audio.set_volume("default", normalized)` for slider changes.
- [Phase 04]: Quick settings Wi-Fi rows remain guarded and display-only when provider data lacks a non-empty network id.
- [Phase 04]: Unavailable and permission-denied states are visible in quick settings while technical details stay in logs and diagnostics.
- [Phase 04]: The top panel remains a compact status and entry surface; direct service controls stay in quick settings.
- [Phase 04]: Final surface regressions use shipped panel source plus focused command snippets to prove callback-free proxy behavior.
- [Phase 04]: Frontend docs show service mutations through named proxy methods instead of legacy service event channels.
- [Phase 04]: Service proxy command methods require `service.<name>.control`; read capability remains state-only.
- [Phase 04]: Shell surface transitions use `shell.toggle-surface` and `shell.hide-surface` with `surface_id`, not quick-settings-specific shell event names.
- [Phase 03]: Plan 01 addresses BHOST-02 by removing mesh.exec_shell from the public backend MVP host API surface. — Phase 03 context explicitly overrides shell execution as non-MVP and requires structured command execution only.
- [Phase 03]: Plan 01 locks backend process execution to strict mesh.exec(program, args); legacy single-string splitting is removed. — Prevents unintended argv tokenization and matches Phase 03 D-01/D-02.
- [Phase 03]: Plan 03 keeps mesh.config() as the only backend config API and returns full plugin settings. — Matches D-08/D-09 and avoids premature config lookup helpers.
- [Phase 03]: Plan 03 locks backend log public levels to debug, info, warn, and error across both call styles. — The warning level remains only as an undocumented compatibility alias.
- [Phase 03]: Plan 03 keeps invalid backend log levels non-fatal and visible through warnings. — Matches D-11 so plugin author mistakes do not crash backend scripts.
- [Phase 03]: Bundled providers issue system commands only through structured mesh.exec(program, args). — Keeps the Phase 03 backend host API strict and removes provider dependency on exec_shell.
- [Phase 03]: Shell pipeline parsing for PipeWire and UPower lives in Luau provider code, not Rust core. — Preserves the no service-specific Rust command behavior rule.

## Performance Metrics

| Plan | Duration | Tasks | Files |
|------|----------|-------|-------|
| Phase 02 P01 | 7min | 3 tasks | 4 files |
| Phase 03 P02 | 9min | 3 tasks | 7 files |
| Phase 03 P03 | 5min | 3 tasks | 4 files |
| Phase 04 P01 | 3min | 3 tasks | 5 files |
| Phase 04 P02 | 6min | 3 tasks | 5 files |
| Phase 04 P03 | 4min | 3 tasks | 2 files |
| Phase 04 P04 | 12min | 3 tasks | 6 files |
| Phase 04 P05 | 4min | 3 tasks | 3 files |
| Phase 04 P06 | 4min | 3 tasks | 4 files |
| Phase 01 P01 | 35min | 3 tasks | 2 files |
| Phase 01 P02 | 30min | 3 tasks | 1 files |
| Phase 01 P03 | 25min | 3 tasks | 12 files |
| Phase 03 P01 | 4min | 3 tasks | 2 files |
| Phase 03 P03 | 4min | 3 tasks | 3 files |
| Phase 03 P02 | 6min | 3 tasks | 5 files |

## Session

Last session: 2026-05-03T18:45:16.900Z
Stopped At: Completed 03-02-PLAN.md
Resume File: None

## Accumulated Context

### Roadmap Evolution

- v1.0 planning artifacts archived to `.planning/milestones/v1.0-reset-2026-05-03-*`.
- v1.1 reset roadmap focuses on backend plugin MVP stability.

## Blockers

(None)

## Pending Todos

- Archived v1.0 verification debt remains in `.planning/milestones/v1.0-reset-2026-05-03-phases/` for reference, but is not part of active v1.1 routing.

## Artifact Index

| Artifact | Path |
|----------|------|
| Project context | `.planning/PROJECT.md` |
| Requirements | `.planning/REQUIREMENTS.md` |
| Roadmap | `.planning/ROADMAP.md` |
| State | `.planning/STATE.md` |
| Codebase map | `.planning/codebase/` |

---
*State reset: 2026-05-03 after milestone v1.1 start*
