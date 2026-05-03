---
phase: 02-service-proxy-delivery
plan: 01
subsystem: runtime
tags: [rust, luau, service-proxy, diagnostics, shell]

requires:
  - phase: 01-backend-host-api-contract
    provides: backend service emissions and host API runtime plumbing
provides:
  - Visible diagnostics for failed service interface lookups
  - Top-level service proxy field read tracking
  - Value-based shell invalidation for tracked service fields
  - Regression coverage for live proxy reads and command routing
affects: [service-proxy-delivery, frontend-reactivity, core-surfaces]

tech-stack:
  added: []
  patterns:
    - Lua proxy __index records top-level service field dependencies
    - Shell compares tracked service fields before marking frontend components dirty
    - Interface lookup failures record diagnostics before returning Lua errors

key-files:
  created:
    - .planning/phases/02-service-proxy-delivery/02-01-SUMMARY.md
  modified:
    - crates/core/runtime/scripting/src/context.rs
    - crates/core/shell/src/shell/component.rs
    - crates/core/shell/src/shell/service.rs

key-decisions:
  - "Service proxies are state-and-command surfaces only; callback-style bind/on_change APIs were removed from the public proxy path."
  - "Service update invalidation is based on tracked top-level field value changes, not whole-service emissions."
  - "Lookup diagnostics are recorded before InterfaceUnavailable or CapabilityDenied errors are returned, so pcall changes control flow without hiding visibility."

patterns-established:
  - "Lookup diagnostics: call record_lookup_diagnostic before returning interface/capability lookup errors."
  - "Proxy dependency tracking: create_service_proxy records service name plus top-level field on every non-method proxy read."
  - "Shell invalidation: handle_service_event compares only tracked fields between previous and next payloads."

requirements-completed: [PROXY-01, PROXY-02, PROXY-04, PROXY-05, PROXY-06]

duration: 7min
completed: 2026-05-02
---

# Phase 02 Plan 01: Reactive Proxy Runtime Summary

**Service proxies now expose live state and command methods with visible lookup diagnostics and field-level rerender invalidation.**

## Performance

- **Duration:** 7 min
- **Started:** 2026-05-02T09:13:21Z
- **Completed:** 2026-05-02T09:20:43Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments

- Added plugin-visible diagnostic records for failed `require("@mesh/...")`, `mesh.service.use(...)`, and explicit interface imports before returning the existing Lua error path.
- Removed legacy proxy callback/bind invalidation from the runtime and replaced it with tracked top-level field reads from `__mesh_svc_<service>`.
- Updated shell service handling so repeated emissions with unchanged tracked fields do not dirty the component.
- Preserved named proxy command routing through `mesh.<service>.<command>` channels and covered audio/network command cases.

## Task Commits

1. **Task 1: Emit visible diagnostics for failed interface lookups** - `09de66e` (feat)
2. **Task 2: Replace callback-style proxy invalidation with tracked fields** - `074b23e` (feat)
3. **Task 3: Preserve command routing and repeated live proxy state** - `57ac87d` (test)

## Files Created/Modified

- `crates/core/runtime/scripting/src/context.rs` - Lookup diagnostics, service field dependency tracking, callback-free proxy reads, and regression tests.
- `crates/core/shell/src/shell/component.rs` - Value-based dirty invalidation for tracked service fields.
- `crates/core/shell/src/shell/service.rs` - Regression tests for audio and network command channel routing.
- `.planning/phases/02-service-proxy-delivery/02-01-SUMMARY.md` - Execution summary.

## Decisions Made

- Removed `bind` and `on_change` from the proxy `__index` surface instead of keeping compatibility shims.
- Kept `mesh.service.use(...)` as a proxy construction helper, but it now follows the same lookup diagnostic/error rules as `require(...)`.
- Used shallow top-level field tracking by service name, matching the Phase 02 contract rather than deep path tracking.

## Deviations from Plan

None - plan executed as written.

## Issues Encountered

- Running `cargo test -p mesh-core-shell` outside the Nix shell failed because the host environment lacked `xkbcommon.pc`. Retried with `nix develop --command cargo test -p mesh-core-shell`, which passed.

## Known Stubs

- `crates/core/shell/src/shell/component.rs:1224` contains an existing zero-size placeholder `WidgetNode` for blocked surface portal recursion. This was pre-existing and unrelated to proxy delivery.
- `crates/core/runtime/scripting/src/context.rs:1359` initializes `audio_source = ""` inside a test before live payload assertions. This is test setup, not UI-facing stub data.

## Verification

- `cargo test -p mesh-core-scripting context` - passed, 15 tests.
- `nix develop --command cargo test -p mesh-core-shell` - passed, 10 tests.
- `rg -n "InterfaceUnavailable|CapabilityDenied|pcall|plugin_id|requested version" crates/core/runtime/scripting/src/context.rs` - diagnostics path present.
- `rg -n "__mesh_svc_|tracked|dirty" crates/core/runtime/scripting/src/context.rs crates/core/shell/src/shell/component.rs` - tracked reads and dirty comparison paths present.
- `rg -n "mesh\\.audio\\.set_volume|mesh\\.network\\.set_wifi_enabled|ServiceCommand" crates/core/shell/src/shell/service.rs` - command routing present.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Plan 02 can build on a runtime that exposes service state through direct proxy reads, records lookup diagnostics even under `pcall`, and invalidates components only when consumed top-level service fields change.

## Self-Check: PASSED

- Created files exist: `.planning/phases/02-service-proxy-delivery/02-01-SUMMARY.md`.
- Modified code files exist: `context.rs`, `component.rs`, `service.rs`.
- Task commits exist: `09de66e`, `074b23e`, `57ac87d`.

---
*Phase: 02-service-proxy-delivery*
*Completed: 2026-05-02*
