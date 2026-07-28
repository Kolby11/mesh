---
phase: 13
slug: navigation-bar-rendering-proof
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-08
---

# Phase 13 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `nix develop -c cargo test -p mesh-core-shell navigation_bar` |
| **Full suite command** | `nix develop -c cargo test -p mesh-core-shell navigation_bar && nix develop -c cargo test -p mesh-core-shell keyframe_animation` |
| **Estimated runtime** | ~90 seconds |

## Sampling Rate

- **After every task commit:** Run the task-specific quick command from the PLAN.md `<verify>` block.
- **After every plan wave:** Run `nix develop -c cargo test -p mesh-core-shell navigation_bar && nix develop -c cargo test -p mesh-core-shell keyframe_animation`
- **Before `$gsd-verify-work`:** Full suite must be green.
- **Max feedback latency:** 120 seconds.

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 13-01-01 | 01 | 1 | NAV-01 | T-13-01 | Main surface preserves shell-owned layout and service wiring while adding richer status structure | shell/unit | `nix develop -c cargo test -p mesh-core-shell navigation_bar` | yes | pending |
| 13-01-02 | 01 | 1 | NAV-01 | T-13-01 | Status copy remains passive/selectable instead of becoming a disguised control | grep/shell | `grep -n 'selectable=\"true\"' modules/frontend/navigation-bar/src/main.mesh && nix develop -c cargo test -p mesh-core-shell navigation_bar` | yes | pending |
| 13-02-01 | 02 | 2 | NAV-01, NAV-02 | T-13-02 | Existing controls retain keyboard and activation semantics after layout migration | shell/unit | `nix develop -c cargo test -p mesh-core-shell navigation_bar` | yes | pending |
| 13-02-02 | 02 | 2 | NAV-01 | T-13-02 | Dormant status components are reused without introducing new feature surfaces | grep | `grep -n 'BatteryButton\\|MetaLabel\\|MetaPill' modules/frontend/navigation-bar/src/main.mesh modules/frontend/navigation-bar/COMPONENTS.md` | yes | pending |
| 13-03-01 | 03 | 3 | NAV-03, NAV-04 | T-13-03 | Selectable passive text and one bounded keyframe proof coexist without weakening control clarity | shell/unit | `nix develop -c cargo test -p mesh-core-shell navigation_bar && nix develop -c cargo test -p mesh-core-shell keyframe_animation` | yes | pending |
| 13-03-02 | 03 | 3 | NAV-04 | T-13-03 | Animation declarations use `animation.*` tokens and a custom keyframe on the shipped bar | grep | `grep -n 'animation:' modules/frontend/navigation-bar/src/main.mesh modules/frontend/navigation-bar/src/components/*.mesh` | yes | pending |
| 13-04-01 | 04 | 4 | NAV-01, NAV-05 | T-13-04 | Constrained-width behavior compresses secondary text before controls disappear | shell/unit | `nix develop -c cargo test -p mesh-core-shell navigation_bar` | yes | pending |
| 13-04-02 | 04 | 4 | NAV-05 | T-13-04 | Container-query restyles preserve runtime state and control presence | shell/unit | `nix develop -c cargo test -p mesh-core-shell navigation_bar` | yes | pending |
| 13-05-01 | 05 | 5 | NAV-02, NAV-05 | T-13-05 | Real-surface tests prove shipped behavior rather than fixture-only assumptions | shell/unit | `nix develop -c cargo test -p mesh-core-shell navigation_bar -- --nocapture` | yes | pending |
| 13-05-02 | 05 | 5 | NAV-01, NAV-03, NAV-04 | T-13-05 | Docs/module inventory describe the shipped richer proof surface accurately | grep/docs | `grep -n 'navigation bar\\|BatteryButton\\|selectable' modules/frontend/navigation-bar/COMPONENTS.md docs/frontend/mesh-syntax.md` | yes | pending |

## Wave 0 Requirements

Existing Rust shell test infrastructure covers all phase requirements.

## Manual-Only Verifications

The richer navigation-bar surface should still receive an optional human look-pass after execution, but all required Phase 13 acceptance behaviors have an automated verification path.

## Validation Sign-Off

- [x] All tasks have automated verify commands.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all required test infrastructure.
- [x] No watch-mode flags.
- [x] Feedback latency target under 120 seconds.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** pending
