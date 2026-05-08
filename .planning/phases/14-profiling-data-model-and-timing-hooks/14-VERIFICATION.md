---
phase: 14-profiling-data-model-and-timing-hooks
verified: 2026-05-08T17:25:00Z
status: passed
score: 4/4 must-haves verified
gaps: []
---

# Phase 14: Profiling Data Model and Timing Hooks Verification Report

**Phase Goal:** Add a profiling runtime model and low-overhead timing hooks that measure real shell stages without changing normal user-facing behavior when profiling is off.
**Verified:** 2026-05-08T17:25:00Z
**Status:** passed
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | Profiling is a typed, debug-only shell contract rather than an end-user settings surface or parallel diagnostics subsystem. | ✓ VERIFIED | `crates/core/foundation/debug/src/lib.rs` now defines `ProfilingSnapshot`, `ProfilingStage`, sample/summary types, and `DebugOverlayState.profiling_enabled`; `crates/core/shell/src/shell/types.rs`, `runtime/request.rs`, `ipc.rs`, and `crates/tools/cli/src/main.rs` route profiling through `ToggleDebugProfiling`, `shell:debug_profiling`, and `mesh-shell debug profiling`. |
| 2 | Profiling session storage is bounded and starts clean when profiling is enabled. | ✓ VERIFIED | `crates/core/shell/src/shell/runtime/profiling.rs` maintains fixed recent-sample retention, shell-wide/per-surface accumulators, and `reset_for_new_session`; `profiling_session_reset_discards_previous_samples` proves a second enable clears prior samples. |
| 3 | The required top-level stages are measured at real runtime seams rather than inferred from one outer render span. | ✓ VERIFIED | `runtime/wayland.rs` records `InputHandling`; `runtime/request.rs` and `runtime/mod.rs` record `RuntimeUpdateHandling`; `component/rendering.rs` records `TreeBuild`, `StyleRestyle`, and `Layout`; `component/shell_component.rs` records `Paint`; `runtime/render.rs` records `PresentCommit`, `RedrawCount`, and `TotalSurfaceRender`. |
| 4 | Profiling snapshots stay silent when disabled and expose stable shell-wide/per-surface rollups when enabled. | ✓ VERIFIED | `runtime/debug.rs` emits profiling payloads only when `profiling_enabled` is true; shell tests prove disabled-mode omission, required stage buckets, per-surface `surface_id` identity, redraw count, and total render accounting. |

**Score:** 4/4 truths verified

---

### Requirements Coverage

| Requirement | Description | Status | Evidence |
| --- | --- | --- | --- |
| `PROF-02` | When profiling is disabled, the shell does not emit live profiling snapshots or require profiling-specific UI/runtime work. | ✓ SATISFIED | `DebugSnapshot.profiling` is `None` when disabled; `debug_snapshot_omits_profiling_payload_when_disabled` and `profiling_disabled_runtime_stage_helpers_remain_inert` pass. |
| `PROF-03` | When profiling is enabled, instrumentation overhead stays bounded enough that the inspector can be used during live interaction without making measurements meaningless. | ✓ SATISFIED | Fixed recent-sample retention, shell-owned gated helpers, and collector reset behavior are implemented in `runtime/profiling.rs`; focused profiling tests pass. |
| `TIME-01` | Profiling captures shell-wide timing buckets for input handling, script/runtime updates, tree build, style/restyle, layout, paint, present/commit, redraw count, and total surface render time. | ✓ SATISFIED | `profiling_snapshot_includes_required_shell_stage_buckets` proves all required stage enums appear in shell summaries. |
| `TIME-03` | Profiling snapshot data rolls up stage timings into stable shell-wide and per-surface summaries suitable for a live rolling inspector. | ✓ SATISFIED | `runtime/debug.rs` builds `ProfilingSnapshot` from collector state; `profiling_snapshot_tracks_bounded_surface_samples_and_redraw_counts` and `profiling_snapshot_uses_surface_id_as_canonical_key_and_skips_unworked_surfaces` verify stable rollups. |

---

### Required Artifacts

| Artifact | Expected | Status | Details |
| --- | --- | --- | --- |
| `crates/core/foundation/debug/src/lib.rs` | Typed profiling snapshot and stage contract | ✓ VERIFIED | Adds profiling state, stage enum, samples, summaries, and debug toggle/session fields. |
| `crates/core/shell/src/shell/runtime/profiling.rs` | Bounded collector and rollup storage | ✓ VERIFIED | Provides shell-wide/per-surface accumulators, reset, and snapshot helpers. |
| `crates/core/shell/src/shell/runtime/wayland.rs` | Input stage timing hook | ✓ VERIFIED | Records shell-wide `InputHandling` timing when profiling is enabled. |
| `crates/core/shell/src/shell/runtime/request.rs` | Runtime update timing hook | ✓ VERIFIED | Records `RuntimeUpdateHandling` around shell request application and profiling toggle/reset semantics. |
| `crates/core/shell/src/shell/runtime/render.rs` | Present/redraw/total render rollups | ✓ VERIFIED | Harvests component stage records and records `PresentCommit`, `RedrawCount`, and `TotalSurfaceRender`. |
| `crates/core/shell/src/shell/component/rendering.rs` | Tree/style/layout timing hooks | ✓ VERIFIED | Records `TreeBuild`, `StyleRestyle`, and `Layout` directly at execution sites. |
| `crates/core/shell/src/shell/component/shell_component.rs` | Paint timing hook and profiling record handoff | ✓ VERIFIED | Records `Paint` and exposes component profiling records to the shell render loop. |
| `crates/core/shell/src/shell/tests.rs` | Automated proof of profiling behavior | ✓ VERIFIED | Contains debug/profiling control, snapshot, stage bucket, disabled-mode, redraw, and surface identity tests. |

---

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| --- | --- | --- | --- |
| Debug-path profiling control and disabled snapshot behavior | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell debug_` | 5 tests passed | ✓ PASS |
| Profiling collector, stage bucket, snapshot, and disabled-helper behavior | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` | 8 tests passed | ✓ PASS |
| Profiling snapshot/debug symbol presence in code | `grep -n 'profil\|redraw\|surface render' crates/core/shell/src/shell/runtime/debug.rs crates/core/shell/src/shell/tests.rs` | Matches found | ✓ PASS |

---

### Anti-Patterns Found

| File | Pattern | Severity | Impact |
| --- | --- | --- | --- |
| `14-04-PLAN.md` | Verification command used two cargo test-name filters in one invocation, which cargo does not support. | ⚠️ Low | Verification intent was preserved by running separate `debug_` and `profiling_` slices. |
| `—` | No evidence of end-user settings expansion, unbounded trace retention, or aggregate-only profiling output was found in the Phase 14 implementation. | ℹ️ Info | The implementation stayed inside the agreed debug-only, bounded, rolling-profiler boundary. |

---

### Human Verification Required

None. Phase 14’s acceptance criteria are covered by shell-owned implementation evidence and focused automated tests.

---

### Gaps Summary

No blocker gaps remain. Phase 14 now provides the debug-only profiling contract, bounded runtime collector, real stage timing hooks, and shell-owned regression proof required for later attribution and inspector phases.

---

_Verified: 2026-05-08T17:25:00Z_
_Verifier: Codex_
