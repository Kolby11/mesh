---
phase: 101-per-region-damage
reviewed: 2026-06-10T23:00:00Z
depth: standard
files_reviewed: 6
files_reviewed_list:
  - crates/core/presentation/src/wayland_surface/backend.rs
  - crates/core/frontend/host/src/lib.rs
  - crates/core/shell/src/shell/component/shell_component.rs
  - crates/core/shell/src/shell/component.rs
  - crates/core/shell/src/shell/runtime/render.rs
  - crates/core/presentation/src/lib.rs
findings:
  critical: 0
  warning: 2
  info: 3
  total: 5
status: issues_found
---

# Phase 101: Code Review Report

**Reviewed:** 2026-06-10T23:00:00Z
**Depth:** standard
**Files Reviewed:** 6
**Status:** issues_found

## Summary

Threaded `Vec<DamageRect>` end-to-end from `ShellComponent::take_present_damage()` through the render dispatch and presentation engine into per-rect `wl_surface::damage_buffer` calls, replacing the single unioned `Option<DamageRect>`. The core behaviors check out: per-rect damage loop is correct, 16-cap union fallback is correct, empty vec skip is correct, and the SHM copy region correctly stays unioned. No correctness bugs or security issues found. Two dead-code warnings and three informational items noted below.

---

## Warnings

### WR-01: Dead code — `_paint_damage` binding computed but never consumed

**File:** `crates/core/shell/src/shell/component/shell_component.rs:457-461`

**Issue:** The old `paint_damage` binding was retained with an underscore prefix after being detached from `merge_optional_damage`. The computation:

```rust
let _paint_damage = if effective_damage.full_surface {
    Some(surface_damage)
} else {
    effective_damage.rect
};
```

is wasted work — `effective_damage.rect` (bounding union) is computed and immediately discarded. The actual damage accumulation at lines 643–652 uses `effective_damage.rects` directly.

**Fix:** Remove the binding entirely. If `effective_damage.rect` is needed elsewhere, it should be computed at its point of use.

```rust
// Remove lines 457-461 entirely.
// The actual damage accumulation at lines 643-652 supersedes this.
```

---

### WR-02: Dead code — unreachable `unwrap_or_else` fallback in `protocol_damage_rects`

**File:** `crates/core/presentation/src/wayland_surface/backend.rs:447-452`

**Issue:** The >16-rect path in `protocol_damage_rects` folds all rects into a single union:

```rust
let union = rects
    .iter()
    .copied()
    .fold(None, |acc, r| Some(union_damage(acc, r)))
    .unwrap_or_else(|| full_damage(width, height));
```

The `fold` closure `|acc, r| Some(union_damage(acc, r))` always returns `Some(T)`. Since this arm is only reached when `rects.len() > 16 > 0`, the iterator is non-empty, so the fold always produces `Some(...)`. The `unwrap_or_else(|| full_damage(width, height))` fallback is structurally unreachable — it can never be tested, cannot trigger, and misleads readers into thinking there is a fallback path for empty input (which is already handled by the `rects.is_empty()` guard on line 441).

Note: this is **not** a correctness issue — the union is computed correctly. It is a code-quality dead-code issue.

**Fix:** Replace `.unwrap_or_else(...)` with `.unwrap()`:

```rust
let union = rects
    .iter()
    .copied()
    .fold(None, |acc, r| Some(union_damage(acc, r)))
    .unwrap();
```

Or alternatively, simplify to avoid the `Option` wrapper entirely:

```rust
let union = rects
    .iter()
    .copied()
    .reduce(|a, b| union_damage(Some(a), b))
    .unwrap(); // rects is non-empty here
```

---

## Info

### IN-01: Duplicated `union_damage` and `clip_damage` across module boundaries

**Files:**
- `crates/core/presentation/src/wayland_surface/backend.rs:410,455` (handles `Option<DamageRect>`, checks zero-width/height)
- `crates/core/shell/src/shell/component/shell_component.rs:1439,1458` (takes `DamageRect` directly, no degenerate checks)

**Issue:** `union_damage` and `clip_damage` are implemented independently in two crates with slightly different signatures and defensive checks. The `backend.rs` version is more defensive (handles `Option` and zero-dimension rects); the `shell_component.rs` version trusts its callers. Both are correct in their current context, but duplication invites divergence.

**Fix:** Consider extracting shared damage-rect geometry helpers into `mesh_core_render` (which already defines `DamageRect`). This is a non-blocking refactor opportunity.

---

### IN-02: Misleading comment on SHM copy empty-slice fallback

**File:** `crates/core/presentation/src/wayland_surface/backend.rs:730-734`

**Issue:** The comment says:

```rust
// If the slice is empty (shouldn't normally reach here due to
// the skip gate in render.rs), upload the full buffer.
```

The `or_else` fallback is in fact the correct and necessary behavior for an empty damage slice — it's not a "shouldn't reach here" defensive edge case. The `fold` on an empty iterator correctly returns `None`, and the `or_else` provides a full-damage default. This is sound defensive code; the comment just frames it as an anomaly.

**Fix:** Replace the comment with something that describes the design intent:

```rust
// Empty slices (e.g., from non-render callers) default to full-buffer copy.
```

---

### IN-03: `PresentationEngine::present()` convenience method appears unused outside the DevWindow path

**File:** `crates/core/presentation/src/lib.rs:110-127`

**Issue:** The public `present()` method creates a throwaway `DamageRect` and delegates to `present_with_damage(..., &[full])`. A grep of the shell crate shows no external callers — the only code path into `present()` is `present_with_damage` → `Backend::DevWindow(bridge).present(...)`. If there truly are no callers, this method is dead API surface.

**Fix:** Either:
- Remove `present()` if truly unused, or
- Document its intended audience (e.g., "for callers that always want full-buffer uploads"). If kept, consider inlining the `full_damage` construction into the `DevWindow` arm of `present_with_damage` since that's the only consumer.

---

_Reviewed: 2026-06-10T23:00:00Z_
_Reviewer: OpenCode (gsd-code-reviewer)_
_Depth: standard_
