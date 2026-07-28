---
phase: 14-profiling-data-model-and-timing-hooks
plan: 04
subsystem: snapshot-proof
tags: [profiling, snapshots, tests, debug]

requires:
  - phase: 14-01
    provides: Debug profiling control path and typed snapshot contract
  - phase: 14-02
    provides: Collector/session storage and snapshot wiring
  - phase: 14-03
    provides: Live runtime stage instrumentation

provides:
  - Snapshot-focused shell tests for profiling session/reset, required stage coverage, and per-surface identity
  - Finalized shell/runtime proof that profiling stays silent when disabled and reports stable rollups when enabled

affects:
  - phase-15-attribution
  - phase-16-live-inspector

tech-stack:
  added: []
  patterns:
    - "Snapshot proof is shell-owned and automated rather than UI-inspection-driven"
    - "Support trait changes required by instrumentation are finalized in the same proof wave that validates them"

key-files:
  created:
    - .planning/phases/14-profiling-data-model-and-timing-hooks/14-04-SUMMARY.md
  modified:
    - crates/core/shell/src/shell/tests.rs
    - crates/core/shell/src/shell/types.rs
    - crates/core/shell/src/shell/component.rs
    - crates/core/shell/src/shell/runtime/profiling.rs

key-decisions:
  - "Phase 14 proof uses focused `debug_` and `profiling_` cargo filters because the plan's combined filter command was not valid cargo syntax."
  - "Per-surface identity remains `surface_id` first, with module id attached as optional context."
  - "The trait/component support changes needed for stage harvesting are finalized together with the regression proof instead of being left as uncommitted spillover."

requirements-completed: [PROF-02, PROF-03, TIME-01, TIME-03]

duration: 1 session
completed: 2026-05-08
---

# Phase 14 Plan 04: Snapshot Rollups and Regression Proof

**Phase 14 now has shell-owned regression proof for profiling session control, required stage rollups, disabled-mode silence, and per-surface identity/accounting.**

## Accomplishments

- Added snapshot-focused tests for required shell stage buckets, per-surface `surface_id` accounting, bounded surface samples, redraw counts, and disabled-mode inert behavior.
- Finalized the trait/component support needed for stage-record harvesting so the runtime instrumentation introduced in the previous wave is fully committed and verifiable.
- Verified the profiling/debug test slices end-to-end with focused cargo filters that match actual cargo CLI behavior.

## Task Commits

Pending commit in this workspace run. The implementation is validated and committed immediately after this summary.

## Verification

- `grep -n 'profil\|redraw\|surface render' crates/core/shell/src/shell/runtime/debug.rs crates/core/shell/src/shell/tests.rs`
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell debug_`
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_`

## Deviations From Plan

- The plan listed `nix develop -c cargo test -p mesh-core-shell debug_ profiling_ -- --nocapture`, but cargo accepts only one test-name filter at a time. The same verification intent was executed with two focused commands: `debug_` and `profiling_`.
- `crates/core/shell/src/shell/types.rs`, `crates/core/shell/src/shell/component.rs`, and `crates/core/shell/src/shell/runtime/profiling.rs` were included in the final proof commit because they were necessary support files for the already-implemented stage-harvesting path and could not be left uncommitted without breaking the phase.

## Self-Check: PASSED

- Summary file exists.
- Profiling snapshot and control regression tests are present in `crates/core/shell/src/shell/tests.rs`.
- Focused `debug_` and `profiling_` test runs passed.

---
*Phase: 14-profiling-data-model-and-timing-hooks*
*Completed: 2026-05-08*
