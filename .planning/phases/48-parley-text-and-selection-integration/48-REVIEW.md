---
phase: 48-parley-text-and-selection-integration
reviewed: 2026-05-19T00:00:00Z
depth: standard
files_reviewed: 3
files_reviewed_list:
  - crates/core/frontend/render/src/parley_adapter.rs
  - crates/core/frontend/render/src/lib.rs
  - crates/core/frontend/render/src/proof.rs
findings:
  critical: 1
  warning: 3
  info: 1
  total: 5
status: issues_found
---

# Phase 48: Code Review Report

**Reviewed:** 2026-05-19T00:00:00Z
**Depth:** standard
**Files Reviewed:** 3
**Status:** issues_found

## Summary

This phase adds Parley text shaping to the proof/evidence path (`FocusedTextEvidence`) and introduces selection cursor coordinate derivation via `Cursor::from_point` / `cursor.geometry`. The integration is gated behind the `renderer-parley` feature flag and uses a `thread_local` `FontContext` cache for performance.

The core shaping logic is structurally sound and `Cursor::from_point` / `cursor.geometry` do not panic on empty layouts (verified against parley 0.7.0 source). However there is one behavioral bug that produces silently wrong selection coordinates, a coordinate-space ambiguity between the feature-on and feature-off builds, a dead exported function, and a misleading comment that documents the wrong code path.

---

## Critical Issues

### CR-01: `no_fonts` path silently returns `(0.0, 0.0)` for selection coords instead of `None`

**File:** `crates/core/frontend/render/src/parley_adapter.rs:115-148`

**Issue:** In `shape_text_with_selection_evidence`, when `layout.len() == 0` (no fonts found), the `shaped` string is correctly set to `"parley_text::{content}::no_fonts"`. However, `cursor_point` is still called unconditionally on lines 147-148. On an empty layout, `Cursor::from_point` returns a cursor with `index = layout.data.text_len` and `geometry()` falls through to `last_line_cursor_rect`, which calls `layout.get(0)` — returning `None` on an empty layout — and returns `BoundingBox::default()` (all zeros). `cursor_point` therefore returns `Some((0.0, 0.0))`, not `None`.

The caller in `proof.rs` lines 211-216 has an `or_else` fallback that reads raw attribute coords, but it only fires when the parley call returns `None`. Because `cursor_point` returns `Some((0.0, 0.0))`, the fallback never fires and the stored `selection_anchor`/`selection_focus` are `(0.0, 0.0)` — meaningless coordinates that are neither the raw surface-space values nor real cursor positions.

The test in `proof.rs:564-569` documents incorrect behavior: the comment claims "Parley returned None for selection coords; selection_point fallback returns raw attribute values" but that is not what happens. The test only asserts `is_some()` and therefore does not catch the bug.

**Fix:** Guard `cursor_point` calls so they are only attempted when the layout actually has lines:

```rust
// In shape_text_with_selection_evidence, replace:
let anchor = cursor_point("_mesh_selection_anchor_x", "_mesh_selection_anchor_y");
let focus  = cursor_point("_mesh_selection_focus_x",  "_mesh_selection_focus_y");

// With:
let (anchor, focus) = if layout.len() > 0 {
    (
        cursor_point("_mesh_selection_anchor_x", "_mesh_selection_anchor_y"),
        cursor_point("_mesh_selection_focus_x",  "_mesh_selection_focus_y"),
    )
} else {
    (None, None)
};
```

This makes the `or_else` fallback in `proof.rs:211-216` fire correctly in the no-fonts case, and the comment at `proof.rs:564` will then be accurate.

---

## Warnings

### WR-01: Coordinate-space inconsistency between feature-on and feature-off builds

**File:** `crates/core/frontend/render/src/parley_adapter.rs:131-144` and `crates/core/frontend/render/src/proof.rs:220-228`

**Issue:** The two builds store semantically different values in `FocusedTextEvidence.selection_anchor` / `.selection_focus`:

