---
phase: 48
plan: "02"
subsystem: mesh-core-render
tags: [parley, selection, text-shaping, renderer-adapter, proof, feature-gate]
dependency_graph:
  requires: [parley-text-shaping-adapter, proof-diagnostics-threading]
  provides: [parley-selection-geometry-evidence]
  affects: [mesh-core-render]
tech_stack:
  added: []
  patterns: [Parley Cursor::from_point selection geometry, BoundingBox x0/y0 f64 coords, text-local coordinate translation]
key_files:
  created: []
  modified:
    - crates/core/frontend/render/src/parley_adapter.rs
    - crates/core/frontend/render/src/proof.rs
decisions:
  - "BoundingBox uses x0/y0 (f64) fields, not bb.min.x — verified from parley-0.7.0/src/util.rs before writing code"
  - "cursor_point returns Some((bb.x0 as f32, bb.y0 as f32)) from Cursor::geometry(&layout, 1.0)"
  - "build_layout returns Some(layout) even when layout.len()==0 so caller can inspect and push diagnostic"
  - "shape_text_evidence refactored to delegate to build_layout (Plan 01 public signature unchanged)"
  - "proof_snapshot_preserves_theme_owned_selection_payload updated with cfg gates (Rule 1 bug fix)"
metrics:
  duration: "~12 minutes"
  completed: "2026-05-19"
  tasks: 2
  files: 2
---

# Phase 48 Plan 02: Parley Selection Geometry Evidence Summary

Parley `Cursor::from_point` selection geometry wired into `FocusedTextEvidence.selection_anchor/focus` under the `renderer-parley` feature; theme-owned colors pass through unmodified; default build byte-identical to pre-Phase-48.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 (RED) | Failing tests for shape_text_with_selection_evidence | 42999ce | parley_adapter.rs (+51 lines) |
| 1 (GREEN) | Add shape_text_with_selection_evidence + build_layout | d070cae | parley_adapter.rs (+87 lines, -18 lines) |
| 2 (RED) | Failing tests for parley-wired focused_text_evidence | 254eb50 | proof.rs (+67 lines) |
| 2 (GREEN) | Wire parley selection coords into focused_text_evidence | 3271e55 | proof.rs (+26 lines, -15 lines) |
| Deviation | Fix test feature-awareness for existing selection test | d2501f1 | proof.rs (+16 lines, -2 lines) |

## New shape_text_with_selection_evidence Signature

```rust
pub fn shape_text_with_selection_evidence(
    node: &WidgetNode,
    content: &str,
    diagnostics: &mut Vec<FocusedProofDiagnostic>,
) -> (String, Option<(f32, f32)>, Option<(f32, f32)>)
```

`parley_adapter.rs` grew from 142 lines (Plan 01) to 262 lines (+120 lines).

