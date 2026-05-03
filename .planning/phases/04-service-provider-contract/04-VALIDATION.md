---
phase: 04
slug: service-provider-contract
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-03
---

# Phase 04 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness with Tokio tests |
| **Config file** | `Cargo.toml` workspace and package-level `Cargo.toml` files |
| **Quick run command** | `nix develop -c cargo test -p mesh-core-scripting backend` |
| **Full suite command** | `nix develop -c cargo test -p mesh-core-scripting backend && nix develop -c cargo test -p mesh-core-backend spawn_backend_service && nix develop -c cargo test -p mesh-core-shell service_contract` |
| **Estimated runtime** | ~120 seconds |

---

## Sampling Rate

- **After every task commit:** Run the plan-local quick command.
- **After every plan wave:** Run the full suite command above.
- **Before `$gsd-verify-work`:** Full suite must be green.
- **Max feedback latency:** 120 seconds.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 04-01-01 | 01 | 1 | BSVC-02 | T-04-01 | Backend exported state snapshots cannot be confused with provider identity metadata | unit | `nix develop -c cargo test -p mesh-core-scripting backend_state` | yes | pending |
| 04-01-02 | 01 | 1 | BSVC-02, BSVC-05 | T-04-02 | Command handlers return result data without losing state updates | async unit | `nix develop -c cargo test -p mesh-core-backend command_result` | yes | pending |
| 04-02-01 | 02 | 2 | BSVC-01, BSVC-03 | T-04-03 | Latest state is keyed by interface and provider id remains metadata | unit | `nix develop -c cargo test -p mesh-core-shell service_contract` | yes | pending |
| 04-02-02 | 02 | 2 | BSVC-01 | T-04-04 | Contract mismatches warn instead of adding service-specific core branches | unit | `nix develop -c cargo test -p mesh-core-shell service_contract` | yes | pending |
| 04-03-01 | 03 | 3 | BSVC-03, BSVC-04, BSVC-05 | T-04-05 | `require("@mesh/audio").state` reads reactive state and command dispatch returns a result table | unit | `nix develop -c cargo test -p mesh-core-scripting interface_proxy` | yes | pending |
| 04-03-02 | 03 | 3 | BSVC-04, BSVC-05 | T-04-06 | Unauthorized or unsupported commands return visible failure results | unit | `nix develop -c cargo test -p mesh-core-scripting interface_proxy` | yes | pending |
| 04-04-01 | 04 | 4 | BSVC-02, BSVC-03 | T-04-07 | Bundled providers export top-level `state` and do not inject `source_plugin` | bundled script tests + grep | `nix develop -c cargo test -p mesh-core-scripting bundled_backend_scripts_expose_required_host_api_surface` | yes | pending |
| 04-04-02 | 04 | 4 | BSVC-04, BSVC-05 | T-04-08 | Bundled command handlers update state and return result tables where command outcomes matter | runtime tests + grep | `nix develop -c cargo test -p mesh-core-backend backend_command` | yes | pending |

*Status: pending until execution.*

---

## Wave 0 Requirements

Existing Rust test infrastructure covers all phase requirements. No new test framework or harness installation is required.

---

## Manual-Only Verifications

All phase behaviors have automated verification.

---

## Validation Sign-Off

- [x] All tasks have automated verify commands or existing Wave 0 infrastructure.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all MISSING references.
- [x] No watch-mode flags.
- [x] Feedback latency target is under 120 seconds for focused runs.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** approved 2026-05-03