- **Feature off** (`proof.rs:224-227`): raw surface-space floats parsed directly from `_mesh_selection_anchor_x/y` attributes.
- **Feature on** (`parley_adapter.rs:141-144`): `bb.x0 / bb.y0` from `cursor.geometry()`, which are text-layout-local coordinates (origin at the text box's `(0, 0)`).

Callers receiving a `FocusedProofSnapshot` cannot know which coordinate space the evidence is in without inspecting the feature flag. The test at `proof.rs:572-575` explicitly validates this difference but the struct field has no documentation indicating that its semantics change depending on the build. Downstream consumers (e.g. accessibility tooling, automated test harnesses) that compare snapshots across feature variants will get false mismatches.

**Fix:** Either document the coordinate-space contract on the `FocusedTextEvidence` fields, or — better — make both paths return the same space by adding origin translation to the non-parley path or surface-space re-translation to the parley path:

```rust
// In FocusedTextEvidence:
/// Selection anchor in text-local space (relative to the text origin,
/// NOT surface space). On `renderer-parley` builds this is derived from
/// `Cursor::geometry`; on default builds it is attribute-value minus text origin.
pub selection_anchor: Option<(f32, f32)>,
```

### WR-02: `shape_text_evidence` is `pub` but has no callers outside tests

**File:** `crates/core/frontend/render/src/parley_adapter.rs:78-96`

**Issue:** `shape_text_evidence` is declared `pub`. The only actual production call site uses `shape_text_with_selection_evidence` (proof.rs:203). `shape_text_evidence` is called exclusively from `#[cfg(test)]` blocks inside `parley_adapter.rs`. Because `parley_adapter` is a private module (`mod parley_adapter` in `lib.rs`, no `pub mod` or `pub use`), the visibility does not leak beyond the crate — but the declaration misleads readers into thinking the function is part of a public API.

**Fix:** Change `pub fn shape_text_evidence` to `pub(crate) fn shape_text_evidence` (or just remove it and inline into tests), since it only exists as a simpler test surface for `build_layout`.

```rust
// Change:
pub fn shape_text_evidence(
// To:
pub(crate) fn shape_text_evidence(
```

### WR-03: Misleading comment documents the wrong code path in `proof.rs`

**File:** `crates/core/frontend/render/src/proof.rs:564`

**Issue:** The comment reads:

```
// CI fallback path — Parley returned None for selection coords; selection_point
// fallback returns raw attribute values. Verify no panic and Some values.
```

As described in CR-01, Parley does not return `None` in the `no_fonts` case — it returns `Some((0.0, 0.0))`. The comment is factually wrong and will mislead anyone diagnosing no-font CI failures. The test that follows (`is_some()` assertion) passes but for the wrong reason.

**Fix:** After applying the CR-01 guard, update the comment to reflect actual behavior:

```rust
if evidence.parley_text.contains("::no_fonts") {
    // CI without fonts: Parley returned None (guarded by layout.len() > 0 check).
    // The or_else fallback in focused_text_evidence fires and sets raw attribute values.
    assert!(evidence.selection_anchor.is_some());
    assert!(evidence.selection_focus.is_some());
```

---

## Info

### IN-01: `LayoutContext` is re-allocated on every shaping call while `FontContext` is cached

**File:** `crates/core/frontend/render/src/parley_adapter.rs:47`

**Issue:** `FONT_CX` (expensive font discovery) is correctly cached as a `thread_local`. However `LayoutContext::new()` is created fresh on every `build_layout` call. In parley 0.7.0 `LayoutContext` holds internal scratch buffers used during shaping that it would reuse across calls if retained. On the proof path this runs once per text node per render tick.

`LayoutContext` is not `Send`, so a `thread_local` is appropriate. This is a minor efficiency note rather than a correctness issue and is out of v1 scope — flagged here for awareness when the proof path gets heavier usage.

**Fix (when needed):** Add a second `thread_local` alongside `FONT_CX`:

```rust
thread_local! {
    static FONT_CX: RefCell<FontContext> = RefCell::new(FontContext::new());
    static LAYOUT_CX: RefCell<LayoutContext<()>> = RefCell::new(LayoutContext::new());
}
```

---

_Reviewed: 2026-05-19T00:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
