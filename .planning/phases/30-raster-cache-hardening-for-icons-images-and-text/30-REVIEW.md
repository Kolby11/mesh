---
phase: 30
reviewed: 2026-05-12T13:37:08Z
depth: standard
files_reviewed: 12
files_reviewed_list:
  - crates/core/foundation/debug/src/lib.rs
  - crates/core/frontend/render/src/display_list.rs
  - crates/core/frontend/render/src/surface/glyph.rs
  - crates/core/frontend/render/src/surface/icon.rs
  - crates/core/frontend/render/src/surface/mod.rs
  - crates/core/frontend/render/src/surface/profiling.rs
  - crates/core/frontend/render/src/surface/text.rs
  - crates/core/shell/src/shell/component.rs
  - crates/core/shell/src/shell/component/shell_component.rs
  - crates/core/shell/src/shell/component/tests/invalidation/profiling.rs
  - crates/core/shell/src/shell/runtime/debug.rs
  - crates/core/shell/src/shell/tests.rs
findings:
  critical: 0
  warning: 0
  info: 0
  total: 0
status: passed_after_fix
---

# Phase 30: Code Review Report

**Reviewed:** 2026-05-12T13:37:08Z
**Depth:** standard
**Files Reviewed:** 12
**Status:** passed_after_fix

## Summary

Final blocker-only re-review found one remaining high issue in the cached icon opacity barrier path. It was fixed by checking node-level opacity and clipping before consulting cached icon resource opacity.

## Resolved Critical Issues

### CR-01: Cached Icon Opacity Skips Node-Level Barriers

**File:** `crates/core/frontend/render/src/display_list.rs:1767`

**Issue:** The `DisplayPrimitiveSlot::Icon` branch returns immediately from cached resource opacity before the existing node-level opacity and clipping checks at lines 1785-1791. Once a warmed file icon is classified as `Opaque`, `batch_barrier` returns `None` even if the icon node itself has `opacity < 1.0` or clipping overflow. That violates the Phase 30 requirement that cached resource opacity be used only conservatively, because source opacity is not enough to remove barriers introduced by node style.

**Fix:** Check node-level opacity and clip barriers before consulting cached icon resource opacity, or fold those checks into the icon branch and only return `None` for an opaque cached resource when the node is otherwise barrier-free.

**Resolution:** Fixed. `batch_barrier` now applies node opacity and clipping checks before cached icon resource opacity, and the display-list test covers an opaque cached file icon with `opacity < 1.0`.

```rust
if node.computed_style.opacity < 1.0 {
    return Some(DisplayBatchBarrier::Opacity);
}
if node.computed_style.overflow_x.clips_contents()
    || node.computed_style.overflow_y.clips_contents()
{
    return Some(DisplayBatchBarrier::Clip);
}

if matches!(slot, DisplayPrimitiveSlot::Icon) {
    return match cached_icon_resource_opacity(node) {
        crate::surface::icon::CachedResourceOpacity::Opaque => None,
        crate::surface::icon::CachedResourceOpacity::Translucent => {
            Some(DisplayBatchBarrier::Translucency)
        }
        crate::surface::icon::CachedResourceOpacity::Unknown => Some(DisplayBatchBarrier::Icon),
    };
}
```

---

_Reviewed: 2026-05-12T13:37:08Z_
_Reviewer: the agent (gsd-code-reviewer)_
_Depth: standard_
