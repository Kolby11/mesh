---
phase: 01
slug: plugin-package-manifest-foundation
status: draft
nyquist_compliant: true
wave_0_complete: false
created: 2026-05-03
---

# Phase 01 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | Cargo workspace |
| **Quick run command** | `nix develop -c cargo test -p mesh-core-plugin package` |
| **Full suite command** | `nix develop -c cargo test -p mesh-core-plugin -p mesh-core-shell plugin_package` |
| **Estimated runtime** | ~20 seconds |

## Sampling Rate

- **After every task commit:** Run `nix develop -c cargo test -p mesh-core-plugin package`
- **After every plan wave:** Run `nix develop -c cargo test -p mesh-core-plugin -p mesh-core-shell plugin_package`
- **Before `$gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 01-01-01 | 01 | 1 | PINST-01 | T-01-01 | rejects malformed package manifest | unit | `nix develop -c cargo test -p mesh-core-plugin package_manifest` | ✅ | ⬜ pending |
| 01-01-02 | 01 | 1 | PINST-03 | T-01-02 | rejects duplicate plugin IDs and empty backend categories | unit | `nix develop -c cargo test -p mesh-core-plugin package_manifest` | ✅ | ⬜ pending |
| 01-02-01 | 02 | 1 | PINST-02 | T-01-03 | exposes frontend backend-category dependencies | unit | `nix develop -c cargo test -p mesh-core-plugin package_graph` | ✅ | ⬜ pending |
| 01-02-02 | 02 | 1 | PINST-04 | T-01-04 | active provider must reference installed backend in same category | unit | `nix develop -c cargo test -p mesh-core-plugin package_graph` | ✅ | ⬜ pending |
| 01-03-01 | 03 | 2 | PINST-05 | T-01-05 | shell loads graph from default package manifest path | integration unit | `nix develop -c cargo test -p mesh-core-shell plugin_package` | ✅ | ⬜ pending |
| 01-03-02 | 03 | 2 | PINST-06 | — | no remote download/signing behavior required | static | `grep -R "marketplace\\|download\\|signature" crates/core/extension/plugin/src crates/core/shell/src` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

## Wave 0 Requirements

Existing infrastructure covers all phase requirements.

## Manual-Only Verifications

All phase behaviors have automated verification.

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 30s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-05-03
