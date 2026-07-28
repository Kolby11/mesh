---
phase: 01-backend-host-api-contract
plan: 03
subsystem: backend-host-api
tags: [rust, luau, mlua, backend, lsp, docs]
requires:
  - phase: 01-01
    provides: backend host API execution, config, and logging surface
  - phase: 01-02
    provides: runtime-backed backend service emission and polling behavior
provides:
  - restored `mesh.service.emit_json(value?)` compatibility for string, table, and nil/current-payload forms
  - visible invalid-JSON failure behavior for backend `emit_json`
  - aligned runtime, docs, and LSP contract text for `emit_json`
affects: [backend-runtime, plugin-authors, lsp-api-knowledge, backend-docs]
tech-stack:
  added: []
  patterns: [visible host-api failures, compatibility-preserving backend host API evolution]
key-files:
  created: [.planning/phases/01-backend-host-api-contract/01-03-SUMMARY.md]
  modified:
    - crates/core/runtime/scripting/src/backend.rs
    - crates/tools/lsp/src/knowledge/mesh_api.rs
    - docs/plugins/backend/core/README.md
key-decisions:
  - "Restored `mesh.service.emit_json(value?)` as a compatibility API instead of keeping the narrowed string-only form."
  - "Invalid JSON now raises an `mlua::Error` so backend API misuse is visible instead of silently discarded."
patterns-established:
  - "When a backend host API is documented in runtime comments, plugin docs, and LSP knowledge, all three surfaces should be updated together."
requirements-completed: [HOST-05]
completed: 2026-05-01
---

# Phase 01 Plan 03: Backend Host API Contract Summary

**Restored `mesh.service.emit_json(value?)` compatibility and made malformed JSON fail visibly**

## Performance

- **Completed:** 2026-05-01T17:48:06Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- Restored backend `mesh.service.emit_json(...)` so it accepts JSON strings, Lua tables, and `nil` fallback to the current command payload.
- Made malformed JSON visible by returning an `mlua::Error` instead of silently swallowing parse failures.
- Synced the restored contract across runtime tests, backend docs, and LSP API knowledge.

## Task Commits

Implemented in one atomic gap-closure commit because the runtime behavior, regression tests, and contract text were tightly coupled:

1. **Tasks 1-3: Restore compatibility, add regression coverage, and align docs/LSP** - `1b9cad4` (fix)

## Files Created/Modified

- `crates/core/runtime/scripting/src/backend.rs` - Restored `emit_json(value?)` behavior and added nil/table/error regression tests.
- `crates/tools/lsp/src/knowledge/mesh_api.rs` - Updated the editor-facing `emit_json` signature and description.
- `docs/plugins/backend/core/README.md` - Updated backend plugin docs to describe JSON text, Lua table, and nil/current-payload fallback.
- `.planning/phases/01-backend-host-api-contract/01-03-SUMMARY.md` - Plan execution summary.

## Decisions Made

- Kept the existing string-based `mesh.service.emit_json(result.stdout)` path fully compatible for bundled backends like `upower-power`.
- Used `runtime.current_payload` for `emit_json(nil)` fallback so each handler re-emits only its active command payload, not stale prior state.
- Treated invalid JSON as host API misuse that should raise immediately rather than log-and-ignore.

## Deviations from Plan

None

## Issues Encountered

None

## User Setup Required

None

## Next Phase Readiness

- The two blocker gaps from Phase 01 verification are addressed in code and covered by backend tests.
- Phase 01 is ready for formal re-verification against the existing `01-VERIFICATION.md` findings.

## Verification

- `cargo test -p mesh-core-scripting emit_json` — passed
- `cargo test -p mesh-core-scripting backend` — passed
- `rg -n "emit_json\\(value\\?\\)|current command payload|from_str::<JsonValue>|current_payload" crates/core/runtime/scripting/src/backend.rs crates/tools/lsp/src/knowledge/mesh_api.rs docs/plugins/backend/core/README.md` — matched restored compatibility contract
- `rg -n "mesh\\.service\\.emit_json\\(result\\.stdout\\)" packages/plugins/backend/core/upower-power/src/main.luau` — confirmed bundled backend compatibility fixture still uses the preserved string form

## Self-Check: PASSED

- Summary file exists: `.planning/phases/01-backend-host-api-contract/01-03-SUMMARY.md`
- Verified task commit: `1b9cad4`

---
*Phase: 01-backend-host-api-contract*
*Completed: 2026-05-01*
