---
phase: 03-backend-host-api-contract
verified: 2026-05-03T19:02:32Z
status: passed
score: 5/5 must-haves verified
overrides_applied: 1
overrides:
  - must_have: "BHOST-02: Backend Luau scripts can call mesh.exec_shell(cmd) for shell-style commands and receive stdout, stderr, and exit status."
    reason: "Phase 03 user decision changed the MVP shell execution scope: remove exec_shell and migrate bundled providers to structured mesh.exec(program, args). The decision is recorded in 03-DISCUSSION-LOG.md and 03-CONTEXT.md."
    accepted_by: "user decision recorded in 03-DISCUSSION-LOG.md"
    accepted_at: "2026-05-03T00:00:00Z"
---

# Phase 3: Backend Host API Contract Verification Report

**Phase Goal:** Lock backend host APIs for the backend plugin MVP: strict structured process execution, migrated bundled providers, stable config/log contract, and bounded poll interval behavior.
**Verified:** 2026-05-03T19:02:32Z
**Status:** passed
**Re-verification:** No - initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `mesh.exec(program, args)` is strict structured-only and returns stdout, stderr, success, and exit status. | VERIFIED | `BackendScriptContext::install_host_api()` binds `(String, Vec<String>)`; `run_exec()` uses `StdCommand::new(program).args(args)`; result conversion sets `success`, `stdout`, `stderr`, and `code`. Tests cover accepted structured args, rejected single-string form, missing program, and non-zero exit. |
| 2 | Shell-style execution is removed from the MVP backend public API and bundled providers are migrated. | PASSED (override) | Roadmap/requirements still mention `mesh.exec_shell`, but Phase 03 discussion chose "Remove from MVP". Code does not register `exec_shell`; bundled host API test asserts it is absent; provider scripts call structured `mesh.exec`. |
| 3 | `mesh.config()` returns the backend plugin's full settings table with no Phase 3 lookup helpers. | VERIFIED | `mesh.config` returns `runtime.settings` via `lua.to_value`; `config_returns_backend_settings` asserts nested fields and array values; `host_api.rs` documents only `mesh.config()`. |
| 4 | `mesh.log(level, msg)` and named log methods produce plugin-scoped structured logs with stable levels and non-fatal invalid levels. | VERIFIED | `log_message()` emits tracing entries with `plugin_id`; public `debug/info/warn/error` methods are registered; tests cover generic and named call styles plus invalid `trace` continuing to emit payload. |
| 5 | `mesh.service.set_poll_interval(ms)` is bounded and affects subsequent backend polling after callbacks return. | VERIFIED | Scripting host clamps below `MIN_POLL_INTERVAL_MS = 50` and warns; backend runtime defensively uses `.max(MIN_POLL_INTERVAL_MS)`; runtime tests cover poll and command-handler interval changes after callback return. |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/core/runtime/scripting/src/backend.rs` | Backend Luau host API registration, result conversion, config/log/poll tests | VERIFIED | `gsd-sdk verify.artifacts` passed for plans 01, 03, and 04; manual inspection found substantive implementations and tests. |
| `crates/core/runtime/scripting/src/host_api.rs` | Shared host API comments aligned with public backend API | VERIFIED | Documents `mesh.exec(program, args)`, `mesh.config()`, `mesh.log`, and poll interval; no `config.get` or public `exec_shell` docs. |
| `crates/core/runtime/backend/src/lib.rs` | Runtime poll interval refresh behavior | VERIFIED | Contains `MIN_POLL_INTERVAL_MS`, `bounded_poll_interval_ms`, and `refresh_interval` calls after poll/command paths. |
| `packages/plugins/backend/core/pipewire-audio/src/main.luau` | Structured PipeWire commands | VERIFIED | Uses `mesh.exec("wpctl", {...})` and no `exec_shell`. |
| `packages/plugins/backend/core/pulseaudio-audio/src/main.luau` | Structured PulseAudio commands | VERIFIED | Uses `mesh.exec("pactl", {...})` and no `exec_shell`. |
| `packages/plugins/backend/core/networkmanager-network/src/main.luau` | Structured NetworkManager/Bluetooth commands | VERIFIED | Uses `mesh.exec("nmcli", {...})` and `mesh.exec("bluetoothctl", {...})`; no `exec_shell`. |
| `packages/plugins/backend/core/upower-power/src/main.luau` | Structured UPower command and Luau parsing | VERIFIED | Uses `mesh.exec("upower", {...})`, includes `time_remaining_minutes`, and no `exec_shell`. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| Backend Luau `mesh.exec` | OS process execution | `install_host_api()` -> `run_exec()` -> `StdCommand::new(program).args(args)` | WIRED | Response is converted through one Lua table path. |
| Provider scripts | Structured host API | `mesh.exec(program, args)` calls in audio/network/power Luau files | WIRED | Static grep found no provider `mesh.exec_shell` calls. |
| Backend config settings | Luau script API | `new_with_settings*` -> `runtime.settings` -> `mesh.config()` | WIRED | Contract test emits nested settings from Lua. |
| Backend logs | Structured tracing | `mesh.log` callable/table methods -> `log_message(plugin_id, level, message)` | WIRED | Unknown levels warn rather than throw. |
| Poll interval setter | Backend runtime cadence | `set_poll_interval` -> `ctx.poll_interval_ms()` -> `refresh_interval()` | WIRED | Refresh occurs after poll and command callbacks. |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|--------------------|--------|
| `backend.rs` | exec result table | `StdCommand::output()` -> `ExecOutcome` -> Lua table | Yes | FLOWING |
| `backend.rs` | config table | `runtime.settings` supplied at context construction | Yes | FLOWING |
| `backend.rs` | log event | Luau `mesh.log*` call -> `tracing::*` with plugin id | Yes | FLOWING |
| `backend.rs` / `lib.rs` | poll interval | Luau setter -> runtime mutex -> backend interval refresh | Yes | FLOWING |
| bundled provider scripts | service payloads | external command stdout parsed in Luau, unavailable path on failure | Yes in code path | FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full workspace build | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo build` | Orchestrator evidence: passed after all phase changes | PASS |
| Full workspace tests | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test` | Orchestrator evidence: passed after all phase changes | PASS |
| Artifact verification | `gsd-sdk query verify.artifacts` for plans 01-04 | 10/10 declared artifacts passed | PASS |
| Schema drift | orchestrator schema drift check | `drift_detected=false` | PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| BHOST-01 | 03-01, 03-02 | Backend scripts can call structured `mesh.exec(cmd, args)` and receive stdout, stderr, and exit status. | SATISFIED | Strict structured binding, process result tests, and migrated bundled providers. |
| BHOST-02 | 03-01, 03-02 | REQUIREMENTS.md says `mesh.exec_shell(cmd)` exists. | SATISFIED BY OVERRIDE | User decision removed this from MVP; code asserts absence and providers no longer depend on it. |
| BHOST-03 | 03-03 | Backend scripts can call `mesh.config()` and receive plugin settings as a Luau table. | SATISFIED | `mesh.config()` returns runtime settings; nested config test passes in code. |
| BHOST-04 | 03-03 | Backend scripts can call `mesh.log(level, msg)` and produce plugin-scoped structured logs. | SATISFIED | Callable log table and named methods call `log_message()` with plugin id. |
| BHOST-05 | 03-04 | Backend scripts can call `mesh.service.set_poll_interval(ms)` and affect future poll cadence without shell restart. | SATISFIED | Host clamp tests and backend runtime cadence tests cover poll and command callbacks. |

No orphaned Phase 3 requirements were found in `.planning/REQUIREMENTS.md`; BHOST-01 through BHOST-05 all map to Phase 3 and all appear in plan frontmatter.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| None in phase-modified files | - | - | - | Stub/placeholder scans found no blocking anti-patterns in the verified phase files. |

### Human Verification Required

None. This phase locks backend API contracts and has automated code/test evidence for the required behavior.

### Gaps Summary

No blocking gaps found. The only scope mismatch is the stale BHOST-02 wording in `.planning/REQUIREMENTS.md` and `.planning/ROADMAP.md`; it is covered by the recorded Phase 03 user decision to remove `mesh.exec_shell` from the MVP and migrate callers to structured `mesh.exec`.

---

_Verified: 2026-05-03T19:02:32Z_
_Verifier: the agent (gsd-verifier)_
