---
phase: 27-viewport-culling-and-visibility-elision
verified: 2026-05-11T11:55:06Z
status: passed
score: 3/3 must-haves verified
overrides_applied: 0
re_verification:
  previous_status: none
  previous_score: null
  gaps_closed: []
  gaps_remaining: []
  regressions: []
---

# Phase 27: Viewport Culling and Visibility Elision Verification Report

**Phase Goal:** Prune offscreen, hidden, or clip-excluded work earlier so the CPU renderer stops generating unnecessary paint work.
**Verified:** 2026-05-11T11:55:06Z
**Status:** passed

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | Explicit hidden semantics are distinct from plain opacity-zero semantics. | ✓ VERIFIED | `ComputedStyle` now carries `visibility` in `crates/core/ui/elements/src/style/types.rs:150-211` and defines `Visibility` in `:684-689`. CSS `visibility` resolves into that enum in `crates/core/ui/elements/src/style/resolve.rs:1045-1051`. Render tests distinguish hidden-vs-opacity behavior in `crates/core/frontend/render/src/display_list.rs:1458-1493`. |
| 2 | Fully out-of-viewport descendants under explicit clip/scroll authority are omitted, while partially intersecting descendants still paint. | ✓ VERIFIED | Pruning metrics and subtree omission live in `crates/core/frontend/render/src/display_list.rs:77-80`, `:575-697`, and `:628-646`. Focused tests prove full omission and partial-intersection non-omission in `:1498-1536`. |
| 3 | Aggregate pruning proof flows through the existing invalidation/debug pipeline without a second diagnostics path. | ✓ VERIFIED | Aggregate counters were added to `RetainedPaintSnapshot` in `crates/core/foundation/debug/src/lib.rs:188-203`, translated in `crates/core/shell/src/shell/component.rs:151-176`, serialized in `crates/core/shell/src/shell/runtime/debug.rs:717-759`, and asserted in `crates/core/shell/src/shell/tests.rs:1568-1658` and `:2186-2190`. |

**Score:** 3/3 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
| --- | --- | --- | --- |
| `crates/core/ui/elements/src/style/types.rs` | Explicit hidden signal separate from `opacity` | ✓ VERIFIED | `ComputedStyle.visibility` and `Visibility` enum are present in `:150-211` and `:684-689`. |
| `crates/core/ui/elements/src/style/resolve.rs` | `visibility:hidden|collapse` no longer lowers to bare opacity | ✓ VERIFIED | `apply_declaration()` resolves `visibility` into `Visibility::{Hidden,Collapse,Visible}` in `:1045-1051`. |
| `crates/core/frontend/render/src/display_list.rs` | Explicit hidden omission, clipped subtree preclipping, and aggregate pruning counters | ✓ VERIFIED | Hidden gating, clipped subtree omission, and tests are in `:523-697` and `:1458-1536`. |
| `crates/core/foundation/debug/src/lib.rs` | Retained paint snapshot carries aggregate pruning counters | ✓ VERIFIED | `RetainedPaintSnapshot` includes `omitted_subtrees`, `omitted_nodes`, `omitted_commands`, and `preclipped_descendants` in `:188-203`. |
| `crates/core/shell/src/shell/runtime/debug.rs` | Existing `mesh.debug` invalidation JSON exposes aggregate pruning counters | ✓ VERIFIED | `profiling_invalidation_json()` serializes the new fields in `:756-759`. |
| `crates/core/shell/src/shell/tests.rs` | Stable shell assertions cover the aggregate counters | ✓ VERIFIED | `profiling_snapshot_exposes_typed_surface_invalidation_counts` and the JSON serialization assertion cover the counters in `:1568-1658` and `:2186-2190`. |

### Key Link Verification

| From | To | Via | Status | Details |
| --- | --- | --- | --- | --- |
| `crates/core/ui/elements/src/style/resolve.rs` | `crates/core/frontend/render/src/display_list.rs` | explicit `Visibility` semantics | ✓ VERIFIED | Visibility now resolves explicitly before `display_list.rs:671-678` decides hidden pruning, so plain opacity is no longer the authority. |
| `crates/core/frontend/render/src/display_list.rs` | `crates/core/shell/src/shell/component.rs` | `DisplayListMetrics` pruning counters | ✓ VERIFIED | The new `DisplayListMetrics` counters in `display_list.rs:77-80` are mapped into `RetainedPaintSnapshot` in `component.rs:173-176`. |
| `crates/core/shell/src/shell/component.rs` | `crates/core/shell/src/shell/runtime/debug.rs` | invalidation snapshot serialization | ✓ VERIFIED | The existing invalidation snapshot written in `shell_component.rs:376-477` is serialized through `profiling_invalidation_json()` in `debug.rs:717-759`. |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| --- | --- | --- | --- |
| Display-list hidden/viewport pruning tests | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list` | 15 tests passed, including explicit hidden, opacity-zero, full-viewport omission, and partial-intersection coverage. | ✓ PASS |
| Painter clipping regressions | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render painter_` | 3 tests passed. | ✓ PASS |
| Shell profiling/debug payload serialization | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling` | 30 tests passed after repairing test-only import regressions in the shell component harness. | ✓ PASS |
| Workspace formatting | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check` | Passes after formatting the verification-unblocking test harness imports. | ✓ PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| --- | --- | --- | --- | --- |
| `CULL-01` | `27-01-PLAN.md` | Fully offscreen descendants inside clipped or scrollable regions are omitted from retained paint-command generation or execution on the CPU path. | ✓ SATISFIED | `display_list.rs:628-697` performs subtree preclipping under clip/scroll authority, and `display_list_preclips_fully_out_of_viewport_descendants` proves it in `:1498-1516`. |
| `CULL-02` | `27-01-PLAN.md` | Nodes hidden by explicit visibility, surface state, or fully ineffective opacity stop generating unnecessary CPU paint work until they become visible again. | ✓ SATISFIED | Explicit hidden semantics are represented in `types.rs:150-211` and `:684-689`, consumed by `node_is_explicitly_hidden()` in `display_list.rs:671-678`, and distinguished from plain opacity in tests `:1458-1493`. |
| `CULL-04` | `27-01-PLAN.md` | Clipping and viewport rules avoid per-item CPU overhead on small primitives when a cheaper elision or coarser-boundary alternative exists. | ✓ SATISFIED | Phase 27 omits whole non-intersecting subtrees before command generation in `display_list.rs:628-697`, exposing aggregate proof counters in the existing invalidation payload via `component.rs:151-176` and `debug.rs:717-759`. |

Orphaned requirements: none.

### Human Verification Required

None.

### Gaps Summary

None. Phase 27’s required explicit-hidden semantics, viewport-aware subtree omission, and aggregate debug proof all verify against the current codebase and focused test selectors.

### Notes

- Verification required a non-functional fix to shell component test imports added on `HEAD`; those fixes were limited to the test harness and were necessary only to run the required `mesh-core-shell profiling` selector.

---

_Verified: 2026-05-11T11:55:06Z_  
_Verifier: Codex autonomous closeout_
