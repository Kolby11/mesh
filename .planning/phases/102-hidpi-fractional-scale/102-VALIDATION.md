---
phase: 102
slug: hidpi-fractional-scale
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-06-10
---

# Phase 102 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | Cargo.toml workspace |
| **Quick run command** | `nix develop -c cargo check -p mesh-core-presentation` |
| **Full suite command** | `nix develop -c cargo test -p mesh-core-presentation -- --test-threads=1` |
| **Estimated runtime** | ~15 seconds |

---

## Per-task Verification Map

| task ID | Plan | Wave | Requirement | Threat Ref | Test Type | Automated Command |
|---------|------|------|-------------|------------|-----------|-------------------|
| 102-01-01 | 01 | 1 | HDPI-01 | T-102-01, T-102-02 | check | `nix develop -c cargo check -p mesh-core-presentation` |
| 102-01-02 | 01 | 1 | HDPI-04, HDPI-05 | T-102-01, T-102-02 | check | `nix develop -c cargo check -p mesh-core-presentation` |
| 102-01-03 | 01 | 1 | HDPI-01, HDPI-04, HDPI-05 | T-102-03, T-102-04 | test | `nix develop -c cargo test -p mesh-core-presentation -- --test-threads=1` |
| 102-02-01 | 02 | 2 | HDPI-02 | T-102-05 | check | `nix develop -c cargo check --workspace` |
| 102-02-02 | 02 | 2 | HDPI-03, HDPI-04 | T-102-06, T-102-07, T-102-08 | check | `nix develop -c cargo check -p mesh-core-presentation` |
| 102-02-03 | 02 | 2 | HDPI-02, HDPI-03, HDPI-04 | T-102-05, T-102-06 | test | `nix develop -c cargo test -p mesh-core-presentation -- --test-threads=1 && nix develop -c cargo check --workspace` |

---

## Wave 0 Requirements

Existing test infrastructure covers all phase requirements. No new test framework installation needed.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| HiDPI sharpness at 2× | HDPI-01 | Requires physical HiDPI display | Run MESH on a 2× display, verify text/icons are sharp |
| Fractional scale wp_viewporter | HDPI-02 | Requires compositor with wp_fractional_scale_v1 | Run on KDE with 150% scaling, verify no visual artifacts |
| Monitor hotplug scale change | HDPI-03 | Requires physical display hotplug | Plug/unplug HiDPI monitor, verify smooth resize |
| Integer fallback without wp_fractional_scale_v1 | HDPI-04 | Requires non-KDE compositor | Run on Sway with `scale=2` output config, verify sharp rendering |
| No-viewporter fallback | HDPI-05 | Requires compositor without wp_viewporter | Run on basic wlroots compositor, verify no protocol errors |

---

## Validation Sign-Off

- [x] All tasks have automated verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 30s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
