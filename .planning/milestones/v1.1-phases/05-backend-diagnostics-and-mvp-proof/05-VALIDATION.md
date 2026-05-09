---
phase: 05
slug: backend-diagnostics-and-mvp-proof
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-04
---

# Phase 05 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness with Tokio tests |
| **Config file** | `Cargo.toml` workspace and package-level `Cargo.toml` files |
| **Quick run command** | `nix develop -c cargo test -p mesh-core-shell backend_lifecycle` |
| **Full suite command** | `nix develop -c cargo test -p mesh-core-scripting backend && nix develop -c cargo test -p mesh-core-backend backend && nix develop -c cargo test -p mesh-core-shell backend_lifecycle && nix develop -c cargo test -p mesh-core-diagnostics lifecycle` |
| **Estimated runtime** | ~150 seconds |

---

## Sampling Rate

- **After every task commit:** Run the plan-local quick command.
- **After every plan wave:** Run the full suite command above.
- **Before `$gsd-verify-work`:** Full suite must be green.
- **Max feedback latency:** 150 seconds.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 05-01-01 | 01 | 1 | BDIAG-01, BDIAG-02 | T-05-01 | Runtime failure stages are surfaced distinctly and do not silently collapse load/init/poll/command/state-snapshot errors | unit | `nix develop -c cargo test -p mesh-core-scripting backend_command_result` | yes | pending |
| 05-01-02 | 01 | 1 | BDIAG-02 | T-05-02 | Repeated poll failures degrade first and stop cleanly without crashing the shell | async unit | `nix develop -c cargo test -p mesh-core-backend backend` | yes | pending |
| 05-02-01 | 02 | 2 | BDIAG-02, BDIAG-03 | T-05-03 | Active-provider failures clear stale public state and update runtime status instead of leaving last-known-good payloads visible | unit | `nix develop -c cargo test -p mesh-core-shell backend_lifecycle` | yes | pending |
| 05-02-02 | 02 | 2 | BDIAG-03, BDIAG-04 | T-05-04 | Diagnostics deduplicate by provider plus stage while updating count/timestamp metadata | unit | `nix develop -c cargo test -p mesh-core-diagnostics lifecycle` | yes | pending |
| 05-03-01 | 03 | 3 | BREF-01 | T-05-05 | Fresh reference provider exercises config, logging, poll interval, exported state, and command handlers | bundled script tests | `nix develop -c cargo test -p mesh-core-scripting bundled_` | yes | pending |
| 05-03-02 | 03 | 3 | BREF-02 | T-05-06 | Reference provider commands emit result tables and updated service state through the public backend contract | async unit | `nix develop -c cargo test -p mesh-core-backend reference_media` | yes | pending |
| 05-04-01 | 04 | 4 | BREF-03 | T-05-07 | Backend author docs describe exported `state`, strict `mesh.exec`, explicit provider selection, and the new reference provider | grep + doc read | `! grep -R -n "mesh.exec_shell\\|next candidate is tried\\|mesh.service.emit(data)" docs/plugins/backend/core docs/extensibility.md` | yes | pending |
| 05-04-02 | 04 | 4 | BREF-02, BREF-03 | T-05-08 | Reference note names the exact provider files and command/result/state flow proven by tests | grep + targeted tests | `nix develop -c cargo test -p mesh-core-backend reference_media && grep -R -n "@mesh/reference-media\\|reference-media" docs/plugins/backend/core packages/plugins/backend/core/reference-media` | yes | pending |

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
- [x] Feedback latency target is under 150 seconds for focused runs.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** approved 2026-05-04
