---
phase: 03-backend-host-api-contract
plan: 01
subsystem: backend-runtime
tags: [rust, luau, mlua, host-api, process-exec]

requires:
  - phase: 02-backend-lifecycle-foundation
    provides: deterministic backend runtime lifecycle and Luau script host context
provides:
  - strict structured backend process execution through mesh.exec(program, args)
  - stable process result tables for spawn failures and non-zero exits
  - removal of mesh.exec_shell from the backend MVP host API surface
affects: [backend-host-api-contract, bundled-backend-provider-migration, backend-mvp-reference]

tech-stack:
  added: []
  patterns:
    - strict Luau host function argument binding
    - process outcomes converted through ExecOutcome and exec_outcome_to_lua

key-files:
  created:
    - .planning/phases/03-backend-host-api-contract/03-01-SUMMARY.md
  modified:
    - crates/core/runtime/scripting/src/backend.rs
    - crates/core/runtime/scripting/src/host_api.rs

key-decisions:
  - "Backend MVP process execution is strict mesh.exec(program, args); legacy single-string splitting is removed."
  - "BHOST-02 is addressed by removing mesh.exec_shell from the public backend MVP host API surface."

patterns-established:
  - "mesh.exec binds to (String, Vec<String>) so malformed API usage is distinct from process failure result tables."
  - "Backend process failures return success/stdout/stderr/code tables through the existing ExecOutcome conversion path."

requirements-completed: [BHOST-01, BHOST-02]

duration: 4min
completed: 2026-05-03
---

# Phase 03 Plan 01: Backend Host API Process Contract Summary

**Strict structured `mesh.exec(program, args)` with stable failure result tables and no public `mesh.exec_shell` backend API**

## Performance

- **Duration:** 4 min
- **Started:** 2026-05-03T18:27:31Z
- **Completed:** 2026-05-03T18:31:04Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments

- Tightened `mesh.exec` to require a program plus Luau args table; the old single-string command splitting path was removed.
- Added backend tests proving missing programs and non-zero exits return structured result tables instead of throwing.
- Removed `mesh.exec_shell` from backend host registration and comments, with a test asserting it is absent.

## Task Commits

Each task was committed atomically:

1. **Task 1: Make mesh.exec require structured program and args** - `8413d69` (feat)
2. **Task 2: Preserve structured process failure result tables** - `65d641d` (test)
3. **Task 3: Remove mesh.exec_shell from the backend public API surface** - `9680145` (feat)

**Plan metadata:** recorded in the final docs commit.

## Files Created/Modified

- `crates/core/runtime/scripting/src/backend.rs` - Enforces structured `mesh.exec`, preserves result table conversion, removes `exec_shell`, and adds contract tests.
- `crates/core/runtime/scripting/src/host_api.rs` - Updates shared backend host API comments to list `mesh.exec(program, args)`, config, logging, and poll interval control without advertising `exec_shell`.
- `.planning/phases/03-backend-host-api-contract/03-01-SUMMARY.md` - Captures execution results and verification.

## Decisions Made

- Backend MVP command execution now uses only strict structured `mesh.exec(program, args)`.
- The conflicting BHOST-02 requirement is satisfied per Phase 03 context by removing `mesh.exec_shell` from the MVP surface, not preserving it.

## Verification

- `nix develop -c cargo test -p mesh-core-scripting backend` - PASS, 27 backend tests passed.
- `nix develop -c cargo test -p mesh-core-scripting exec_` - PASS, 5 exec-focused tests passed.
- `grep -n "split_whitespace\|exec_shell" crates/core/runtime/scripting/src/backend.rs crates/core/runtime/scripting/src/host_api.rs` - PASS with one deliberate `exec_shell` absence assertion in `bundled_backend_scripts_expose_required_host_api_surface`.
- `grep -n "mesh.set(\"exec_shell\"\|fn run_exec_shell\|split_whitespace" crates/core/runtime/scripting/src/backend.rs crates/core/runtime/scripting/src/host_api.rs` - PASS, no matches.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## Known Stubs

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Plan 02 can migrate bundled backend Luau providers away from `mesh.exec_shell` calls using the now-locked `mesh.exec(program, args)` contract.

## Self-Check: PASSED

- Found summary file: `.planning/phases/03-backend-host-api-contract/03-01-SUMMARY.md`
- Found modified source files: `crates/core/runtime/scripting/src/backend.rs`, `crates/core/runtime/scripting/src/host_api.rs`
- Found task commits: `8413d69`, `65d641d`, `9680145`

---
*Phase: 03-backend-host-api-contract*
*Completed: 2026-05-03*
