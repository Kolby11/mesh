---
phase: 96-selector-dependency-tracking
plan: "03"
subsystem: style-resolution/shell-rendering
tags: [targeted-restyle, pseudo-classes, hover, focus, inheritance, performance]
dependency_graph:
  requires: [96-01, 96-02]
  provides:
    - restyle_subtree_for_keys_with_index_and_inheritance in resolve.rs
    - collect_interaction_changed_keys in rendering.rs
    - previous_hovered_path / previous_focused_key cross-frame state diff
  affects:
    - crates/core/ui/elements/src/style/resolve.rs
    - crates/core/shell/src/shell/component/rendering.rs
    - crates/core/shell/src/shell/component.rs
tech_stack:
  added: []
  patterns:
    - targeted subtree restyle with inheritance propagation
    - borrow-checker-safe two-phase borrow (compute then borrow_mut)
    - affected-key set computed from previous/current hover+focus diff
key_files:
  modified:
    - crates/core/ui/elements/src/style/resolve.rs
    - crates/core/shell/src/shell/component/rendering.rs
    - crates/core/shell/src/shell/component.rs
decisions:
  - "affected_keys computed before &mut borrow of index_cache to satisfy borrow checker"
  - "collect_all_keys reused from runtime_tree import instead of duplicated in rendering.rs"
  - "Non-target, non-affected-subtree nodes still recurse children (needed to find target-keyed descendants deeper in the tree)"
  - "Pre-existing test failures in integration tests confirmed unrelated to plan changes (failed before and after)"
metrics:
  duration: "~25 minutes"
  completed: "2026-06-07"
  tasks_completed: 3
  files_modified: 3
---

# Phase 96 Plan 03: Targeted Interaction Restyle Summary

**One-liner:** Wired Plan 01's reverse index and Plan 02's changed_state_bits into finalize_tree so hover/focus state changes trigger inheritance-aware targeted restyle of only the changed node and its descendants.

## What Was Built

### Task 1 — Inheritance-aware targeted restyle (resolve.rs)

Replaced `restyle_subtree_for_keys_with_index` with a two-function design:

**`restyle_subtree_for_keys_with_index`** — unchanged public entry point, now delegates to `restyle_subtree_for_keys_with_index_and_inheritance(node, ..., parent_style=None)`.

**`restyle_subtree_for_keys_with_index_and_inheritance`** — new recursive implementation with correct logic:
- If node is a target key OR `parent_style.is_some()` (ancestor was restyled): recompute the node's style, apply inherited values from parent, pass `ParentInheritedStyle` to children
- If node is neither: don't restyle it, but **continue recursing** — target-keyed nodes may be deeper in the tree
- This correctly handles "node A is a target → children of A inherit A's updated style" AND "non-target root nodes still descend to find target children"

### Task 2 — Previous interaction state storage (component.rs, rendering.rs)

Added two fields to `FrontendSurfaceComponent`:
- `previous_hovered_path: Vec<String>` — previous frame's hover path
- `previous_focused_key: Option<String>` — previous frame's focused key

At the end of `finalize_tree`, these are snapshotted from the current values so the next frame can diff.

Added `collect_interaction_changed_keys(tree)` method that:
1. Unions previous + current hover path keys
2. Unions previous + current focused keys
3. For each changed key, collects all descendant keys via `collect_descendant_keys`
4. Returns empty HashSet on first frame (no previous state) — triggers full-tree fallback

Added `collect_descendant_keys` free function using the existing `collect_all_keys` import from runtime_tree.

### Task 3 — Wire targeted restyle into finalize_tree (rendering.rs)

