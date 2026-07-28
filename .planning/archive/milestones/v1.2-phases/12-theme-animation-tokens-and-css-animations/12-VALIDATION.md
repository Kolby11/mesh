---
phase: 12
slug: theme-animation-tokens-and-css-animations
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-08
---

# Phase 12 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `nix develop -c cargo test -p mesh-core-component keyframes && nix develop -c cargo test -p mesh-core-elements animation_token` |
| **Full suite command** | `nix develop -c cargo test -p mesh-core-component -p mesh-core-elements -p mesh-core-render -p mesh-core-shell animation` |
| **Estimated runtime** | ~90 seconds |

## Sampling Rate

- **After every task commit:** Run the task-specific quick command from the PLAN.md `<verify>` block.
- **After every plan wave:** Run `nix develop -c cargo test -p mesh-core-component -p mesh-core-elements -p mesh-core-render -p mesh-core-shell animation`
- **Before `$gsd-verify-work`:** Full suite must be green.
- **Max feedback latency:** 120 seconds.

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 12-01-01 | 01 | 1 | ANIM-01 | T-12-01 | Invalid token names do not silently alias old `motion.*` keys | unit/docs | `nix develop -c cargo test -p mesh-core-elements animation_token` | yes | pending |
| 12-01-02 | 01 | 1 | ANIM-02 | T-12-01 | CSS animation token references resolve only through explicit `token(...)` | unit/docs | `nix develop -c cargo test -p mesh-core-elements animation_token` | yes | pending |
| 12-02-01 | 02 | 1 | ANIM-03, ANIM-05 | T-12-02 | Unsupported keyframes fail closed at compile time | parser unit | `nix develop -c cargo test -p mesh-core-component keyframes` | yes | pending |
| 12-02-02 | 02 | 1 | ANIM-03, ANIM-05 | T-12-02 | Keyframe declarations preserve only validated transition-safe properties | parser unit | `nix develop -c cargo test -p mesh-core-component keyframes` | yes | pending |
| 12-03-01 | 03 | 2 | ANIM-03, ANIM-04 | T-12-03 | Playback honors fill/iteration/play-state without unbounded redraws | renderer unit | `nix develop -c cargo test -p mesh-core-render keyframe` | yes | pending |
| 12-03-02 | 03 | 2 | ANIM-04 | T-12-03 | Transition-safe interpolation remains shared/equivalent | renderer unit | `nix develop -c cargo test -p mesh-core-render keyframe` | yes | pending |
| 12-04-01 | 04 | 3 | ANIM-04, ANIM-05 | T-12-04 | Active animations continue by stable `_mesh_key` and emit runtime diagnostics | shell unit | `nix develop -c cargo test -p mesh-core-shell animation` | yes | pending |
| 12-04-02 | 04 | 3 | ANIM-04 | T-12-04 | Completed finite animations stop dirty repaint churn | shell unit | `nix develop -c cargo test -p mesh-core-shell animation` | yes | pending |
| 12-05-01 | 05 | 4 | ANIM-01, ANIM-02, ANIM-05 | T-12-05 | Docs/config expose the strict token and diagnostics contract | docs/grep | `grep -R "motion\\." config/themes docs/css-coverage.md docs/theming/themes.md docs/frontend/mesh-syntax.md` | yes | pending |

## Wave 0 Requirements

Existing Rust test infrastructure covers all phase requirements.

## Manual-Only Verifications

All Phase 12 behaviors have automated verification. Live visual animation feel remains useful for Phase 13 UAT, but Phase 12 acceptance is automated.

## Validation Sign-Off

- [x] All tasks have automated verify commands.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all required test infrastructure.
- [x] No watch-mode flags.
- [x] Feedback latency target under 120 seconds.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** pending
