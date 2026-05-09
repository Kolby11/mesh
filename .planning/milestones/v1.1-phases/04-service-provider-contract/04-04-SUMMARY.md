---
phase: 04-service-provider-contract
plan: 04
subsystem: backend-services
tags: [luau, rust, mlua, backend-services, service-contracts]
requires:
  - phase: 04-service-provider-contract
    provides: "Plans 01-03 exported backend state snapshots, interface latest-state storage, frontend module.state proxy reads, and generic command result events."
provides:
  - "Bundled audio, network, power, and theme providers migrated to top-level exported Luau state."
  - "Provider-authored source_plugin fields removed from bundled public provider state."
  - "Bundled provider command handlers return JSON-compatible result tables while refreshing exported state."
  - "Runtime tests proving bundled exported state and bundled command result-table behavior."
affects: [backend-services, scripting-runtime, backend-runtime, service-provider-contract]
tech-stack:
  added: []
  patterns:
    - "Bundled providers initialize a top-level non-local state table with contract-required public fields."
    - "Provider command handlers return { ok = true } or { ok = false, error = ... } and update exported state through shared refresh helpers."
    - "mesh.service.emit remains covered only as compatibility behavior; migrated bundled providers use exported state."
key-files:
  created:
    - .planning/phases/04-service-provider-contract/04-04-SUMMARY.md
  modified:
    - packages/plugins/backend/core/pipewire-audio/src/main.luau
    - packages/plugins/backend/core/pulseaudio-audio/src/main.luau
    - packages/plugins/backend/core/networkmanager-network/src/main.luau
    - packages/plugins/backend/core/upower-power/src/main.luau
    - packages/plugins/backend/core/shell-theme/src/main.luau
    - crates/core/runtime/scripting/src/backend.rs
    - crates/core/runtime/backend/src/lib.rs
key-decisions:
  - "Bundled providers no longer place source_plugin in public state; provider identity remains runtime metadata."
  - "Audio set_volume handlers preserve legacy percent payloads while accepting normalized volume payloads."
  - "Empty exported collection fields are verified directly on the Luau state table in tests because empty subtables are ambiguous after JSON conversion."
patterns-established:
  - "Bundled unavailable state is represented by a contract-shaped exported state table rather than mesh.service.emit_unavailable()."
  - "Network commands return result tables and refresh state after accepted command execution."
  - "Focused bundled_ and command_result test filters exercise migrated provider contract behavior."
requirements-completed: [BSVC-01, BSVC-02, BSVC-03, BSVC-04, BSVC-05]
duration: 6min
completed: 2026-05-03
---

# Phase 04 Plan 04: Bundled Provider Contract Migration Summary

**Bundled backend providers now publish public service data through exported Luau `state` tables and return command result tables without provider-authored identity fields.**

## Performance

- **Duration:** 6 min
- **Started:** 2026-05-03T20:17:53Z
- **Completed:** 2026-05-03T20:23:50Z
- **Tasks:** 3
- **Files modified:** 8

## Accomplishments

- Migrated PipeWire and PulseAudio providers to top-level exported audio state, preserving legacy `percent` payload compatibility and normalized `volume` payload handling.
- Migrated NetworkManager, UPower, and shell-theme providers to exported state and removed provider-authored `source_plugin` from public state.
- Updated scripting/backend runtime tests to prove bundled exported state, compatibility-only `mesh.service.emit`, and a real bundled provider command returning `{ ok = false, error = "invalid sound path" }`.

## Task Commits

Each task was committed atomically:

1. **Task 1: Migrate audio providers to exported state** - `eb144b2` (feat)
2. **Task 2: Migrate network, power, and theme providers to exported state** - `00553d3` (feat)
3. **Task 3: Update bundled runtime tests for exported state and command results** - `3377e39` (test)

**Plan metadata:** committed separately after this summary was written.

## Files Created/Modified

