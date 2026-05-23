---
phase: 56
slug: animation-and-transition-paint-integration
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-23
---

# Phase 56 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | Cargo workspace |
| **Quick run command** | `nix develop -c cargo test -p mesh-core-shell animation -- --nocapture` |
| **Full suite command** | `nix develop -c cargo test -p mesh-core-shell animation -- --nocapture && nix develop -c cargo test -p mesh-core-shell shipped_navigation -- --nocapture && nix develop -c cargo test -p mesh-core-render render_object_tree_marks -- --nocapture` |
| **Estimated runtime** | ~120 seconds after dependencies are built |

---

## Sampling Rate

- **After every task commit:** Run that task's focused `nix develop -c cargo test ...` command.
- **After every plan wave:** Run the full suite command.
- **Before `$gsd-verify-work`:** Full suite and backend-neutrality grep must be green.
- **Max feedback latency:** 180 seconds for focused tests.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 56-01-01 | 01 | 1 | ANIM-01, ANIM-02 | T-56-01 | Animation bucket classification remains explicit and diagnostic-friendly | unit | `nix develop -c cargo test -p mesh-core-shell animation_property_bucket -- --nocapture` | ✅ | ⬜ pending |
| 56-01-02 | 01 | 1 | ANIM-01 | T-56-02 | Existing token/keyframe parsing remains compatible | unit | `nix develop -c cargo test -p mesh-core-elements animation -- --nocapture` | ✅ | ⬜ pending |
| 56-02-01 | 02 | 2 | ANIM-02 | T-56-03 | Paint-only transitions avoid full layout | unit | `nix develop -c cargo test -p mesh-core-shell animation_transition_dirty -- --nocapture` | ✅ | ⬜ pending |
| 56-02-02 | 02 | 2 | ANIM-02 | T-56-03 | Layout-affecting transitions still relayout | unit | `nix develop -c cargo test -p mesh-core-shell animation_transition_dirty -- --nocapture` | ✅ | ⬜ pending |
| 56-03-01 | 03 | 3 | ANIM-01, ANIM-02 | T-56-04 | Paint-only keyframes repaint without layout only when stops are classified paint-only | unit | `nix develop -c cargo test -p mesh-core-shell keyframe_animation -- --nocapture` | ✅ | ⬜ pending |
| 56-03-02 | 03 | 3 | ANIM-01 | T-56-02 | Unsupported/missing animation diagnostics stay visible | unit | `nix develop -c cargo test -p mesh-core-shell animation_token -- --nocapture` | ✅ | ⬜ pending |
| 56-04-01 | 04 | 4 | ANIM-03 | T-56-05 | Animated bounds include current effect/transform overflow | unit | `nix develop -c cargo test -p mesh-core-shell animation_damage -- --nocapture` | ✅ | ⬜ pending |
| 56-04-02 | 04 | 4 | ANIM-03 | T-56-05 | Damage includes previous and current animated bounds | unit | `nix develop -c cargo test -p mesh-core-shell animation_damage -- --nocapture` | ✅ | ⬜ pending |
| 56-05-01 | 05 | 5 | ANIM-01, ANIM-02, ANIM-03 | T-56-06 | Shipped navigation/audio animation regressions stay bounded | integration | `nix develop -c cargo test -p mesh-core-shell shipped_navigation -- --nocapture` | ✅ | ⬜ pending |
| 56-05-02 | 05 | 5 | ANIM-01, ANIM-02, ANIM-03 | — | Full Phase 56 proof and metadata update | integration | `nix develop -c cargo test -p mesh-core-shell animation -- --nocapture && nix develop -c cargo test -p mesh-core-shell shipped_navigation -- --nocapture && nix develop -c cargo test -p mesh-core-render render_object_tree_marks -- --nocapture` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing Rust test infrastructure covers Phase 56. No new test framework is required.

---

## Manual-Only Verifications

All Phase 56 behaviors should have automated verification.

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 180s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
