---
phase: 15-backend-and-surface-attribution
verified: 2026-05-08T17:49:11Z
status: passed
score: 3/3 must-haves verified
gaps: []
---

# Phase 15: Backend and Surface Attribution Verification Report

**Phase Goal:** Extend the debug-only profiling pipeline so shell snapshots preserve stable per-surface timing breakdowns and expose backend provider/service attribution for poll/update, command handling, and state publish/delivery work.
**Verified:** 2026-05-08T17:49:11Z
**Status:** passed
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | Backend profiling is attributed to a concrete `(interface, provider_id)` identity instead of a single aggregate backend bucket. | ✓ VERIFIED | `crates/core/foundation/debug/src/lib.rs` defines typed backend snapshot contracts; `crates/core/shell/src/shell/runtime/profiling.rs` stores backend rollups by `(interface, provider_id)`; `profiling_snapshot_groups_backend_stage_proof_under_expected_provider_identity` proves all final backend stages stay grouped under `mesh.audio` / `@mesh/pipewire-audio`. |
| 2 | Backend work is split into explicit `PollUpdate`, `CommandHandling`, and `StatePublishDelivery` stages at real shell-owned seams. | ✓ VERIFIED | `crates/core/shell/src/shell/runtime/mod.rs`, `request.rs`, and `service_state.rs` feed stage-specific samples into the profiler; the focused tests `profiling_backend_poll_update_attributes_accepted_backend_messages`, `profiling_service_command_attributes_active_provider_dispatch`, and `profiling_state_publish_delivery_attributes_accepted_service_updates` each prove one stage on accepted work. |
| 3 | Per-surface profiling snapshots remain stable and directly comparable with shell totals while profiling-disabled mode stays silent. | ✓ VERIFIED | `crates/core/shell/src/shell/runtime/debug.rs` emits one snapshot containing shell, surface, and backend rollups; `profiling_surface_snapshot_preserves_surface_and_module_identity_with_comparable_totals` proves `surface_id`, `module_id`, and matching shell-vs-surface totals; `profiling_disabled_backend_paths_do_not_fabricate_snapshots` proves backend attribution additions still emit no profiling payload when disabled. |

**Score:** 3/3 truths verified

---

### Requirements Coverage

| Requirement | Description | Status | Evidence |
| --- | --- | --- | --- |
| `TIME-02` | Shell-wide totals and per-surface timings remain directly comparable in the final snapshot. | ✓ SATISFIED | `ProfilingSurfaceSnapshot` still carries `surface_id` and `module_id`; `profiling_surface_snapshot_preserves_surface_and_module_identity_with_comparable_totals`, `profiling_snapshot_uses_surface_id_as_canonical_key_and_skips_unworked_surfaces`, and `profiling_snapshot_backfills_surface_module_id_after_empty_stage_metadata` prove stable per-surface rollups. |
| `BACK-01` | Backend profiling exposes typed provider/service attribution rather than aggregate-only backend timing. | ✓ SATISFIED | `ProfilingSnapshot.backends` carries typed backend summaries keyed by interface/provider identity, and `profiling_snapshot_tracks_bounded_backend_samples_by_provider` plus `profiling_snapshot_groups_backend_stage_proof_under_expected_provider_identity` prove the keyed output. |
| `BACK-02` | Backend attribution is split across update, command, and state publish/delivery seams. | ✓ SATISFIED | `profiling_backend_poll_update_attributes_accepted_backend_messages`, `profiling_service_command_attributes_active_provider_dispatch`, and `profiling_state_publish_delivery_attributes_accepted_service_updates` each prove one stage on the real accepted path; stale-path regressions prove rejected updates stay silent. |

---

### Required Artifacts

| Artifact | Expected | Status | Details |
| --- | --- | --- | --- |
| `crates/core/foundation/debug/src/lib.rs` | Typed backend profiling contract | ✓ VERIFIED | Defines `ProfilingBackendSnapshot`, `ProfilingBackendStage`, backend samples, and summaries used by the shell profiler. |
| `crates/core/shell/src/shell/runtime/profiling.rs` | Bounded collector for shell, surface, and backend attribution | ✓ VERIFIED | Keeps bounded backend accumulators beside stable shell/per-surface rollups. |
| `crates/core/shell/src/shell/runtime/mod.rs` | Poll/update attribution seam | ✓ VERIFIED | Accepted backend service updates reach profiling through provider-aware shell messages. |
| `crates/core/shell/src/shell/runtime/request.rs` | Command-handling attribution seam | ✓ VERIFIED | Service command dispatch records `CommandHandling` against the active provider. |
| `crates/core/shell/src/shell/runtime/service_state.rs` | Publish/delivery attribution seam | ✓ VERIFIED | Accepted service-event fanout records `StatePublishDelivery` timing while stale updates stay silent. |
| `crates/core/shell/src/shell/runtime/debug.rs` | Deterministic shell/surface/backend snapshot assembly | ✓ VERIFIED | Emits profiling only when enabled and sorts backend and surface summaries deterministically. |
| `crates/core/shell/src/shell/tests.rs` | Automated proof of final backend and surface attribution behavior | ✓ VERIFIED | Contains focused stage-attribution, disabled-mode, and per-surface stability regressions, including the final proof tests added in Plan 15-04. |

---

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| --- | --- | --- | --- |
| Focused shell profiling regressions for backend attribution and per-surface stability | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` | 20 tests passed | ✓ PASS |
| Final proof tests are present in the shell regression file | `grep -n 'PollUpdate\|CommandHandling\|StatePublishDelivery\|surface_id\|module_id' crates/core/shell/src/shell/tests.rs` | Matches found for backend stages and per-surface identity checks | ✓ PASS |

---

### Anti-Patterns Found

| File | Pattern | Severity | Impact |
| --- | --- | --- | --- |
| `—` | No aggregate-only backend fallback, no profiling-on-by-default behavior, and no dropped per-surface identity fields were found in the final Phase 15 implementation. | ℹ️ Info | The phase stayed inside the debug-only, bounded, shell-owned attribution design. |

---

### Human Verification Required

None. Phase 15 acceptance is covered by shell-owned implementation evidence and focused automated profiling regressions.

---

### Gaps Summary

No blocker gaps remain. Phase 15 now closes with typed backend attribution for poll/update, command handling, and publish/delivery work, plus stable per-surface timing snapshots that remain comparable with shell-wide totals.

---

_Verified: 2026-05-08T17:49:11Z_
_Verifier: Codex_