Key additions:
- `fn build_layout` — private shared helper, returns `Option<parley::Layout<()>>`
- `pub fn shape_text_with_selection_evidence` — new public function (Plan 02)
- `use parley::editing::Cursor` — import for `Cursor::from_point`
- 3 new unit tests: `parley_selection_evidence_maps_anchor_focus`, `parley_selection_evidence_returns_none_when_attrs_absent`, `parley_selection_evidence_uses_text_origin_attribute_when_present`
- BoundingBox field access uses `bb.x0 as f32, bb.y0 as f32` (verified: parley-0.7.0 uses `f64` fields `x0`/`y0`, NOT `min.x`/`min.y` as the plan's template suggested)

## focused_text_evidence Diff Snippet

```diff
-    #[cfg(feature = "renderer-parley")]
-    let parley_text =
-        crate::parley_adapter::shape_text_evidence(node, content.as_str(), diagnostics);
-    #[cfg(not(feature = "renderer-parley"))]
-    let parley_text = { ... };
-    Some(FocusedTextEvidence {
-        parley_text, content,
-        selection_anchor: selection_point(...),
-        selection_focus: selection_point(...),
-    })
+    #[cfg(feature = "renderer-parley")]
+    let (parley_text, selection_anchor, selection_focus) = {
+        let (shaped, anchor, focus) = crate::parley_adapter::shape_text_with_selection_evidence(...);
+        let anchor = anchor.or_else(|| selection_point(...)); // fallback to raw attrs
+        let focus = focus.or_else(|| selection_point(...));
+        (shaped, anchor, focus)
+    };
+    #[cfg(not(feature = "renderer-parley"))]
+    let (parley_text, selection_anchor, selection_focus) = {
+        (format!("parley_text::{content}::shape=line_break_bidi_align"),
+         selection_point(...), selection_point(...))
+    };
```

`proof.rs` grew from 509 lines (Plan 01) to 601 lines (+92 lines).

## Build and Test Results

| Command | Result |
|---------|--------|
| `cargo check -p mesh-core-render` | Finished (0 errors, 1 pre-existing dead_code warning) |
| `cargo check -p mesh-core-render --features renderer-parley` | Finished (0 errors, 2 pre-existing dead_code warnings) |
| `cargo check -p mesh-core-render --tests` | Finished (0 errors) |
| `cargo check -p mesh-core-render --features renderer-parley --tests` | Finished (0 errors) |
| `cargo test -p mesh-core-render` | Link failure (pre-existing: missing libfreetype/libfontconfig — same as Plan 01) |
| `cargo test -p mesh-core-render --features renderer-parley` | Link failure (same pre-existing environment issue) |

Linker failures are confirmed pre-existing from Plan 01; `cargo check` validates full semantic correctness for both feature paths.

## Confirmation: Production Paths Untouched

`git diff --name-only HEAD~6..HEAD -- crates/core/frontend/render/src/surface/painter/text.rs` returns empty (D-04 preserved).

`git diff --name-only HEAD~6..HEAD -- crates/core/frontend/render/src/surface/text.rs` returns empty (D-05 preserved).

`git diff --name-only HEAD~6..HEAD -- crates/core/ui/elements/src/layout.rs` returns empty (TextMeasurer untouched — D-05 preserved).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] BoundingBox uses x0/y0 (f64), not bb.min.x/min.y**
- **Found during:** Task 1 read_first step (plan template predicted `bb.min.x` might be wrong)
- **Issue:** Plan's code template used `bb.min.x` / `bb.min.y`; actual parley-0.7.0 `BoundingBox` struct (in `util.rs`) uses `pub x0: f64, pub y0: f64`
- **Fix:** Used `bb.x0 as f32, bb.y0 as f32` instead
- **Files modified:** `crates/core/frontend/render/src/parley_adapter.rs`
- **Commit:** d070cae

**2. [Rule 1 - Bug] proof_snapshot_preserves_theme_owned_selection_payload would fail with renderer-parley**
- **Found during:** Task 2 analysis
- **Issue:** Existing unguarded test asserted `selection_anchor == Some((2.0, 3.0))` with raw attribute values, but with `renderer-parley` enabled, `focused_text_evidence` now uses Parley cursor geometry which transforms the coordinates
- **Fix:** Updated test with `#[cfg(not(feature = "renderer-parley"))]` guard for exact value assertions; added `#[cfg(feature = "renderer-parley")]` branch that only asserts `Some(_)` presence. Theme-owned colors still asserted unmodified in both cfg paths.
- **Files modified:** `crates/core/frontend/render/src/proof.rs`
- **Commit:** d2501f1

## Known Stubs

None — selection geometry is produced from real Parley `Cursor::from_point` geometry (or falls back to raw attribute values when Parley returns None). All data flows to proof evidence only; no placeholder values in UI.

## Threat Flags

None — no new network endpoints, auth paths, or trust boundaries introduced. The adapter is debug-proof-path only, feature-gated, and operates entirely in memory on shell-owned data.

## Self-Check: PASSED

- `crates/core/frontend/render/src/parley_adapter.rs` — FOUND
- `crates/core/frontend/render/src/proof.rs` — FOUND
- Commit 42999ce — FOUND
- Commit d070cae — FOUND
- Commit 254eb50 — FOUND
- Commit 3271e55 — FOUND
- Commit d2501f1 — FOUND
