---
phase: 31
slug: smoothness-proof-and-cpu-render-tuning
status: draft
nyquist_compliant: true
wave_0_complete: false
created: 2026-05-12
---

# Phase 31 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness through Cargo 1.94.1 |
| **Config file** | Workspace `Cargo.toml` |
| **Quick run command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list` |
| **Full suite command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render && env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling` |
| **Estimated runtime** | ~120 seconds |

---

## Sampling Rate

- **After every task commit:** Run `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list` plus the most relevant focused selector for the touched module.
- **After every plan wave:** Run `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render && env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling`.
- **Before `$gsd-verify-work`:** Full suite must be green, canonical proof must be captured with `--nocapture`, and `31-UAT.md` must cover all five scenarios.
- **Max feedback latency:** 120 seconds for focused validation.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 31-01-01 | 01 | 0 | PERF-03, SMTH-01 | T-31-02 / T-31-03 | Accepted changes require shipped-surface benchmark evidence, not counters alone | benchmark + artifact | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase26_real_surface_baseline_emits_canonical_proof_measurements -- --nocapture` | ✅ benchmark, ❌ W0 artifact | ⬜ pending |
| 31-01-02 | 01 | 0 | SMTH-01, SMTH-02 | T-31-04 | Human-visible smoothness and interaction correctness are checked on shipped surfaces | manual UAT | Complete `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-UAT.md` | ❌ W0 | ⬜ pending |
| 31-01-03 | 01 | 1 | SMTH-02 | T-31-01 / T-31-04 | Partial repaint preserves ordering, clipping, backgrounds, opacity, overlays, and scrollbars | unit/integration | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list && env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell navigation profiling` | ✅ | ⬜ pending |
| 31-01-04 | 01 | 1 | PERF-03, SMTH-02 | T-31-01 / T-31-03 | Cache/repaint tuning keeps bounded caches, freshness checks, bypass behavior, and conservative fallbacks | unit/benchmark | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render icon display_list` plus canonical proof command | ✅ | ⬜ pending |
| 31-01-05 | 01 | 2 | SMTH-03 | — | GPU backend and parallel paint/layout remain explicitly out of scope | documentation review | Verify Phase 31 benchmark, UAT, verification, and summary artifacts mention future boundaries without implementation files | ❌ W0 artifact | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-01-BENCHMARK.md` — scenario-by-scenario before/after comparison against Phase 26 baseline and Phase 30 cache proof.
- [ ] `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-UAT.md` — focused manual notes for `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, and `backend_update`.
- [ ] Optional helper assertion in `crates/core/shell/src/shell/component/tests/invalidation/profiling.rs` — only if implementation needs machine-readable Phase 31 deltas instead of log comparison.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Canonical shipped-surface smoothness for `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, and `backend_update` | PERF-03, SMTH-01 | Visible smoothness cannot be fully proven by internal counters; Phase 31 explicitly requires focused UAT notes | Exercise each canonical interaction on the shipped shell surface after tuning, record before/after perception, and confirm no visible regressions apart from smoother rendering |
| Future GPU/parallel boundary remains intact | SMTH-03 | Scope control is primarily artifact and diff review | Confirm no new GPU backend, worker-thread paint/layout, or benchmark-system files were added; record the future boundary in Phase 31 proof artifacts |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all MISSING references.
- [x] No watch-mode flags.
- [x] Feedback latency < 120s for focused validation.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** approved 2026-05-12 for planning; execution sign-off pending completed Wave 0 artifacts.
