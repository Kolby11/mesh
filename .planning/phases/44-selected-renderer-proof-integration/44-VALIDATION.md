---
phase: 44
slug: selected-renderer-proof-integration
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-18
---

# Phase 44 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `cargo test -p mesh-core-render proof` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~180 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p mesh-core-render proof` for render-crate tasks or `cargo test -p mesh-core-shell phase44` for shell/component tasks.
- **After every plan wave:** Run `cargo test -p mesh-core-render proof` and `cargo test -p mesh-core-shell phase44`.
- **Before `$gsd-verify-work`:** Full suite must be green with `cargo test --workspace`.
- **Max feedback latency:** 180 seconds for focused commands.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 44-01-01 | 01 | 1 | INTG-01 | T-44-01-01 | N/A | unit | `cargo test -p mesh-core-render proof` | yes | pending |
| 44-01-02 | 01 | 1 | INTG-01, INTG-04 | T-44-01-02 | N/A | unit | `cargo test -p mesh-core-render proof` | yes | pending |
| 44-02-01 | 02 | 2 | INTG-01 | T-44-02-01 | Non-fatal diagnostics only | integration | `cargo test -p mesh-core-shell phase44` | yes | pending |
| 44-02-02 | 02 | 2 | INTG-01 | T-44-02-02 | N/A | integration | `cargo test -p mesh-core-shell phase44` | yes | pending |
| 44-03-01 | 03 | 2 | INTG-03 | T-44-03-01 | N/A | unit | `cargo test -p mesh-core-render proof` | yes | pending |
| 44-03-02 | 03 | 2 | INTG-04 | T-44-03-02 | N/A | unit | `cargo test -p mesh-core-render proof` | yes | pending |
| 44-04-01 | 04 | 3 | INTG-02 | T-44-04-01 | N/A | integration | `cargo test -p mesh-core-shell navigation` | yes | pending |
| 44-04-02 | 04 | 3 | INTG-01, INTG-02, INTG-03, INTG-04 | T-44-04-02 | N/A | evidence | `rg -n "INTG-01|INTG-02|INTG-03|INTG-04" .planning/phases/44-selected-renderer-proof-integration/44-INTEGRATION-EVIDENCE.md` | no | pending |

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements.

---

## Manual-Only Verifications

All phase behaviors have automated verification.

---

## Validation Sign-Off

- [x] All tasks have automated verify commands or existing infrastructure.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all MISSING references.
- [x] No watch-mode flags.
- [x] Feedback latency target is below 180 seconds for focused commands.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** approved 2026-05-18
