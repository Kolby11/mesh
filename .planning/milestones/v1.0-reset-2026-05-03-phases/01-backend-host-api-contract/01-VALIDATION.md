---
phase: 01
slug: backend-host-api-contract
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-01
---

# Phase 01 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | Cargo workspace |
| **Quick run command** | `cargo test -p mesh-core-scripting backend` |
| **Full suite command** | `cargo test -p mesh-core-scripting && cargo test -p mesh-core-backend` |
| **Estimated runtime** | ~60 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p mesh-core-scripting backend` for scripting runtime changes or `cargo test -p mesh-core-backend` for backend loop changes.
- **After every plan wave:** Run `cargo test -p mesh-core-scripting && cargo test -p mesh-core-backend`.
- **Before `$gsd-verify-work`:** Full suite must be green.
- **Max feedback latency:** 60 seconds.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 01-01-01 | 01 | 1 | HOST-01 | T-01-01 | Structured exec does not invoke shell unless `exec_shell` is used | unit | `cargo test -p mesh-core-scripting backend::tests::exec_accepts_program_and_args` | yes | pending |
| 01-01-02 | 01 | 1 | HOST-03 | T-01-02 | Plugin receives only its own settings table | unit | `cargo test -p mesh-core-scripting backend::tests::config_returns_backend_settings` | yes | pending |
| 01-01-03 | 01 | 1 | HOST-04 | T-01-03 | Logging does not panic on valid levels | unit | `cargo test -p mesh-core-scripting backend::tests::log_level_function_and_aliases_are_callable` | yes | pending |
| 01-01-04 | 01 | 1 | HOST-05 | T-01-04 | Bad emits do not leak stale payloads | unit | `cargo test -p mesh-core-scripting backend::tests::bad_emit_payload_does_not_emit_stale_state` | yes | pending |
| 01-02-01 | 02 | 2 | HOST-06 | T-01-05 | Poll interval changes are bounded and do not busy-loop | integration | `cargo test -p mesh-core-backend` | yes | pending |
| 01-02-02 | 02 | 2 | HOST-01,HOST-02,HOST-05 | T-01-06 | Bundled scripts keep using explicit host APIs | integration | `cargo test -p mesh-core-scripting && cargo test -p mesh-core-backend` | yes | pending |

*Status: pending, green, red, flaky*

---

## Wave 0 Requirements

Existing Rust test infrastructure covers all phase requirements.

---

## Manual-Only Verifications

All phase behaviors have automated verification.

---

## Validation Sign-Off

- [x] All tasks have automated verify commands or existing infrastructure.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all missing references.
- [x] No watch-mode flags.
- [x] Feedback latency < 60s.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** approved 2026-05-01