Replaced the old `targeted_interaction_restyle` branch (which still did full-tree restyle) with:
- Compute `affected_keys` BEFORE borrowing `index_cache` (borrow checker constraint)
- If `affected_keys.is_empty()` → fall back to full-tree restyle (first frame behavior)
- Otherwise → call `restyle_subtree_for_keys_cached` on affected subtrees only
- `preserve_surface_root` path: iterate children individually, each gets `restyle_subtree_for_keys_cached`
- `merge_runtime_primitive_defaults` and `reused_retained_layout` run as before

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Fix restyle_subtree_for_keys_with_index to propagate inherited style | 5eda5f1 | resolve.rs |
| 2 | Store previous interaction state + collect_interaction_changed_keys | fc28722 | component.rs, rendering.rs |
| 3 | Wire targeted restyle into finalize_tree with fallback logic | 6ca9a85 | resolve.rs (fmt), rendering.rs |

## Verification

- `cargo check -p mesh-core-elements` — PASSED (no errors)
- `nix develop -c cargo check -p mesh-core-shell` — PASSED (no errors, 7 pre-existing warnings)
- `nix develop -c cargo test -p mesh-core-elements` — **101 passed, 0 failed** (all existing tests pass, including state_to_rules tests from Plan 01)
- `nix develop -c cargo test -p mesh-core-shell retained_widget_tree` — PASSED

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Incorrect recursion stop in restyle_subtree_for_keys_with_index_and_inheritance**
- **Found during:** Task 1 verification (test failure)
- **Issue:** Initial implementation stopped recursing into children if a node was neither a target nor in an affected subtree. This broke the test `targeted_restyle_recomputes_only_named_stateful_nodes` — the tree root had no `_mesh_key` so the function returned without visiting its children, leaving all target nodes unstyled.
- **Fix:** Added an `else` branch that continues recursing into children with `parent_style=None`. Non-target nodes without restyled ancestors still need to be traversed to find target-keyed descendants deeper in the tree.
- **Files modified:** `crates/core/ui/elements/src/style/resolve.rs`
- **Commit:** 6ca9a85 (included with Task 3 formatting commit)

**2. [Rule 3 - Blocking] Borrow checker conflict between collect_interaction_changed_keys and index_cache**
- **Found during:** Task 3 compilation
- **Issue:** `collect_interaction_changed_keys` requires `&self` but `index_cache = &mut self.cached_style_rule_index` holds a mutable borrow. Calling `self.collect_interaction_changed_keys(tree)` inside the mutable borrow scope caused E0502.
- **Fix:** Moved `affected_keys` computation before `let index_cache = &mut self.cached_style_rule_index`. The HashSet is pre-computed via `if targeted_interaction_restyle { ... } else { HashSet::new() }` before the mutable borrow.
- **Files modified:** `crates/core/shell/src/shell/component/rendering.rs`
- **Commit:** 6ca9a85

**3. Pre-existing integration test failures are unrelated**
- **Note:** `cargo test -p mesh-core-shell` shows 36 failures. Verified pre-existing by running with `git stash` — same failures before our changes. These test failures are in integration tests that depend on real surfaces and service state unrelated to this plan's changes.

## Known Stubs

None. The implementation is complete and fully wired.

## Threat Flags

None. This change is internal to the style restyle path and does not introduce new network endpoints, auth paths, file access patterns, or schema changes at trust boundaries.

## Self-Check: PASSED

- `crates/core/ui/elements/src/style/resolve.rs` — FOUND (modified)
- `crates/core/shell/src/shell/component/rendering.rs` — FOUND (modified)
- `crates/core/shell/src/shell/component.rs` — FOUND (modified)
- Commit `5eda5f1` — FOUND (feat: inheritance-aware targeted restyle)
- Commit `fc28722` — FOUND (feat: previous state storage + collect_interaction_changed_keys)
- Commit `6ca9a85` — FOUND (feat: wire targeted restyle into finalize_tree)
- `grep -c 'fn restyle_subtree_for_keys_with_index_and_inheritance' resolve.rs` — 1
- `grep -c 'previous_hovered_path' component.rs` — 2 (field + initializer)
- `grep -c 'fn collect_interaction_changed_keys' rendering.rs` — 1
- `grep -c 'affected_keys.is_empty()' rendering.rs` — 1
- All 101 mesh-core-elements tests pass
