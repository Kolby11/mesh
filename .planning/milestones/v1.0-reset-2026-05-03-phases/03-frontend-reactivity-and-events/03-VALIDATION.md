---
phase: 03
slug: frontend-reactivity-and-events
status: approved
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-02
---

# Phase 03 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test` |
| **Config file** | `Cargo.toml` workspace |
| **Quick run command** | `cargo test -p mesh-core-scripting context` |
| **Full suite command** | `cargo test -p mesh-core-scripting context && cargo test -p mesh-core-shell && cargo test -p mesh-core-render` |
| **Estimated runtime** | unknown in this environment; run inside `nix develop` if needed |

---

## Sampling Rate

- **After every task commit:** Run the task's listed `cargo test` command or the narrowest package command that covers the touched files.
- **After every plan wave:** Run `cargo test -p mesh-core-scripting context && cargo test -p mesh-core-shell && cargo test -p mesh-core-render`.
- **Before `$gsd-verify-work`:** Full suite must be green.
- **Max feedback latency:** one package-level test command per task.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 03-01-01 | 01 | 1 | FRONT-01 | T-03-01 | No stale UI after real reactive change | unit | `cargo test -p mesh-core-scripting context` | yes | pending |
| 03-01-02 | 01 | 1 | FRONT-02 | T-03-02 | Rebuild only follows dirty script state or redraw flag | unit/integration | `cargo test -p mesh-core-shell` | yes | pending |
| 03-02-01 | 02 | 2 | FRONT-03 | T-03-03 | Click handlers keep current state and event payload | unit/integration | `cargo test -p mesh-core-shell` | yes | pending |
| 03-02-02 | 02 | 2 | FRONT-04 | T-03-04 | Change/release/focus values are typed and bounded | unit/integration | `cargo test -p mesh-core-shell && cargo test -p mesh-core-render` | yes | pending |
| 03-02-03 | 02 | 2 | FRONT-05 | T-03-05 | Handler errors are diagnostic, not fatal render crashes | unit/integration | `cargo test -p mesh-core-shell` | yes | pending |
| 03-03-01 | 03 | 3 | FRONT-01..FRONT-05 | T-03-06 | Real proof component exercises event/state/render path | integration/grep | `cargo test -p mesh-core-shell` | yes | pending |

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Visual feel of inline navigation-bar volume slider | FRONT-04 | Automated tests can prove event flow but not whether the compact control feels acceptable in the real panel | Run shell in dev environment, drag the inline slider, confirm no overlap/clipping and tooltip/icon update behavior remains readable |

---

## Validation Sign-Off

- [x] All tasks have automated verify commands or existing test infrastructure.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all missing references.
- [x] No watch-mode flags.
- [x] Feedback latency bounded by package-level tests.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** approved 2026-05-02
