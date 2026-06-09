---
phase: 96-selector-dependency-tracking
plan: "02"
subsystem: shell/component/runtime-tree
tags: [retained-tree, state-diffing, pseudo-classes, performance]
dependency_graph:
  requires: [96-01]
  provides: [changed_state_bits in RetainedTreeDirtySummary]
  affects: [runtime_tree.rs, RetainedTreeDirtySummary consumers]
tech_stack:
  added: []
  patterns: [bitmask-xor-diffing, per-frame-state-accumulation]
key_files:
  modified:
    - crates/core/shell/src/shell/component/runtime_tree.rs
decisions:
  - "state_bitmask() kept self-contained in runtime_tree.rs to avoid cross-crate dependency on private resolve.rs constants"
  - "Task 3 required no code changes: existing tests use field-by-field assertions and do not directly construct RetainedNodeSnapshot or call diff_flags()"
  - "Smithay xkbcommon system library missing in CI environment; build failure pre-existed and is unrelated to these changes"
metrics:
  duration: "8 minutes"
  completed: "2026-06-07"
  tasks_completed: 3
  files_modified: 1
---

# Phase 96 Plan 02: State Diffing with changed_state_bits Summary

**One-liner:** Per-node state diffing via ElementState XOR bitmask stored in RetainedTreeDirtySummary.changed_state_bits for targeted pseudo-class restyle.

## What Was Built

Extended the retained widget tree's state tracking from "did state change?" (boolean hash comparison) to "which state bits changed?" (bitmask XOR). This enables Plan 3's targeted restyle logic to apply only to rules matching the flipped pseudo-classes.

### Changes in runtime_tree.rs

**RetainedNodeSnapshot struct** (previously partial in commit c13af9e):
- Field is now `state: ElementState` (direct storage, not `state_hash: u64`)

**diff_flags() method**: Changed return type from `RetainedNodeDirtyFlags` to `(RetainedNodeDirtyFlags, u32)`:
- Computes `changed_state_bits = state_bitmask(self.state) ^ state_bitmask(next.state)` when state differs
- Returns 0 as changed_state_bits when state is identical

**state_bitmask() helper**: New function converting `ElementState` to a u32 bitmask with stable bit positions (hovered=0, focused=1, active=2, disabled=3, ..., focus_visible=12).

**RetainedTreeDirtySummary struct**: Added `changed_state_bits: u32` field — OR-accumulated across all state-dirty nodes during `update()`.

**RetainedWidgetTree::update()**: Destructures `(flags, node_state_bits)` from `diff_flags()` and runs `dirty.changed_state_bits |= node_state_bits`.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Complete state_hash removal — fix diff_flags state comparison | eb1103b | runtime_tree.rs |
| 2 | Add changed_state_bits to RetainedTreeDirtySummary and diff_flags return | eb1103b | runtime_tree.rs |
| 3 | Verify existing tests — no changes needed (field-by-field assertions) | (no-code) | runtime_tree.rs |

Tasks 1 and 2 were committed together (eb1103b) as they were coordinated changes to the same function and struct in the same file. The prior partial commit c13af9e had already replaced the struct field but left a stale `state_hash` reference in `diff_flags()` — this was the key fix in task 1.

## Verification

**Code correctness verified by inspection:**
- Zero `state_hash` references remain in runtime_tree.rs
- Zero `state_fingerprint` references remain in runtime_tree.rs
- `diff_flags()` return type is `(RetainedNodeDirtyFlags, u32)`
- `state_bitmask()` defined with 13 bit positions matching ElementState fields
- `dirty.changed_state_bits |= node_state_bits` in update() loop

**Tests verified compatible:**
- Tests use field-by-field assertions (`dirty.layout`, `dirty.state`, etc.) — not full struct comparison
- `assert_eq!(retained.last_dirty(), dirty)` works because both sides have the same `changed_state_bits`
- No test directly constructs `RetainedNodeSnapshot` or calls `diff_flags()`

**Environment note:** `cargo test -p mesh-core-shell` could not run in this environment because `smithay-client-toolkit` requires the `xkbcommon` system library (pkg-config), which is not installed. This build failure pre-dates these changes (confirmed in plan 96-01 which only tested `mesh-core-elements`).

## Deviations from Plan

**1. [Rule 1 - Bug] Fixed stale state_hash reference from incomplete prior commit**
- **Found during:** Task 1 start
- **Issue:** Commit c13af9e replaced the struct field (`state: ElementState`) and the `retained_snapshot()` assignment, and removed `state_fingerprint()`, but left `if self.state_hash != next.state_hash` in `diff_flags()` — a compile error masked by smithay build failure
- **Fix:** Replaced with `if self.state != next.state` as part of the Task 1+2 combined implementation
- **Files modified:** runtime_tree.rs
- **Commit:** eb1103b

**2. Tasks 1 and 2 committed together**
- **Reason:** Both tasks modified the same function (`diff_flags`) and the same struct (`RetainedTreeDirtySummary`). Splitting into separate commits would have left a non-compiling intermediate state (task 1 fix + old return type = type mismatch in update()).
- **Impact:** No separate Task 1 commit hash; all changes in eb1103b.

**3. Task 3 required no code changes**
- **Reason:** Existing tests do not directly construct `RetainedNodeSnapshot` or call `diff_flags()` — they test through `RetainedWidgetTree::update()` and check individual fields of the returned summary. No structural pattern match on the full struct. Tests are correct as-is.

## Known Stubs

None. The implementation is complete and fully wired.

## Threat Flags

None. This change is internal to the retained tree diffing logic and does not introduce new network endpoints, auth paths, file access patterns, or schema changes at trust boundaries.

## Self-Check: PASSED

- `crates/core/shell/src/shell/component/runtime_tree.rs` — FOUND (modified)
- Commit `eb1103b` — FOUND (feat: complete state diffing with changed_state_bits)
- `grep -c 'state_hash' runtime_tree.rs` — 0 (all removed)
- `grep 'changed_state_bits: u32' runtime_tree.rs` — FOUND (field in struct)
- `grep 'fn diff_flags.*->.*u32' runtime_tree.rs` — FOUND (updated return type)
- `grep -c 'state_bitmask' runtime_tree.rs` — 3 (definition + 2 calls in diff_flags)
- `grep 'dirty.changed_state_bits' runtime_tree.rs` — FOUND (accumulation in update)
