---
phase: 18
slug: targeted-optimization-pass
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-09
---

# Phase 18 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark` |
| **Full suite command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark && env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_ && env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell debug_inspector` |
| **Estimated runtime** | ~180 seconds |

---

## Sampling Rate

- **After every task commit:** Run the focused command named in that task.
- **After every plan wave:** Run the full suite command above.
- **Before `$gsd-verify-work`:** Full suite must be green and `18-OPTIMIZATION-PROOF.md` must record >= 10% improvement.
- **Max feedback latency:** 180 seconds for focused feedback, excluding Nix cache lock waits.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 18-01-01 | 01 | 1 | OPT-01 | T-18-01 | N/A | artifact + unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark` | yes | pending |
| 18-01-02 | 01 | 1 | OPT-01 | T-18-01 | N/A | artifact | `test -f .planning/phases/18-targeted-optimization-pass/18-BASELINE.md` | yes | pending |
| 18-02-01 | 02 | 2 | OPT-01 | T-18-02 | Preserve benchmark contract | focused unit | selected by `18-BASELINE.md` | yes | pending |
| 18-02-02 | 02 | 2 | OPT-03 | T-18-03 | Profiling off remains silent | unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` | yes | pending |
| 18-03-01 | 03 | 3 | OPT-02 | T-18-04 | N/A | artifact | `test -f .planning/phases/18-targeted-optimization-pass/18-OPTIMIZATION-PROOF.md` | yes | pending |
| 18-03-02 | 03 | 3 | OPT-03 | T-18-03 | Public contracts unchanged | unit/component | full suite command | yes | pending |

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Live compositor feel of optimized interaction | OPT-02 | Automated tests use deterministic shell/component fixtures, not a real compositor session. | If a live Wayland session is available, enable debug profiling, run the selected benchmark interaction before and after the change, and compare the same metric recorded in `18-OPTIMIZATION-PROOF.md`. |

---

## Validation Sign-Off

- [x] All tasks have automated verify or artifact checks.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all missing references.
- [x] No watch-mode flags.
- [x] Feedback latency target documented.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** approved 2026-05-09
