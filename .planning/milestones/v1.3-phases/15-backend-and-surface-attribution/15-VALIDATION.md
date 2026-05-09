---
phase: 15
slug: backend-and-surface-attribution
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-08
---

# Phase 15 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` |
| **Full suite command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell` |
| **Estimated runtime** | ~180 seconds |

## Sampling Rate

- **After every task commit:** Run the task-specific `<verify>` command from the relevant `PLAN.md`.
- **After every plan wave:** Run `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_`
- **Before `$gsd-verify-work`:** Full `mesh-core-shell` tests must be green.
- **Max feedback latency:** 180 seconds.

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 15-01-01 | 01 | 1 | BACK-01 | T-15-01 | Backend attribution is modeled as typed provider/service snapshots instead of aggregate-only shell buckets | grep/unit | `grep -n 'ProfilingBackend' crates/core/foundation/debug/src/lib.rs crates/core/shell/src/shell/runtime/debug.rs` | yes | pending |
| 15-01-02 | 01 | 1 | TIME-02, BACK-01 | T-15-01 | Collector stores backend summaries beside stable per-surface summaries without unbounded retention | shell/unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` | yes | pending |
| 15-02-01 | 02 | 2 | BACK-01, BACK-02 | T-15-02 | Backend update traffic records a poll/update stage against the accepted provider/service identity | shell/unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` | yes | pending |
| 15-02-02 | 02 | 2 | BACK-02 | T-15-02 | Service command dispatch records a command-handling stage for the active provider without touching disabled-mode behavior | shell/unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` | yes | pending |
| 15-03-01 | 03 | 3 | BACK-02 | T-15-03 | Service-event validation and component fanout record state publish/delivery timing for accepted provider updates | shell/unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` | yes | pending |
| 15-03-02 | 03 | 3 | TIME-02 | T-15-03 | Per-surface snapshots keep stage totals, `surface_id`, and `module_id` stable for shell-vs-surface comparison | shell/unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` | yes | pending |
| 15-04-01 | 04 | 4 | TIME-02, BACK-01, BACK-02 | T-15-04 | Debug snapshots sort backend attribution deterministically and preserve shell/per-surface/backend rollups together | shell/unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` | yes | pending |
| 15-04-02 | 04 | 4 | TIME-02, BACK-01, BACK-02 | T-15-04 | Focused shell tests prove disabled-mode silence plus provider/service stage attribution across update, command, and delivery seams | shell/unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` | yes | pending |

## Wave 0 Requirements

Existing Rust shell test infrastructure covers all Phase 15 requirements.

## Manual-Only Verifications

None required for planning acceptance. Optional manual inspection through the debug overlay/IPC path may still help during execution, but the Phase 15 contract should be proven through shell-owned tests.

## Validation Sign-Off

- [x] All tasks have automated verify commands.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all required test infrastructure.
- [x] No watch-mode flags.
- [x] Feedback latency target under 180 seconds.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** pending
