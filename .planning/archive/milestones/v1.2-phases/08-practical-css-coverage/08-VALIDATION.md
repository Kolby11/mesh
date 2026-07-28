---
phase: 08
slug: practical-css-coverage
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-05
---

# Phase 08 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `nix develop -c cargo test -p mesh-core-elements style` |
| **Full suite command** | `nix develop -c cargo test -p mesh-core-component style && nix develop -c cargo test -p mesh-core-elements style && nix develop -c cargo test -p mesh-core-elements layout && nix develop -c cargo test -p mesh-tools-lsp css` |
| **Estimated runtime** | ~60 seconds |

---

## Sampling Rate

- **After every task commit:** Run the task's focused `cargo test` command.
- **After every plan wave:** Run `nix develop -c cargo test -p mesh-core-component style && nix develop -c cargo test -p mesh-core-elements style && nix develop -c cargo test -p mesh-core-elements layout && nix develop -c cargo test -p mesh-tools-lsp css`.
- **Before `$gsd-verify-work`:** Full suite must be green.
- **Max feedback latency:** 90 seconds.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 08-01-01 | 01 | 1 | CSS-01, CSS-03 | T-08-01 | Unsupported CSS reports deterministic diagnostics and does not crash runtime styling. | unit | `nix develop -c cargo test -p mesh-core-elements style_diagnostics` | ✅ | ✅ green |
| 08-01-02 | 01 | 1 | CSS-01, CSS-03 | T-08-02 | Supported property table prevents silent unsupported authoring. | unit | `nix develop -c cargo test -p mesh-core-elements supported_css` | ✅ | ✅ green |
| 08-02-01 | 02 | 2 | CSS-02, CSS-04 | T-08-03 | Shorthands resolve to explicit computed fields without panics. | unit | `nix develop -c cargo test -p mesh-core-elements shorthand` | ✅ | ✅ green |
| 08-02-02 | 02 | 2 | CSS-04 | T-08-04 | `var(...)` resolution is local, deterministic, and token-compatible. | unit | `nix develop -c cargo test -p mesh-core-elements css_variable` | ✅ | ✅ green |
| 08-03-01 | 03 | 3 | CSS-01, CSS-02 | T-08-05 | Layout and paint consume only supported concrete style fields. | unit | `nix develop -c cargo test -p mesh-core-elements layout` | ✅ | ✅ green |
| 08-04-01 | 04 | 4 | CSS-01, CSS-02, CSS-03 | T-08-06 | Animation declarations are metadata only; unsupported scheduling remains explicit. | unit | `nix develop -c cargo test -p mesh-core-elements animation` | ✅ | ✅ green |
| 08-05-01 | 05 | 5 | CSS-01, CSS-02, CSS-03, CSS-04 | T-08-07 | Docs and LSP expose the same supported subset. | unit/docs | `nix develop -c cargo test -p mesh-tools-lsp css` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠ flaky*

---

## Wave 0 Requirements

Existing Rust test infrastructure covers all phase requirements.

---

## Manual-Only Verifications

All Phase 8 behaviors have automated verification through parser, resolver, layout, render, LSP, or documentation checks.

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 90s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-05-05
