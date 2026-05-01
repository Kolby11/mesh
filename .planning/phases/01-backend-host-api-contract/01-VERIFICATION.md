---
phase: 01-backend-host-api-contract
verified: 2026-05-01T17:51:41Z
status: passed
score: 8/8 must-haves verified
overrides_applied: 0
gaps: []
---

# Phase 1: Backend Host API Contract Verification Report

**Phase Goal:** Implement and stabilize the backend Luau host APIs that service plugins need for command execution, config access, logging, service emission, and poll interval control.
**Verified:** 2026-05-01T17:51:41Z
**Status:** passed
**Re-verification:** Yes - after gap closure plan 01-03

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | Backend plugins can execute structured commands and shell commands and inspect stdout, stderr, and status. | ✓ VERIFIED | `mesh.exec` still uses `StdCommand::new(program).args(args)` and `mesh.exec_shell` still uses `sh -lc` in [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:231); `exec_accepts_program_and_args` and `exec_returns_structured_result` remain in the passing backend test suite. |
| 2 | Backend plugins can read configured settings through `mesh.config()`. | ✓ VERIFIED | `mesh.config` still returns stored JSON settings in [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:241); Phase 01 Plan 02 previously wired shell settings into the runtime and no gap-closure changes touched that path. |
| 3 | Backend plugins can produce plugin-scoped structured logs. | ✓ VERIFIED | Callable `mesh.log(level, msg)` and aliases remain installed, and `log_message` still tags output with `plugin_id` in [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:253). |
| 4 | Backend plugins can emit service state and adjust polling behavior without shell restart. | ✓ VERIFIED | `mesh.service.emit` and poll-interval plumbing remain intact in [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:176) and [lib.rs](/home/kolby/projects/mesh/crates/core/runtime/backend/src/lib.rs:53); the gap-closure work only narrowed to `emit_json` compatibility. |
| 5 | Backend API failures are surfaced as diagnostics or explicit Luau errors, not silent failures. | ✓ VERIFIED | `mesh.service.emit_json` now parses JSON with `serde_json::from_str::<JsonValue>(...).map_err(mlua::Error::external)` in [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:187), and the new test `emit_json_rejects_invalid_json_string` passes in `cargo test -p mesh-core-scripting backend`. |
| 6 | Existing backend public API names remain compatible while structured forms are added. | ✓ VERIFIED | Bundled fixture usage such as `mesh.service.emit_json(result.stdout)` remains unchanged in [upower-power main.luau](/home/kolby/projects/mesh/packages/plugins/backend/core/upower-power/src/main.luau:73), and backend compatibility tests still pass. |
| 7 | Service-specific logic remains in Luau plugins while Rust only wires runtime, settings, and channels. | ✓ VERIFIED | The gap-closure patch only changed host API translation, tests, and contract text; no service-specific Rust logic was introduced. |
| 8 | Existing `emit_json` and `emit_unavailable` behavior remains compatible. | ✓ VERIFIED | `mesh.service.emit_json` now accepts `Option<LuaValue>` with string, Lua-table, and nil/current-payload branches in [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:187), while docs and LSP now describe the same `value?` contract in [mesh_api.rs](/home/kolby/projects/mesh/crates/tools/lsp/src/knowledge/mesh_api.rs:50) and [backend README](/home/kolby/projects/mesh/docs/plugins/backend/core/README.md:54). |

**Score:** 8/8 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
| --- | --- | --- | --- |
| `crates/core/runtime/scripting/src/backend.rs` | Backend Luau host APIs and tests | ✓ VERIFIED | Restored `emit_json(value?)` compatibility, added explicit invalid-JSON error handling, and added regression tests for string, table, nil/current-payload, and failure cases. |
| `crates/core/runtime/scripting/src/host_api.rs` | Host API docs/comments aligned with runtime API | ✓ VERIFIED | Remains wired and relevant to the backend host API surface. Existing runtime-specific wording ambiguity is a documentation warning, not a blocker to Phase 1 success criteria. |
| `crates/core/runtime/backend/src/lib.rs` | Runtime polling, command dispatch, update channel integration | ✓ VERIFIED | No regressions introduced by the gap-closure patch; Plan 02 summary and unchanged code continue to satisfy the runtime integration truths. |
| `crates/core/shell/src/shell/mod.rs` | Shell spawn plumbing that passes backend settings | ✓ VERIFIED | No regressions introduced by the gap-closure patch; the settings pass-through path remains unchanged from Plan 02. |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| --- | --- | --- | --- |
| `emit_json` compatibility and failure visibility | `cargo test -p mesh-core-scripting emit_json` | 5 tests passed | ✓ PASS |
| Backend host API regression suite | `cargo test -p mesh-core-scripting backend` | 22 tests passed | ✓ PASS |
| Contract text alignment across runtime, docs, and LSP | `rg -n "emit_json\\(value\\?\\)|current command payload|from_str::<JsonValue>|current_payload" crates/core/runtime/scripting/src/backend.rs crates/tools/lsp/src/knowledge/mesh_api.rs docs/plugins/backend/core/README.md` | Matches found in all three surfaces | ✓ PASS |
| Bundled compatibility fixture preserved | `rg -n "mesh\\.service\\.emit_json\\(result\\.stdout\\)" packages/plugins/backend/core/upower-power/src/main.luau` | Match found | ✓ PASS |

### Requirements Coverage

| Requirement | Description | Status | Evidence |
| --- | --- | --- | --- |
| `HOST-01` | Structured `mesh.exec(cmd, args)` returns stdout/stderr/status | ✓ SATISFIED | Verified by unchanged implementation and passing backend scripting tests. |
| `HOST-02` | `mesh.exec_shell(cmd)` returns stdout/stderr/status | ✓ SATISFIED | Verified by unchanged implementation and passing backend scripting tests. |
| `HOST-03` | `mesh.config()` returns plugin settings as a Luau table | ✓ SATISFIED | Verified by existing runtime path plus passing `config_returns_backend_settings`. |
| `HOST-04` | `mesh.log(level, msg)` produces plugin-associated logs | ✓ SATISFIED | Verified by existing callable log table and passing logging test. |
| `HOST-05` | `mesh.service.emit(payload)` publishes JSON-compatible state payloads to the shell | ✓ SATISFIED | Verified by existing emit path plus the restored `emit_json` compatibility and failure behavior. |
| `HOST-06` | `mesh.service.set_poll_interval(ms)` affects the backend poll loop without restart | ✓ SATISFIED | Verified by unchanged Plan 02 runtime integration and its passing summary evidence. |

### Residual Warnings

These do not block Phase 01 completion but remain useful follow-up quality items:

- `crates/core/runtime/backend/src/lib.rs` command-triggered updates still bypass duplicate-payload suppression from the poll path.
- `crates/core/runtime/scripting/src/host_api.rs` still documents frontend and backend `mesh.config` shapes close together, which may remain mildly ambiguous for plugin authors.

### Human Verification Required

None. The previously failing gaps are directly observable in code and covered by automated backend tests.

### Verification Summary

Phase 01 now satisfies its original stabilization goal. The gap-closure plan restored the public `mesh.service.emit_json(value?)` compatibility contract, made malformed JSON fail visibly through `mlua::Error`, and aligned the runtime, documentation, and LSP API knowledge so plugin authors and editor hints no longer disagree with the implementation.

The remaining concerns from review are warnings rather than blockers. They do not invalidate the phase’s must-haves or success criteria, so Phase 01 can be marked complete.

---

_Verified: 2026-05-01T17:51:41Z_
_Verifier: Codex inline verification after execute-phase gap closure_
