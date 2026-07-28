---
phase: 02
slug: service-proxy-delivery
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-02
---

# Phase 02 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | Cargo workspace |
| **Quick run command** | `cargo test -p mesh-core-scripting context` |
| **Full suite command** | `cargo test -p mesh-core-scripting && cargo test -p mesh-core-service && cargo test -p mesh-core-shell` |
| **Estimated runtime** | ~120 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p mesh-core-scripting context` for proxy runtime work, `cargo test -p mesh-core-service` for contract parser/registry work, or `cargo test -p mesh-core-shell` for shell wiring work.
- **After every plan wave:** Run `cargo test -p mesh-core-scripting && cargo test -p mesh-core-service && cargo test -p mesh-core-shell`.
- **Before `$gsd-verify-work`:** Full suite must be green.
- **Max feedback latency:** 120 seconds.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 02-01-01 | 01 | 1 | PROXY-01, PROXY-06 | T-02-01 | Missing or invalid proxy lookups surface visible diagnostics with interface + version context | unit | `cargo test -p mesh-core-scripting context::tests::rejects_missing_interface_contract` | yes | pending |
| 02-01-02 | 01 | 1 | PROXY-02, PROXY-04, PROXY-05 | T-02-02 | Proxy field reads and dirty-state invalidation reflect every emitted payload without service callback APIs | unit | `cargo test -p mesh-core-scripting context` | yes | pending |
| 02-01-03 | 01 | 1 | PROXY-03 | T-02-03 | Contract method calls publish a command with the exact interface and payload | unit | `cargo test -p mesh-core-scripting context::tests::interface_proxy_method_publishes_service_command` | yes | pending |
| 02-02-01 | 02 | 2 | SURF-06 | T-02-04 | Audio/network/power/media contract metadata loads and exposes documented state fields and commands | unit | `cargo test -p mesh-core-service` | yes | pending |
| 02-02-02 | 02 | 2 | PROXY-03, SURF-06 | T-02-05 | Runtime/LSP/docs use the same read-and-command contract metadata surface | unit | `cargo test -p mesh-core-service && cargo test -p mesh-core-scripting context` | yes | pending |
| 02-03-01 | 03 | 3 | PROXY-01, PROXY-02, PROXY-04, PROXY-05 | T-02-06 | Built-in surfaces redraw from proxy updates without depending on service callback APIs | integration | `cargo test -p mesh-core-shell` | yes | pending |
| 02-03-02 | 03 | 3 | PROXY-03, PROXY-06 | T-02-07 | Built-in surfaces dispatch proxy commands and still surface lookup failures visibly | integration | `cargo test -p mesh-core-shell && cargo test -p mesh-core-scripting` | yes | pending |

*Status: pending, green, red, flaky*

---

## Wave 0 Requirements

Existing Rust test infrastructure covers all phase requirements.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Quick-settings and panel feel correct under live shell usage | PROXY-01, PROXY-02, PROXY-04, PROXY-05 | Software tests can prove updates and commands, but they do not confirm the full UX cadence of live audio/network providers | Start the shell, open panel and quick settings, confirm audio/network values change after backend emissions, and verify the UI reacts after repeated updates without any service-specific callback wiring |

---

## Validation Sign-Off

- [x] All tasks have automated verify commands or existing infrastructure.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all missing references.
- [x] No watch-mode flags.
- [x] Feedback latency < 120s.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** approved 2026-05-02
