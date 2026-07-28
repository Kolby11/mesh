---
phase: 03
slug: backend-host-api-contract
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-03
---

# Phase 03 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness with Tokio tests |
| **Config file** | `Cargo.toml` workspace and package-level `Cargo.toml` files |
| **Quick run command** | `nix develop -c cargo test -p mesh-core-scripting backend` |
| **Full suite command** | `nix develop -c cargo test -p mesh-core-scripting backend && nix develop -c cargo test -p mesh-core-backend spawn_backend_service` |
| **Estimated runtime** | ~90 seconds |

---

## Sampling Rate

- **After every task commit:** Run the plan-local quick command.
- **After every plan wave:** Run the full suite command above.
- **Before `$gsd-verify-work`:** Full suite must be green.
- **Max feedback latency:** 90 seconds.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 03-01-01 | 01 | 1 | BHOST-01 | T-03-01 | Structured `mesh.exec` only; no shell token splitting | unit | `nix develop -c cargo test -p mesh-core-scripting backend` | yes | pending |
| 03-01-02 | 01 | 1 | BHOST-01 | T-03-02 | Process failures return result tables | unit | `nix develop -c cargo test -p mesh-core-scripting backend` | yes | pending |
| 03-01-03 | 01 | 1 | BHOST-02 | T-03-03 | `mesh.exec_shell` not exposed as public API | unit + grep | `nix develop -c cargo test -p mesh-core-scripting backend` | yes | pending |
| 03-02-01 | 02 | 2 | BHOST-01, BHOST-02 | T-03-04 | Bundled providers use structured process args | bundled script tests + grep | `nix develop -c cargo test -p mesh-core-scripting bundled_backend_scripts_expose_required_host_api_surface` | yes | pending |
| 03-02-02 | 02 | 2 | BHOST-01, BHOST-02 | T-03-05 | Provider parsing stays in Luau, not Rust service branches | unit + grep | `nix develop -c cargo test -p mesh-core-scripting backend` | yes | pending |
| 03-03-01 | 03 | 2 | BHOST-03 | T-03-06 | `mesh.config()` returns full nested settings table | unit | `nix develop -c cargo test -p mesh-core-scripting config_returns_backend_settings` | yes | pending |
| 03-03-02 | 03 | 2 | BHOST-04 | T-03-07 | `mesh.log` supports fixed levels and both call styles | unit | `nix develop -c cargo test -p mesh-core-scripting log_level_function_and_aliases_are_callable` | yes | pending |
| 03-04-01 | 04 | 3 | BHOST-05 | T-03-08 | Low poll intervals clamp to 50ms and warn | unit | `nix develop -c cargo test -p mesh-core-scripting poll_interval` | yes | pending |
| 03-04-02 | 04 | 3 | BHOST-05 | T-03-09 | Interval changes apply after current callback | async unit | `nix develop -c cargo test -p mesh-core-backend spawn_backend_service` | yes | pending |

*Status: pending until execution.*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements. No new test framework or harness installation is required.

---

## Manual-Only Verifications

All phase behaviors have automated verification.

---

## Validation Sign-Off

- [x] All tasks have automated verify commands or existing Wave 0 infrastructure.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all MISSING references.
- [x] No watch-mode flags.
- [x] Feedback latency target is under 90 seconds for focused runs.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** approved 2026-05-03
