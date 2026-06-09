---
phase: 98
slug: narrow-invalidation-event-routing
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-06-09
---

# Phase 98 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test` |
| **Config file** | `Cargo.toml` (workspace) |
| **Quick run command** | `cargo test -p mesh-core-shell -- invalidation` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p mesh-core-shell -- invalidation`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 98-01-01 | 01 | 1 | INV-01 | — | N/A | unit | `cargo test -p mesh-core-shell -- narrow_script` | ❌ W0 | ⬜ pending |
| 98-01-02 | 01 | 1 | INV-03 | — | N/A | unit | `cargo test -p mesh-core-shell -- threshold_fallback` | ❌ W0 | ⬜ pending |
| 98-02-01 | 02 | 2 | INV-02 | — | N/A | unit | `cargo test -p mesh-core-shell -- service_fanout` | ❌ W0 | ⬜ pending |
| 98-03-01 | 03 | 3 | INV-04, INV-05 | — | N/A | benchmark | `cargo test -p mesh-core-shell -- invalidation::profiling` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/core/shell/src/shell/component/tests/invalidation/narrow_script.rs` — stubs for INV-01, INV-03
- [ ] `crates/core/shell/src/shell/component/tests/invalidation/service_fanout.rs` — stubs for INV-02

*Existing infrastructure covers INV-04 and INV-05 (profiling.rs test file already exists).*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| None | — | — | — |

*All phase behaviors have automated verification.*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
