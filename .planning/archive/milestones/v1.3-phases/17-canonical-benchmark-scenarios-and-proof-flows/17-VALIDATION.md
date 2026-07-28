---
phase: 17
slug: canonical-benchmark-scenarios-and-proof-flows
status: draft
nyquist_compliant: true
wave_0_complete: false
created: 2026-05-09
---

# Phase 17 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | Workspace `Cargo.toml` |
| **Quick run command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark` |
| **Full suite command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell` |
| **Estimated runtime** | ~60 seconds focused, workspace-dependent for full crate |

---

## Sampling Rate

- **After every task commit:** Run the focused command named in the task's `<verification>` block.
- **After every plan wave:** Run `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark` and any focused `debug_inspector` / `profiling_` selectors touched by the wave.
- **Before `$gsd-verify-work`:** `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell` must be green.
- **Max feedback latency:** 60 seconds for focused selectors when dependencies are already built.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 17-01-01 | 01 | 1 | BENCH-01..BENCH-05 | T-17-01 | Debug-only benchmark state does not enable profiling by default | unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark` | yes | pending |
| 17-02-01 | 02 | 1 | BENCH-01..BENCH-04 | T-17-02 | Benchmark launch requests remain explicit and debug-scoped | unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark` | yes | pending |
| 17-03-01 | 03 | 2 | BENCH-01..BENCH-05 | T-17-03 | Inspector renders unavailable/empty/result states without crashing | component | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell debug_inspector` | yes | pending |
| 17-04-01 | 04 | 2 | BACK-03, BENCH-05 | T-17-04 | Backend-driven benchmark correlates generic provider timing with frontend render cost | unit/component | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` | yes | pending |

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements.

---

## Manual-Only Verifications

All phase behaviors have automated verification planned. Live compositor smoke checks are useful after execution but are not required for Phase 17 acceptance.

---

## Validation Sign-Off

- [x] All tasks have automated verify commands or existing test infrastructure.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all missing references.
- [x] No watch-mode flags.
- [x] Feedback latency target is defined.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** approved 2026-05-09
