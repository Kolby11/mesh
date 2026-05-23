---
phase: 57
plan: 1
slug: retained-ordering-visual-bounds-damage-diagnostics
status: complete
completed: 2026-05-23
---

# Summary 57-01: Retained Ordering, Visual Bounds, Damage Diagnostics

## What Changed

- Added retained paint profiling counters for changed layout, changed paint, effect overflow, and fallback promotion.
- Published those counters through `RetainedPaintSnapshot` and the shell debug/profiling JSON payload.
- Changed retained subtree preclipping to use visual bounds so effect overflow intersecting a clip is not incorrectly discarded by layout-only bounds.
- Added display-list tests for z-index command ordering, visual-overflow preclip behavior, and the new profiling counters.
- Refactored shell debug paint JSON construction into helper functions to avoid the existing large `serde_json::json!` macro hitting recursion limits after adding fields.

## Files Changed

- `crates/core/frontend/render/src/display_list.rs`
- `crates/core/foundation/debug/src/lib.rs`
- `crates/core/shell/src/shell/component.rs`
- `crates/core/shell/src/shell/runtime/debug.rs`

## Verification

```bash
cargo fmt --check
nix develop -c cargo test -p mesh-core-render display_list_
nix develop -c cargo test -p mesh-core-shell retained_paint
```

All focused checks passed.
