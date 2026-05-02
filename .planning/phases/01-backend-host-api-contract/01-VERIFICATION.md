---
phase: 01-backend-host-api-contract
verified: 2026-05-01T17:50:41Z
status: passed
score: 8/8 must-haves verified
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 6/8 must-haves verified
  gaps_closed:
    - "Backend API failures are surfaced as diagnostics or explicit Luau errors, not silent failures."
    - "Existing `emit_json` and `emit_unavailable` behavior remains compatible."
  gaps_remaining: []
  regressions: []
---

# Phase 1: Backend Host API Contract Verification Report

**Phase Goal:** Implement and stabilize the backend Luau host APIs that service plugins need for command execution, config access, logging, service emission, and poll interval control.
**Verified:** 2026-05-01T17:50:41Z
**Status:** passed
**Re-verification:** Yes - after gap closure

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | Backend plugins can execute structured commands and shell commands and inspect stdout, stderr, and status. | ✓ VERIFIED | `mesh.exec` and `mesh.exec_shell` remain installed in [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:231); `cargo test -p mesh-core-scripting backend -- --nocapture` passed, including `exec_accepts_program_and_args` and `exec_returns_structured_result`. |
| 2 | Backend plugins can read configured settings through `mesh.config()`. | ✓ VERIFIED | `mesh.config()` still returns runtime settings in [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:240), and runtime wiring still passes settings into `BackendScriptContext` in [lib.rs](/home/kolby/projects/mesh/crates/core/runtime/backend/src/lib.rs:23); `cargo test -p mesh-core-backend -- --nocapture` passed `spawn_backend_service_passes_settings_into_backend_context`. |
| 3 | Backend plugins can produce plugin-scoped structured logs. | ✓ VERIFIED | Callable `mesh.log(level, msg)` plus aliases remain installed in [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:256), and `cargo test -p mesh-core-scripting backend -- --nocapture` passed `log_level_function_and_aliases_are_callable`. |
| 4 | Backend plugins can emit service state and adjust polling behavior without shell restart. | ✓ VERIFIED | `mesh.service.emit`, `mesh.service.emit_json`, and `mesh.service.set_poll_interval` are present in [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:176), and runtime still refreshes intervals in [lib.rs](/home/kolby/projects/mesh/crates/core/runtime/backend/src/lib.rs:50); `cargo test -p mesh-core-backend -- --nocapture` passed `spawn_backend_service_applies_runtime_poll_interval_changes`. |
| 5 | Backend API failures are surfaced as diagnostics or explicit Luau errors, not silent failures. | ✓ VERIFIED | `mesh.service.emit_json` now maps JSON parse failure to `mlua::Error::external` instead of swallowing it in [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:187); `emit_json_rejects_invalid_json_string` and `bad_emit_json_does_not_emit_stale_state` passed at [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:610). |
| 6 | Existing backend public API names remain compatible while structured forms are added. | ✓ VERIFIED | The bundled compatibility fixture test `bundled_backend_scripts_expose_required_host_api_surface` passed at [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:468), and the real runtime integration `shell_theme_backend_runs_through_runtime_loop` still passed in `cargo test -p mesh-core-backend -- --nocapture`. |
| 7 | Service-specific logic remains in Luau plugins while Rust only wires runtime, settings, and channels. | ✓ VERIFIED | Runtime still loads script source and delegates `init`, `on_poll`, and `on_command_*` through `BackendScriptContext` in [lib.rs](/home/kolby/projects/mesh/crates/core/runtime/backend/src/lib.rs:23); no service-specific Rust logic was introduced by the gap-closure change. |
| 8 | Existing `emit_json` and `emit_unavailable` behavior remains compatible. | ✓ VERIFIED | `emit_json` now accepts `Option<LuaValue>` and supports string, table, and nil/current-payload forms in [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:187); regression tests for all three forms passed at [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:564), and the bundled UPower backend still uses the preserved string form in [main.luau](/home/kolby/projects/mesh/packages/plugins/backend/core/upower-power/src/main.luau:73). |

**Score:** 8/8 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
| --- | --- | --- | --- |
| `crates/core/runtime/scripting/src/backend.rs` | Backend Luau host APIs and regression tests | ✓ VERIFIED | Substantive host API implementation plus direct coverage for string/table/nil `emit_json`, explicit invalid-JSON failure, and stale-state protection at [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:187) and [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:564). |
| `crates/core/runtime/backend/src/lib.rs` | Runtime polling, command dispatch, update channel integration | ✓ VERIFIED | Still wired through `spawn_backend_service(...)`, dynamic interval refresh, and backend integration tests at [lib.rs](/home/kolby/projects/mesh/crates/core/runtime/backend/src/lib.rs:23). |
| `crates/tools/lsp/src/knowledge/mesh_api.rs` | Editor-facing backend API contract text | ✓ VERIFIED | `mesh.service.emit_json(value?)` is documented with nil/current-payload behavior at [mesh_api.rs](/home/kolby/projects/mesh/crates/tools/lsp/src/knowledge/mesh_api.rs:50). |
| `docs/plugins/backend/core/README.md` | Plugin-author backend API documentation | ✓ VERIFIED | Backend docs describe JSON text, Lua table, and nil/current-payload `emit_json` compatibility at [README.md](/home/kolby/projects/mesh/docs/plugins/backend/core/README.md:54). |

### Key Link Verification

