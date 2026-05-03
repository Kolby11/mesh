---
phase: 04-real-core-surfaces
plan: 04
subsystem: scripting-runtime-shell-routing
tags:
  - gap-closure
  - service-proxy
  - authorization
requires:
  - phase: 04-real-core-surfaces
    plan: 03
provides:
  - Deep reactive value comparison
  - Read/control service proxy authorization
  - Shell service command source authorization
affects:
  - crates/core/runtime/scripting/src/context.rs
  - crates/core/shell/src/shell/types.rs
  - crates/core/shell/src/shell/service.rs
  - crates/core/shell/src/shell/mod.rs
  - crates/core/shell/src/shell/component.rs
  - crates/core/foundation/capability/src/lib.rs
tech-stack:
  added: []
  patterns:
    - CapabilitySet travels with script-published events
key-files:
  created: []
  modified:
    - crates/core/runtime/scripting/src/context.rs
    - crates/core/shell/src/shell/service.rs
    - crates/core/shell/src/shell/mod.rs
key-decisions:
  - Read service capability grants state access only; proxy methods require service.<name>.control.
  - Shell service command routing re-checks source capabilities before backend dispatch.
requirements-completed:
  - SURF-02
  - SURF-03
  - SURF-04
  - SURF-05
duration: 12 min
completed: 2026-05-03
---

# Phase 04 Plan 04: Runtime Correctness and Authorization Summary

Deep reactive equality and source-aware service command authorization now close the stale network-list and read-only mutation blockers.

## Execution

- **Duration:** 12 min
- **Started:** 2026-05-03T07:13:00Z
- **Completed:** 2026-05-03T07:25:33Z
- **Tasks:** 3
- **Files modified:** 6

## What Changed

- Replaced shallow nested reactive equality with full `serde_json::Value` equality and added same-length nested object/list coverage.
- Threaded `source_plugin_id` and `source_capabilities` through script-published events and `CoreRequest::ServiceCommand`.
- Denied service proxy method access without `service.<name>.control`, while preserving read-only state fields.
- Added shell routing and dispatch gates so forged read-only `mesh.<service>.<command>` events do not reach backend handlers.

## Commits

| Commit | Description |
|--------|-------------|
| 29f29a0 | Closed Phase 4 runtime, provider, and surface routing gaps. |

## Verification

- `cargo test -p mesh-core-scripting reactive_table -- --nocapture` passed.
- `cargo test -p mesh-core-scripting interface_proxy_method_publishes_service_command -- --nocapture` passed.
- `cargo test -p mesh-core-scripting read_only_interface_proxy_denies_command_methods -- --nocapture` passed.
- `cargo test -p mesh-core-scripting -- --nocapture` passed: 43 tests.
- `cargo test -p mesh-core-shell service_command -- --nocapture` was blocked before shell tests ran because `smithay-client-toolkit` requires missing `xkbcommon.pc`.
- Static `rg` checks confirmed source capability fields, service command gates, and read-only denial coverage are present.

## Deviations from Plan

- Added `Clone` to `CapabilitySet` so source capabilities can travel with published events and routed service commands.

**Total deviations:** 1 auto-fixed. **Impact:** Required to implement the planned authorization data flow.

## Issues Encountered

- Shell crate tests cannot compile in this host environment until the `xkbcommon.pc` development package is available.

## Self-Check: PASSED

- Summary file exists.
- Key modified files exist.
- Scripting verification passed.
- Shell authorization is covered by static checks pending the known host package blocker.

