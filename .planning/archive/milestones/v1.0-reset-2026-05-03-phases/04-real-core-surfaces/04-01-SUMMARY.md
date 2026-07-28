---
phase: 04-real-core-surfaces
plan: 01
subsystem: runtime-provider-contract
tags: [rust, luau, service-proxy, audio, backend]

# Dependency graph
requires:
  - phase: 03-frontend-reactivity-and-events
    provides: frontend event handlers and service proxy command path used by quick settings
provides:
  - Audio set_volume proxy publication regression coverage
  - Audio provider payload normalization for volume and percent command shapes
  - Backend command dispatch regression for normalized audio volume payloads
affects: [04-real-core-surfaces, quick-settings, audio-providers, service-proxy]

# Tech tracking
tech-stack:
  added: []
  patterns: [generic Rust proxy/backend dispatch tests with provider-specific Luau command normalization]

key-files:
  created:
    - .planning/phases/04-real-core-surfaces/04-01-SUMMARY.md
  modified:
    - crates/core/runtime/scripting/src/context.rs
    - crates/core/runtime/backend/src/lib.rs
    - packages/plugins/backend/core/audio-interface/interface.toml
    - packages/plugins/backend/core/pipewire-audio/src/main.luau
    - packages/plugins/backend/core/pulseaudio-audio/src/main.luau

key-decisions:
  - "Audio set_volume payload normalization remains in Luau providers; Rust core only verifies generic proxy publication and backend dispatch."
  - "Bundled audio providers preserve legacy percent payload compatibility while accepting normalized volume payloads."

patterns-established:
  - "Provider command handlers derive command percent from payload.percent when present, otherwise from normalized payload.volume."
  - "Proxy command regressions assert the exact public command channel and JSON payload shape."

requirements-completed: [SURF-03]

# Metrics
duration: 3min
completed: 2026-05-03
---

# Phase 04 Plan 01: Audio Command Contract Summary

**Audio set_volume now travels through the finalized proxy/backend contract while bundled providers accept normalized volume payloads and legacy percent payloads.**

## Performance

- **Duration:** 3 min
- **Started:** 2026-05-03T06:28:21Z
- **Completed:** 2026-05-03T06:30:53Z
- **Tasks:** 3
- **Files modified:** 5

## Accomplishments

- Locked frontend audio proxy command publication for both `audio:set_volume("default", 0.5)` and `audio.set_volume("default", 0.5)`.
- Updated PipeWire and PulseAudio providers to convert normalized `payload.volume` into clamped integer percent while retaining `payload.percent`.
- Added backend dispatch coverage proving `mesh.service.payload()` preserves normalized `device_id` and `volume` fields for `on_command_set_volume()`.

## Task Commits

Each task was committed atomically:

1. **Task 1: Lock frontend audio proxy command publication** - `c28f15d` (test)
2. **Task 2: Normalize audio provider set_volume payload handling** - `de1a6da` (fix)
3. **Task 3: Add backend command dispatch regression for normalized audio volume payload** - `cbf6016` (test)

## Files Created/Modified

- `crates/core/runtime/scripting/src/context.rs` - Extended proxy command regression coverage to both colon and dot call styles.
- `crates/core/runtime/backend/src/lib.rs` - Added backend command-loop regression for normalized `set_volume` payload dispatch.
- `packages/plugins/backend/core/audio-interface/interface.toml` - Documented normalized `volume` range for `set_volume`.
- `packages/plugins/backend/core/pipewire-audio/src/main.luau` - Normalizes `payload.volume` or `payload.percent` to clamped `wpctl` percent.
- `packages/plugins/backend/core/pulseaudio-audio/src/main.luau` - Normalizes `payload.volume` or `payload.percent` to clamped `pactl` percent.

## Decisions Made

- Provider-specific audio payload conversion stays in Luau providers, preserving the existing architectural rule that Rust core remains generic.
- Legacy `percent` command payloads remain supported during migration, but the finalized proxy contract uses normalized `volume`.

## Verification

- `cargo test -p mesh-core-scripting interface_proxy_method_publishes_service_command` passed.
- `cargo test -p mesh-core-backend set_volume` passed.
- `rg -n "payload\\.volume|payload\\.percent" packages/plugins/backend/core/pipewire-audio/src/main.luau packages/plugins/backend/core/pulseaudio-audio/src/main.luau` showed both payload shapes in both providers.
- `rg -n "wpctl|pactl" crates/core || true` produced no matches, confirming no service-specific audio commands were added to Rust core.

## Deviations from Plan

None - plan executed exactly as written.

**Total deviations:** 0 auto-fixed.
**Impact on plan:** No scope changes.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Quick settings can now rely on `audio.set_volume(...)` publishing `{ device_id, volume }`, and the bundled audio providers will execute that command shape. Phase 04 Plan 02 can wire the quick-settings slider to the finalized proxy method without provider payload drift.

## Self-Check: PASSED

- Created summary file exists.
- All key modified files exist.
- Task commits found: `c28f15d`, `de1a6da`, `cbf6016`.

---
*Phase: 04-real-core-surfaces*
*Completed: 2026-05-03*
