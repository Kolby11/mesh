---
phase: 70
slug: localized-text-manifest-model
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-24
---

# Phase 70 — Validation Strategy

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust cargo test |
| **Config file** | `crates/core/extension/module/Cargo.toml` |
| **Quick run command** | `cargo test -p mesh-core-module manifest_localized_text -- --nocapture` |
| **Full suite command** | `cargo test -p mesh-core-module manifest -- --nocapture` |
| **Estimated runtime** | ~20 seconds |

## Sampling Rate

- **After every task commit:** Run `cargo test -p mesh-core-module manifest_localized_text -- --nocapture`
- **After every plan wave:** Run `cargo test -p mesh-core-module manifest -- --nocapture`
- **Before `$gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 70-01-01 | 01 | 1 | MI18N-01, MI18N-02 | T-70-01 | Invalid localized text cannot silently enter runtime metadata | unit | `cargo test -p mesh-core-module manifest_localized_text -- --nocapture` | ✅ | pending |
| 70-01-02 | 01 | 1 | MI18N-01, MI18N-03 | T-70-02 | Existing raw-string manifests remain compatible | unit | `cargo test -p mesh-core-module parses_module_json_keybind_display_keys -- --nocapture` | ✅ | pending |
| 70-01-03 | 01 | 1 | MI18N-04 | T-70-03 | Suspicious raw i18n keys produce non-fatal migration diagnostics | unit | `cargo test -p mesh-core-module manifest_localized_text -- --nocapture` | ✅ | pending |

## Wave 0 Requirements

Existing infrastructure covers all phase requirements.

## Manual-Only Verifications

All phase behaviors have automated verification.

## Validation Sign-Off

- [x] All tasks have automated verify commands
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all missing references
- [x] No watch-mode flags
- [x] Feedback latency < 30s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-05-24
