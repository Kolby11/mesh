---
phase: 03-backend-host-api-contract
plan: 02
subsystem: backend-runtime
tags: [luau, host-api, process-exec, bundled-providers]

requires:
  - phase: 03-backend-host-api-contract
    provides: strict structured backend process execution through mesh.exec(program, args)
provides:
  - bundled audio, network, and power providers migrated off mesh.exec_shell
  - Luau-side parsing for PipeWire sink selection and UPower battery state
  - bundled provider fixture coverage for migrated scripts under the strict host API
affects: [backend-host-api-contract, backend-mvp-reference, service-provider-contract]

tech-stack:
  added: []
  patterns:
    - structured mesh.exec(program, args) provider command invocation
    - provider-local Luau parsing replacing shell pipelines

key-files:
  created:
    - .planning/phases/03-backend-host-api-contract/03-02-SUMMARY.md
  modified:
    - packages/plugins/backend/core/pipewire-audio/src/main.luau
    - packages/plugins/backend/core/pulseaudio-audio/src/main.luau
    - packages/plugins/backend/core/networkmanager-network/src/main.luau
    - packages/plugins/backend/core/upower-power/src/main.luau
    - crates/core/runtime/scripting/src/backend.rs

key-decisions:
  - "Bundled providers issue system commands only through structured mesh.exec(program, args)."
  - "Shell pipeline parsing for PipeWire and UPower lives in Luau provider code, not Rust core."

patterns-established:
  - "Provider command handlers pass dynamic values as args table entries instead of interpolated shell strings."
  - "Bundled host API fixture coverage should include migrated providers that previously depended on exec_shell."

requirements-completed: [BHOST-01, BHOST-02]

duration: 6min
completed: 2026-05-03
---

# Phase 03 Plan 02: Bundled Provider Structured Exec Migration Summary

**Bundled backend providers now use structured `mesh.exec(program, args)` with provider-local Luau parsing instead of shell pipelines**

## Performance

- **Duration:** 6 min
- **Started:** 2026-05-03T18:35:48Z
- **Completed:** 2026-05-03T18:41:53Z
- **Tasks:** 3
- **Files modified:** 5

## Accomplishments

- Migrated PipeWire and PulseAudio audio providers from `mesh.exec_shell` to structured `wpctl` and `pactl` calls.
- Migrated NetworkManager and UPower providers to structured `nmcli`, `bluetoothctl`, and `upower` calls.
- Replaced PipeWire awk sink selection and UPower awk JSON generation with Luau parsing while preserving emitted payload fields.
- Extended bundled backend host API fixture coverage to load PipeWire and PulseAudio scripts without `exec_shell`.

## Task Commits

Each task was committed atomically:

1. **Task 1: Migrate audio providers to structured exec** - `4cef91e` (feat)
2. **Task 2: Migrate network and power providers to structured exec** - `e5981ed` (feat)
3. **Task 3: Add static and bundled-provider verification for exec_shell removal** - `57eedba` (test)

**Plan metadata:** recorded in the final docs commit.

## Files Created/Modified

- `packages/plugins/backend/core/pipewire-audio/src/main.luau` - Uses structured `wpctl` calls and Luau sink-id parsing.
- `packages/plugins/backend/core/pulseaudio-audio/src/main.luau` - Uses structured `pactl` calls for volume, mute, and state reads.
- `packages/plugins/backend/core/networkmanager-network/src/main.luau` - Uses structured `nmcli` and `bluetoothctl` calls for polling and commands.
- `packages/plugins/backend/core/upower-power/src/main.luau` - Parses `upower -i` output in Luau and emits the existing battery payload shape.
- `crates/core/runtime/scripting/src/backend.rs` - Adds PipeWire and PulseAudio to bundled host API fixture coverage.
- `.planning/phases/03-backend-host-api-contract/03-02-SUMMARY.md` - Captures execution results and verification.

## Decisions Made

- Followed the Phase 03 contract decision that `BHOST-02` is satisfied by removing bundled dependency on `mesh.exec_shell` rather than preserving public shell execution.
- Kept command parsing and service-specific interpretation in Luau providers; Rust core changes were limited to test fixture coverage.

## Verification

- `nix develop -c cargo test -p mesh-core-scripting backend` - PASS, 28 backend tests passed.
- `nix develop -c cargo test -p mesh-core-scripting bundled_backend_scripts_expose_required_host_api_surface` - PASS, migrated bundled scripts load and initialize under the strict host API fixture.
- `grep -R "mesh.exec_shell" packages/plugins/backend/core/*/src/main.luau` - PASS, no provider matches.
- `grep -R "wpctl\|pactl\|nmcli\|upower" crates/core/shell crates/core/runtime -n` - PASS for service behavior: only provider identity/test fixture path references remain, not Rust command parsing or execution branches.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- Concurrent Phase 03 work briefly modified `crates/core/runtime/scripting/src/backend.rs`; Task 3 reread the current file and applied only the provider fixture additions on top of those changes.
- An unrelated untracked `.planning/phases/03-backend-host-api-contract/03-03-SUMMARY.md` was present after the concurrent executor ran. It was left untouched.

## Known Stubs

None. Stub scan found no TODO, FIXME, placeholder, coming soon, or not available markers in the modified files. Empty Luau tables in the NetworkManager provider are runtime accumulators or existing emitted collection fields.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Bundled providers no longer require public shell execution. Later Phase 03 host API work can continue stabilizing config, logging, and poll interval behavior without carrying provider `exec_shell` compatibility.

## Self-Check: PASSED

- Found summary file: `.planning/phases/03-backend-host-api-contract/03-02-SUMMARY.md`
- Found modified source files: `packages/plugins/backend/core/pipewire-audio/src/main.luau`, `packages/plugins/backend/core/pulseaudio-audio/src/main.luau`, `packages/plugins/backend/core/networkmanager-network/src/main.luau`, `packages/plugins/backend/core/upower-power/src/main.luau`, `crates/core/runtime/scripting/src/backend.rs`
- Found task commits: `4cef91e`, `e5981ed`, `57eedba`

---
*Phase: 03-backend-host-api-contract*
*Completed: 2026-05-03*
