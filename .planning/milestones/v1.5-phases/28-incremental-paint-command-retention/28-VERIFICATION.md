---
phase: 28-incremental-paint-command-retention
verified: 2026-05-11T00:00:00Z
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

# Phase 28: Incremental Paint Command Retention Verification Report

**Phase Goal:** Stop local retained-tree changes from forcing whole-surface paint-command recollection.  
**Verified:** 2026-05-11  
**Status:** passed

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | Retained paint-command ownership is now subtree-local inside `mesh-core-render`. | ✓ VERIFIED | `crates/core/frontend/render/src/display_list.rs` now stores retained paint subtrees keyed by `NodeId` and rebuilds the flat paint-command slice from subtree caches instead of recollecting the full surface every time. |
| 2 | Transform-, scroll-, and local reorder-only updates preserve unrelated sibling subtree command payloads. | ✓ VERIFIED | Focused tests in `crates/core/frontend/render/src/display_list.rs` prove transform, scroll, and reorder changes on one branch keep the unrelated sibling branch command payloads unchanged. |
| 3 | Aggregate reuse, rebuild, and fallback proof flows through the existing invalidation/debug payload without a second diagnostics channel. | ✓ VERIFIED | `crates/core/foundation/debug/src/lib.rs`, `crates/core/shell/src/shell/component.rs`, `crates/core/shell/src/shell/runtime/debug.rs`, and `crates/core/shell/src/shell/tests.rs` now carry and assert subtree reuse or fallback counters under `invalidation.paint`. |

**Score:** 3/3 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
| --- | --- | --- | --- |
| `crates/core/frontend/render/src/display_list.rs` | subtree-owned retained command state, local refresh path, fallback metrics | ✓ VERIFIED | Contains retained paint subtree caches, local dirty-subtree rebuild logic, and aggregate subtree metrics. |
| `crates/core/frontend/render/src/render_object.rs` | dirty-node and dirty-summary signals drive local refresh | ✓ VERIFIED | Scroll offsets now count as geometry-affecting retained paint inputs, and focused tests prove dirty-node tracking still classifies changes. |
| `crates/core/foundation/debug/src/lib.rs` | retained paint snapshot exposes subtree reuse and fallback counters | ✓ VERIFIED | `RetainedPaintSnapshot` includes subtree segment reuse, subtree rebuild, rebuilt-command, and fallback counters. |
| `crates/core/shell/src/shell/runtime/debug.rs` | `mesh.debug` serialization exposes the new aggregate counters | ✓ VERIFIED | `profiling_invalidation_json()` serializes the new subtree reuse and fallback fields under `invalidation.paint`. |
| `crates/core/shell/src/shell/tests.rs` | stable shell assertions cover the new payload shape | ✓ VERIFIED | Profiling payload tests assert the new counters in serialized `mesh.debug` state. |

### Key Link Verification

| From | To | Via | Status | Details |
| --- | --- | --- | --- | --- |
| `crates/core/frontend/render/src/render_object.rs` | `crates/core/frontend/render/src/display_list.rs` | dirty summary + dirty node IDs | ✓ VERIFIED | The shell now feeds render-object dirty signals directly into retained display-list updates so local reuse decisions stay anchored to retained render-object state. |
| `crates/core/frontend/render/src/display_list.rs` | `crates/core/shell/src/shell/component.rs` | `DisplayListMetrics` subtree counters | ✓ VERIFIED | Aggregate subtree counters map into `RetainedPaintSnapshot` through the existing invalidation snapshot builder. |
| `crates/core/shell/src/shell/component.rs` | `crates/core/shell/src/shell/runtime/debug.rs` | invalidation snapshot serialization | ✓ VERIFIED | The shell serializes the new fields into `mesh.debug.profiling.surfaces[].invalidation.paint` without introducing a second diagnostics payload. |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| --- | --- | --- | --- |
| Render-object dirty tracking | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render render_object` | 4 tests passed, including the new scroll-dirty retained paint proof. | ✓ PASS |
| Retained display-list local reuse and fallback proof | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list` | 19 tests passed, including transform-only, scroll-only, reorder-only sibling reuse and ambiguous fallback coverage. | ✓ PASS |
| Shell profiling/debug payload serialization | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling` | 30 tests passed with the new `invalidation.paint` subtree counters serialized and asserted. | ✓ PASS |
| Workspace formatting | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check` | Passed. | ✓ PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| --- | --- | --- | --- | --- |
| `PIPE-01` | `28-01-PLAN.md` | Local retained-tree changes stop forcing whole-surface paint-command recollection. | ✓ SATISFIED | Retained paint-command ownership now uses subtree caches in `display_list.rs`, and focused reuse tests prove sibling branches are preserved during local updates. |
| `PIPE-02` | `28-01-PLAN.md` | Transform-, scroll-, and reorder-heavy updates preserve unrelated retained paint data where correctness is cheap to prove. | ✓ SATISFIED | The new local rebuild path plus transform/scroll/reorder tests in `display_list.rs` show unrelated sibling subtree commands remain stable. |

Orphaned requirements: none.

### Human Verification Required

None.

### Gaps Summary

None. Phase 28’s subtree ownership, conservative fallback behavior, and aggregate debug proof all verify against the current codebase and focused selectors.

---

_Verified: 2026-05-11_  
_Verifier: Codex autonomous closeout_
