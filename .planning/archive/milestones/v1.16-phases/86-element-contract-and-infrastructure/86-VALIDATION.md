---
phase: 86
slug: element-contract-and-infrastructure
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-26
---

# Phase 86 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test` |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `cargo test -p mesh-core-elements -p mesh-core-component -p mesh-core-frontend element` |
| **Full suite command** | `cargo test -p mesh-core-elements -p mesh-core-component -p mesh-core-frontend` |
| **Estimated runtime** | ~60 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p mesh-core-elements -p mesh-core-component -p mesh-core-frontend element`
- **After every plan wave:** Run `cargo test -p mesh-core-elements -p mesh-core-component -p mesh-core-frontend`
- **Before `$gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 90 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 86-01-01 | 01 | 1 | ELEMCORE-01 | — | N/A | unit | `cargo test -p mesh-core-elements element_contract` | ✅ | ⬜ pending |
| 86-01-02 | 01 | 1 | ELEMCORE-03 | — | N/A | unit | `cargo test -p mesh-core-elements element_contract` | ✅ | ⬜ pending |
| 86-02-01 | 02 | 2 | ELEMCORE-02 | — | N/A | unit | `cargo test -p mesh-core-component -p mesh-core-frontend element_contract` | ✅ | ⬜ pending |
| 86-02-02 | 02 | 2 | ELEMCORE-04 | — | N/A | unit | `cargo test -p mesh-core-frontend element_contract` | ✅ | ⬜ pending |
| 86-03-01 | 03 | 3 | ELEMCORE-05 | — | N/A | unit | `cargo test -p mesh-core-component -p mesh-core-frontend element_diagnostic` | ✅ | ⬜ pending |
| 86-03-02 | 03 | 3 | ELEMCORE-06 | — | N/A | docs/source | `test -f docs/frontend/elements.md && grep -q "HTML" docs/frontend/elements.md && grep -q "Qt" docs/frontend/elements.md && grep -q "Flutter" docs/frontend/elements.md` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing Rust test infrastructure covers this phase. No Wave 0 scaffolding is required.

---

## Manual-Only Verifications

All Phase 86 behaviors have automated source or docs verification. Later UI behavior phases require visual and interaction UAT.

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 90s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-05-26
