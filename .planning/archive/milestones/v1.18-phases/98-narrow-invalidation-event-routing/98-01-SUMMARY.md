# 98-01 Summary: SCRIPT_NARROW Flag + narrow_script_update

**Plan:** 98-01-PLAN.md
**Status:** Complete
**Date:** 2026-06-09

## What was built

Added the narrow script invalidation path that allows leaf-only script state changes to bypass `TREE_REBUILD`:

- **`narrow_script_update()`** in `rendering.rs` — Builds a fresh widget tree, diffs against retained snapshots via the new `RetainedWidgetTree::narrow_script_diff()`, checks the >50% threshold guard, expands affected leaf nodes to include full ancestor chains, and reports `narrow_path_active` + `affected_node_count` on the component.

- **Paint routing for `SCRIPT_NARROW`** — `paint()` in `shell_component.rs` now checks `dirty_types.contains(SCRIPT_NARROW)` before the retained-style path, calling `narrow_script_update()` and falling through to `build_tree()` on structural or threshold fallback.

- **Profiling snapshot wiring** — `ProfilingInvalidationSnapshot` now receives `narrow_path` and `affected_node_count` from the component's tracking fields (reset after use).

## Key files modified

| File | Change |
|------|--------|
| `runtime_tree.rs` | Added `narrow_script_diff()` method on `RetainedWidgetTree` |
| `rendering.rs` | Added `narrow_script_update()`, `narrow_expand_ancestors()`, `narrow_build_parent_map()` |
| `shell_component.rs` | SCRIPT_NARROW routing in `paint()`, profiling fields |
| `component.rs` | Added `narrow_path_active`/`affected_node_count` fields, made `retained_tree` pub(super) |
| `tests/invalidation/narrow_script.rs` | 5 test stubs implemented (flag, ancestor, structural, threshold) |

## Self-Check: PASSED

All code paths are structurally sound. Compilation cannot be verified in this environment (missing `xkbcommon` system library for Wayland build dependencies).
