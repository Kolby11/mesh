---
phase: 14
slug: profiling-data-model-and-timing-hooks
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-08
---

# Phase 14 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `nix develop -c cargo test -p mesh-core-shell debug_` |
| **Full suite command** | `nix develop -c cargo test -p mesh-core-shell debug_ profiling_` |
| **Estimated runtime** | ~120 seconds |

## Sampling Rate

- **After every task commit:** Run the task-specific quick command from the PLAN.md `<verify>` block.
- **After every plan wave:** Run `nix develop -c cargo test -p mesh-core-shell debug_ profiling_`
- **Before `$gsd-verify-work`:** Full suite must be green.
- **Max feedback latency:** 120 seconds.

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 14-01-01 | 01 | 1 | PROF-02, TIME-03 | T-14-01 | Profiling state is explicit, shell-owned, and absent from live snapshots when disabled | shell/unit | `nix develop -c cargo test -p mesh-core-shell debug_` | yes | pending |
| 14-01-02 | 01 | 1 | PROF-02 | T-14-01 | Profiling enable/disable routes only through debug request and IPC/CLI seams | grep/shell | `grep -n 'ToggleDebugProfiling\\|debug_profiling' crates/core/shell/src/shell/types.rs crates/core/shell/src/shell/ipc.rs crates/tools/cli/src/main.rs` | yes | pending |
| 14-02-01 | 02 | 2 | PROF-03, TIME-03 | T-14-02 | Collector stores bounded rolling aggregates plus fixed-count recent samples | shell/unit | `nix develop -c cargo test -p mesh-core-shell profiling_` | yes | pending |
| 14-02-02 | 02 | 2 | PROF-03 | T-14-02 | Enabling profiling resets session data and keeps allocation/retention bounded | shell/unit | `nix develop -c cargo test -p mesh-core-shell profiling_` | yes | pending |
| 14-03-01 | 03 | 3 | TIME-01, PROF-03 | T-14-03 | Tree build, style/restyle, layout, paint, present, redraw count, and total surface render stages are measured at real runtime seams | shell/unit | `nix develop -c cargo test -p mesh-core-shell profiling_stage` | yes | pending |
| 14-03-02 | 03 | 3 | PROF-02, TIME-01 | T-14-03 | Profiling-off mode keeps the runtime fast path inert and avoids phantom surface entries | shell/unit | `nix develop -c cargo test -p mesh-core-shell profiling_disabled` | yes | pending |
| 14-04-01 | 04 | 4 | TIME-03 | T-14-04 | Debug snapshots expose stable shell-wide and per-surface rollups for later inspector phases | shell/unit | `nix develop -c cargo test -p mesh-core-shell debug_snapshot profiling_snapshot` | yes | pending |
| 14-04-02 | 04 | 4 | PROF-02, PROF-03, TIME-01, TIME-03 | T-14-04 | End-to-end regression tests prove toggle/reset semantics and required stage coverage without UI-only proof | shell/unit | `nix develop -c cargo test -p mesh-core-shell debug_ profiling_ -- --nocapture` | yes | pending |

## Wave 0 Requirements

Existing Rust shell test infrastructure covers all phase requirements.

## Manual-Only Verifications

Optional manual smoke checks through `mesh-shell ipc shell:debug_profiling` may still be useful during execution, but all required Phase 14 acceptance behaviors should have automated proof paths in `mesh-core-shell`.

## Validation Sign-Off

- [x] All tasks have automated verify commands.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all required test infrastructure.
- [x] No watch-mode flags.
- [x] Feedback latency target under 120 seconds.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** pending
