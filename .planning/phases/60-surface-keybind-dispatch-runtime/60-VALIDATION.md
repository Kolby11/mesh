---
phase: 60
slug: surface-keybind-dispatch-runtime
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-23
---

# Phase 60 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test` |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts` |
| **Full suite command** | `nix develop -c cargo test -p mesh-core-shell shell::component::tests::interaction::navigation` |
| **Estimated runtime** | ~60 seconds |

---

## Sampling Rate

- **After every task commit:** Run `nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts`
- **After every plan wave:** Run `nix develop -c cargo test -p mesh-core-shell shell::component::tests::interaction::navigation`
- **Before `$gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 90 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 60-01-01 | 01 | 1 | KDISP-01, KDISP-03 | T-60-01 / T-60-03 | Manifest actions dispatch only through explicit subscribers | unit | `nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts` | yes | pending |
| 60-01-02 | 01 | 1 | KDISP-02 | T-60-02 | Bare printable keybinds do not steal focused text input | unit | `nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts` | yes | pending |
| 60-01-03 | 01 | 1 | KDISP-04 | T-60-04 | Shipped navigation surface proves manifest-owned dispatch | integration | `nix develop -c cargo test -p mesh-core-shell navigation_bar_keyboard_shortcut_and_theme_activation_work_on_real_surface` | yes | pending |

*Status: pending / green / red / flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements.

---

## Manual-Only Verifications

All phase behaviors have automated verification.

---

## Validation Sign-Off

- [x] All tasks have automated verify commands
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all missing references
- [x] No watch-mode flags
- [x] Feedback latency < 90s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-05-23
