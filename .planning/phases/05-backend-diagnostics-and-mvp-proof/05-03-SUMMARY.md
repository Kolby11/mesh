---
phase: 05-backend-diagnostics-and-mvp-proof
plan: 03
subsystem: runtime
tags: [luau, mlua, backend, reference-plugin, media, mvp-proof, testing]

# Dependency graph
requires:
  - phase: 05-01
    provides: Stage-aware BackendScriptError variants
  - phase: 05-02
    provides: Provider-scoped lifecycle diagnostics buckets, stale-state clearing

provides:
  - Fresh @mesh/reference-media backend provider exercising the full MVP contract
  - Automated proof tests for state emission, polling, command dispatch, and failure attribution
  - Plugin-scoped failure visibility attributable to @mesh/reference-media

affects: [05-04, backend-author-docs, mvp-proof]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - In-memory deterministic backend pattern: config-seeded playlist with pure state mutations, no external binaries
    - Reference provider pattern: exercises all MVP host APIs (config, log, set_poll_interval, state global, on_command_*) in one plugin

key-files:
  created:
    - packages/plugins/backend/core/reference-media/plugin.json
    - packages/plugins/backend/core/reference-media/src/main.luau
  modified:
    - crates/core/runtime/scripting/src/backend.rs
    - crates/core/runtime/backend/src/lib.rs

key-decisions:
  - "Reference provider is a fresh directory at @mesh/reference-media — not a retrofit of mpris-media — to cleanly demonstrate the MVP contract without legacy placeholder behavior."
  - "All behavior is deterministic and in-memory, driven solely by config seeds and state mutations, so tests are never flaky due to host services or binaries."
  - "All four on_command_* handlers (play, pause, next, previous) call mesh.log.info with player_id so every command path is logged with provider identity."
  - "Pause-when-not-playing path returns ok=false to prove the failure attribution test without introducing new infrastructure or crashing the backend."
  - "reference_media_invalid_command_returns_plugin_scoped_failure satisfies both task 2 and task 3 — the test asserts source_plugin == @mesh/reference-media, proving failure attribution without requiring dedicated tracing capture."

patterns-established:
  - "Reference provider pattern: single plugin covers init config seeding, mesh.log in command paths, state global, and all required command handlers."

requirements-completed: [BREF-01, BREF-02, BDIAG-04]

# Metrics
duration: 3min
completed: 2026-05-04
---

# Phase 5 Plan 03: Fresh Reference Media Backend Provider Summary

**Fresh @mesh/reference-media backend proves config, logging, polling, state emission, and command handling deterministically without any external binaries or host services.**

## Performance

- **Duration:** ~3 min
- **Started:** 2026-05-04T17:27:36Z
- **Completed:** 2026-05-04T17:30:42Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments

- Created `packages/plugins/backend/core/reference-media/` with `plugin.json` declaring `@mesh/reference-media` implementing `mesh.media` interface v1.0
- Implemented `src/main.luau` with a config-seeded in-memory playlist, top-level `state` global, `init()` calling `mesh.config()`, `mesh.log.info`, and `mesh.service.set_poll_interval`, plus all four `on_command_*` handlers
- Every command handler logs a provider-scoped message with player_id via `mesh.log.info`
- Pause-when-not-playing returns `{ ok = false, error = "not currently playing" }` to provide a deterministic failure path
- Added 2 new tests to `mesh-core-scripting` proving config seeding and command-state updates
- Added 3 new tests to `mesh-core-backend` proving initial state emission, play+next command flow, and plugin-scoped failure attribution
- All 5 new tests pass alongside the existing 88 tests (75 scripting + 20 backend) with no regressions

## Task Commits

1. **Task 1: Scaffold the fresh reference media provider** - `52d8056` (feat)
2. **Task 2: Add automated proof for state emission, polling, and commands** - `e79c5ea` (feat)
3. **Task 3: Prove provider logs and failure visibility stay plugin-scoped** - satisfied by task 2 commit (acceptance criteria all met: mesh.log.info in command paths done in task 1; failure-attribution test added in task 2)

## Files Created/Modified

- `packages/plugins/backend/core/reference-media/plugin.json` — New backend manifest declaring @mesh/reference-media, mesh.media interface, priority 10
- `packages/plugins/backend/core/reference-media/src/main.luau` — In-memory reference provider with config seeding, logging, polling, and four command handlers
- `crates/core/runtime/scripting/src/backend.rs` — Added 2 new tests: reference_media_provider_reads_config_and_exports_state, reference_media_provider_command_updates_state
- `crates/core/runtime/backend/src/lib.rs` — Added 3 new tests: reference_media_backend_emits_initial_state, reference_media_backend_command_returns_result_and_updated_state, reference_media_invalid_command_returns_plugin_scoped_failure

## Decisions Made

- Used distinct `@mesh/reference-media` directory rather than editing the existing `mpris-media` placeholder — keeps the reference implementation clean and separate from a real provider
- Deterministic in-memory state with a 3-track playlist seeded from `mesh.config()` — no external binaries needed, tests are never flaky
- Pause-when-not-playing returns `{ ok = false, error = "not currently playing" }` rather than raising a Lua error — exercises the provider-authored failure path without triggering lifecycle Failed events (which are for script errors, not validation failures)
- `reference_media_invalid_command_returns_plugin_scoped_failure` asserts `source_plugin == "@mesh/reference-media"` on the `CommandResult` — proves failure attribution without needing dedicated tracing infrastructure

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None — no external service configuration required.

## Known Stubs

None — all state is deterministic and correct. No placeholder UI or empty data flows.

## Threat Flags

None — no new network endpoints, auth paths, file access patterns, or schema changes at trust boundaries. The reference provider is purely in-memory.

## Self-Check: PASSED
- `packages/plugins/backend/core/reference-media/plugin.json` exists and contains `"id": "@mesh/reference-media"`
- `packages/plugins/backend/core/reference-media/src/main.luau` exists and contains `state =`, `mesh.config()`, `mesh.log.info`, `mesh.service.set_poll_interval`, `function on_command_play`
- `crates/core/runtime/scripting/src/backend.rs` contains `reference_media_provider_reads_config_and_exports_state` and `reference_media_provider_command_updates_state`
- `crates/core/runtime/backend/src/lib.rs` contains `reference_media_backend_emits_initial_state`, `reference_media_backend_command_returns_result_and_updated_state`, `reference_media_invalid_command_returns_plugin_scoped_failure`
- Commits 52d8056, e79c5ea exist in git log
- 75 tests pass in mesh-core-scripting, 20 tests pass in mesh-core-backend

---
*Phase: 05-backend-diagnostics-and-mvp-proof*
*Completed: 2026-05-04*
