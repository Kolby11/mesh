---
phase: 16
slug: debug-only-profiling-mode-and-live-inspector
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-08
---

# Phase 16 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell debug_` |
| **Full suite command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell` |
| **Estimated runtime** | ~180 seconds |

## Sampling Rate

- **After every task commit:** Run the task-specific `<verify>` command from the relevant `PLAN.md`.
- **After every plan wave:** Run `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell debug_` and the relevant focused inspector/profiling checks.
- **Before `$gsd-verify-work`:** Full `mesh-core-shell` tests must be green.
- **Max feedback latency:** 180 seconds.

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 16-01-01 | 01 | 1 | PROF-01 | T-16-01 | Debug overlay visibility and profiling enable remain separate shell-owned states | shell/unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell debug_` | yes | pending |
| 16-01-02 | 01 | 1 | PROF-01, INSP-01 | T-16-01 | The shell mounts or routes a debug-only inspector surface without introducing a second end-user diagnostics entrypoint | shell/unit | `grep -n 'ToggleDebugOverlay\\|ToggleDebugProfiling\\|CycleDebugTab' crates/core/shell/src/shell/runtime/request.rs crates/core/shell/src/shell/ipc.rs crates/core/shell/src/shell/types.rs` | yes | pending |
| 16-02-01 | 02 | 2 | INSP-01 | T-16-02 | The inspector ships as a normal `.mesh` frontend module/package with a manifest and `src/main.mesh` entrypoint | grep/component | `find modules/frontend -path '*inspector*/module.json' -o -path '*inspector*/src/main.mesh'` | no | pending |
| 16-02-02 | 02 | 2 | INSP-02 | T-16-02 | The inspector UI defines overview, surfaces, backend services, and benchmark/interaction views | grep/component | `grep -Rni 'overview\\|surfaces\\|backend\\|benchmark' modules/frontend` | yes | pending |
| 16-03-01 | 03 | 3 | INSP-02, INSP-03 | T-16-03 | Sparse or empty profiling data renders stable inspector states instead of broken or missing UI | shell/component | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell inspector_` | yes | pending |
| 16-03-02 | 03 | 3 | INSP-03 | T-16-03 | Backend/service and surface views tolerate no-sample cases while preserving valid debug snapshot rendering | shell/unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` | yes | pending |
| 16-04-01 | 04 | 4 | PROF-01, INSP-01, INSP-02, INSP-03 | T-16-04 | Final shell and surface regressions prove the native panel replacement still uses the existing debug path and exposes the full required inspector view set | shell/component | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell` | yes | pending |

## Wave 0 Requirements

Existing Rust shell and component test infrastructure covers the phase. No new framework install work is required before planning or execution.

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Debug-only inspector interaction feels like the current right-side panel while profiling stays opt-in | PROF-01, INSP-01 | The final right-panel interaction, mounted surface lifecycle, and empty-state readability need a human pass even with shell tests | Start the shell, trigger `shell:debug_overlay`, confirm the inspector appears in the debug path, toggle `shell:debug_profiling` separately, and verify profiling does not auto-open or auto-close the inspector. |
| Overview, surfaces, backend, and benchmark scaffold views remain legible with zero samples and with live samples | INSP-02, INSP-03 | The visual stability and explanatory copy for sparse states are UI concerns not fully covered by grep/unit checks | Open the inspector before enabling profiling, confirm all required views exist with zero-state messaging, then enable profiling and verify live values populate without layout breakage. |

## Validation Sign-Off

- [x] All tasks have automated verify commands or explicit manual verification coverage.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all required test infrastructure.
- [x] No watch-mode flags.
- [x] Feedback latency target under 180 seconds.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** pending