- `packages/plugins/backend/core/pipewire-audio/src/main.luau` - Uses exported audio state and result-returning audio command handlers.
- `packages/plugins/backend/core/pulseaudio-audio/src/main.luau` - Uses exported audio state and result-returning audio command handlers.
- `packages/plugins/backend/core/networkmanager-network/src/main.luau` - Uses exported network state and result-returning network command handlers.
- `packages/plugins/backend/core/upower-power/src/main.luau` - Uses exported power state with required public fields and no public provider identity.
- `packages/plugins/backend/core/shell-theme/src/main.luau` - Uses exported theme state and returns a command result from `set_current`.
- `crates/core/runtime/scripting/src/backend.rs` - Adds bundled exported-state tests and renames the emit test as compatibility coverage.
- `crates/core/runtime/backend/src/lib.rs` - Adds bundled provider command-result event coverage.
- `.planning/phases/04-service-provider-contract/04-04-SUMMARY.md` - Execution record for this plan.

## Decisions Made

- Bundled providers now represent unavailable service data as contract-shaped exported `state` tables instead of calling `mesh.service.emit_unavailable()`.
- Provider identity stays out of public state; runtime metadata remains responsible for `source_plugin`.
- The existing `mpris-media` bundled provider still contains `mesh.service.emit_unavailable()` but is outside this plan's migrated provider set.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Adjusted network exported-state test for empty Luau collection fields**
- **Found during:** Task 3 (runtime test migration)
- **Issue:** The first `bundled_network_provider_exports_state` assertion expected empty collection fields to appear as JSON arrays, but empty Luau subtables are ambiguous after JSON conversion.
- **Fix:** Kept JSON checks for scalar public fields and verified `connections`, `devices`, and `networks` directly on the exported Luau `state` table.
- **Files modified:** `crates/core/runtime/scripting/src/backend.rs`
- **Verification:** `nix develop -c cargo test -p mesh-core-scripting bundled_`
- **Committed in:** `3377e39`

**2. [Rule 2 - Missing Critical] Added a command_result-filtered bundled provider result test**
- **Found during:** Task 3 (runtime test migration)
- **Issue:** The plan's required `cargo test -p mesh-core-backend command_result` filter needed to exercise a bundled provider command result-table path.
- **Fix:** Added `bundled_command_result_handler_returns_result_table` as a focused alias over the bundled PipeWire invalid sound path result-table test.
- **Files modified:** `crates/core/runtime/backend/src/lib.rs`
- **Verification:** `nix develop -c cargo test -p mesh-core-backend command_result`
- **Committed in:** `3377e39`

---

**Total deviations:** 2 auto-fixed (1 bug, 1 missing critical)
**Impact on plan:** Both changes strengthened the planned verification without adding new runtime surface area.

## Issues Encountered

- Nix/Cargo verification briefly waited on shared cache and artifact locks; all required commands completed successfully.
- `.planning/STATE.md` had a pre-existing uncommitted modification at execution start and was intentionally left untouched per the orchestrator instruction.

## Known Stubs

None.

## Threat Flags

None.

## User Setup Required

None - no external service configuration required.

## Verification

- `nix develop -c cargo test -p mesh-core-scripting bundled_` - passed
- `nix develop -c cargo test -p mesh-core-backend command_result` - passed
- `grep -R -n "source_plugin" packages/plugins/backend/core/*/src/main.luau` - no matches
- `grep -R -n "mesh.service.emit" packages/plugins/backend/core/*/src/main.luau` - only `packages/plugins/backend/core/mpris-media/src/main.luau`, outside this plan's migrated providers

## Next Phase Readiness

The bundled providers now exercise the finalized service-provider contract end to end: exported backend state, runtime snapshots, interface latest-state propagation, frontend proxy state reads, and generic command result events are all represented without service-specific Rust command branches.

## Self-Check: PASSED

- Found summary file: `.planning/phases/04-service-provider-contract/04-04-SUMMARY.md`
- Found task commit: `eb144b2`
- Found task commit: `00553d3`
- Found task commit: `3377e39`

---
*Phase: 04-service-provider-contract*
*Completed: 2026-05-03*