| From | To | Via | Status | Details |
| --- | --- | --- | --- | --- |
| `crates/core/runtime/scripting/src/backend.rs` | backend Luau scripts | `mesh` global host injection | ✓ WIRED | `emit_json`, `emit_unavailable`, `payload`, and logging APIs are installed directly into the `mesh.service` and `mesh.log` tables at [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:159). |
| `crates/core/runtime/backend/src/lib.rs` | `crates/core/runtime/scripting/src/backend.rs` | `BackendScriptContext::new_with_settings_and_capabilities(...)` | ✓ WIRED | Runtime still constructs the scripting context and drives `run_poll()` / `run_command()` in [lib.rs](/home/kolby/projects/mesh/crates/core/runtime/backend/src/lib.rs:32). |
| `crates/tools/lsp/src/knowledge/mesh_api.rs` | runtime `emit_json` contract | shared `emit_json(value?)` signature and nil/current-payload description | ✓ WIRED | LSP entry matches the runtime signature and semantics at [mesh_api.rs](/home/kolby/projects/mesh/crates/tools/lsp/src/knowledge/mesh_api.rs:50) and [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:187). |
| `docs/plugins/backend/core/README.md` | runtime `emit_json` contract | backend plugin ergonomics docs | ✓ WIRED | Plugin docs match the runtime behavior and bundled UPower usage at [README.md](/home/kolby/projects/mesh/docs/plugins/backend/core/README.md:54) and [main.luau](/home/kolby/projects/mesh/packages/plugins/backend/core/upower-power/src/main.luau:73). |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| --- | --- | --- | --- | --- |
| `crates/core/runtime/scripting/src/backend.rs` | `payload` emitted by `mesh.service.emit_json(...)` | Current command payload, parsed JSON text, or Lua table conversion in `emit_json` | Yes | ✓ FLOWING |
| `crates/core/runtime/backend/src/lib.rs` | `payload` in `BackendServiceUpdate` | `ctx.run_poll()` / `ctx.run_command()` | Yes | ✓ FLOWING |
| `crates/core/runtime/backend/src/lib.rs` | `settings` exposed through `mesh.config()` | `spawn_backend_service(..., settings, ...)` constructor argument | Yes | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| --- | --- | --- | --- |
| `emit_json` compatibility forms work | `cargo test -p mesh-core-scripting emit_json -- --nocapture` | 5 tests passed: explicit string, Lua table, nil/current-payload, invalid JSON rejection, stale-state protection | ✓ PASS |
| Backend scripting surface still holds together | `cargo test -p mesh-core-scripting backend -- --nocapture` | 22 backend tests passed | ✓ PASS |
| Runtime wiring still works | `cargo test -p mesh-core-backend -- --nocapture` | 3 runtime tests passed | ✓ PASS |
| Runtime/docs/LSP contract alignment | `rg -n "emit_json\\(" . --glob '!target/**'` | Runtime comment, LSP entry, backend docs, tests, and bundled UPower plugin all reference the restored `emit_json(value?)` contract | ✓ PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| --- | --- | --- | --- | --- |
| `HOST-01` | `01-01`, `01-02` | Structured `mesh.exec(cmd, args)` returns stdout/stderr/status | ✓ SATISFIED | `mesh.exec` wiring at [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:231) plus passing backend scripting tests. |
| `HOST-02` | `01-01`, `01-02` | `mesh.exec_shell(cmd)` returns stdout/stderr/status | ✓ SATISFIED | `mesh.exec_shell` wiring at [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:250) plus passing backend scripting tests. |
| `HOST-03` | `01-01`, `01-02` | `mesh.config()` returns plugin settings as a Luau table | ✓ SATISFIED | Settings are injected at [lib.rs](/home/kolby/projects/mesh/crates/core/runtime/backend/src/lib.rs:23) and exposed at [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:240). |
| `HOST-04` | `01-01`, `01-02` | `mesh.log(level, msg)` produces plugin-associated logs | ✓ SATISFIED | Callable log table installed at [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:256). |
| `HOST-05` | `01-01`, `01-02`, `01-03` | `mesh.service.emit(payload)` publishes JSON-compatible state payloads to the shell | ✓ SATISFIED | `emit` and `emit_json` both feed `pending_emit` in [backend.rs](/home/kolby/projects/mesh/crates/core/runtime/scripting/src/backend.rs:176), and runtime forwards updates in [lib.rs](/home/kolby/projects/mesh/crates/core/runtime/backend/src/lib.rs:60). |
| `HOST-06` | `01-02` | `mesh.service.set_poll_interval(ms)` affects the backend poll loop without restart | ✓ SATISFIED | Runtime refreshes the active interval in [lib.rs](/home/kolby/projects/mesh/crates/core/runtime/backend/src/lib.rs:103), with passing runtime test coverage. |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| --- | --- | --- | --- | --- |
| `crates/core/runtime/backend/src/lib.rs` | 70 | Command-triggered updates still bypass duplicate-payload suppression | ⚠️ Warning | Can emit redundant updates after commands, but does not block the Phase 1 host API contract or the closed `emit_json` gaps. |
| `crates/core/runtime/scripting/src/host_api.rs` | 16 | Frontend `mesh.config.get*` and backend `mesh.config()` remain documented together without runtime scoping | ⚠️ Warning | Leaves a documentation ambiguity outside the specific `emit_json` contract verified here. |

### Gaps Summary

The two prior blockers are closed. `mesh.service.emit_json(...)` now accepts the documented compatibility forms, malformed JSON is surfaced as an explicit Luau error instead of being silently discarded, and the runtime comment, backend docs, and LSP knowledge all advertise the same `emit_json(value?)` contract. Phase 01 now meets its roadmap success criteria.

---

_Verified: 2026-05-01T17:50:41Z_
_Verifier: the agent (gsd-verifier)_
