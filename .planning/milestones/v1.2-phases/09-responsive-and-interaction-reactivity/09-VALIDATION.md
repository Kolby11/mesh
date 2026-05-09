---
phase: 09
slug: responsive-and-interaction-reactivity
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-05
---

# Phase 09 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test` |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `nix develop -c cargo test -p mesh-core-elements -p mesh-core-render -p mesh-core-shell responsive interaction restyle container` |
| **Full suite command** | `nix develop -c cargo test` |
| **Estimated runtime** | ~60-180 seconds |

## Sampling Rate

- **After every task commit:** Run `nix develop -c cargo test -p mesh-core-elements -p mesh-core-render -p mesh-core-shell responsive interaction restyle container`
- **After every plan wave:** Run `nix develop -c cargo test`
- **Before `$gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 180 seconds

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 09-01-01 | 01 | 1 | REACT-02 | T-09-01 | Runtime state cannot spoof unrelated node state | unit | `nix develop -c cargo test -p mesh-core-render interaction_state` | yes | pending |
| 09-01-02 | 01 | 1 | REACT-02 | T-09-01 | Focus/hover/active/checked styles follow stable keys | integration | `nix develop -c cargo test -p mesh-core-shell pseudo_state` | yes | pending |
| 09-02-01 | 02 | 2 | REACT-01 | T-09-02 | Size changes do not reload plugin runtimes | integration | `nix develop -c cargo test -p mesh-core-shell container_size_restyle` | yes | pending |
| 09-02-02 | 02 | 2 | REACT-01 | T-09-02 | Container query output uses current dimensions | unit | `nix develop -c cargo test -p mesh-core-render container_query` | yes | pending |
| 09-03-01 | 03 | 3 | REACT-03 | T-09-03 | Hit testing cannot use stale bounds after restyle | integration | `nix develop -c cargo test -p mesh-core-shell restyle_hit_test` | yes | pending |
| 09-03-02 | 03 | 3 | REACT-03 | T-09-03 | Metrics/accessibility use final layout | integration | `nix develop -c cargo test -p mesh-core-shell restyle_metrics accessibility` | yes | pending |
| 09-04-01 | 04 | 4 | REACT-04 | T-09-04 | Local user state survives non-semantic rebuilds | integration | `nix develop -c cargo test -p mesh-core-shell state_preservation_restyle` | yes | pending |
| 09-04-02 | 04 | 4 | REACT-04 | T-09-04 | Removed targets clear deterministically | integration | `nix develop -c cargo test -p mesh-core-shell restyle_state_cleanup` | yes | pending |

## Wave 0 Requirements

Existing infrastructure covers all phase requirements.

## Manual-Only Verifications

All phase behaviors have automated verification.

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 180s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-05-05
