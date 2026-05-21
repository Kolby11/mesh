# Phase 48: Parley Text And Selection Integration - Pattern Map

**Mapped:** 2026-05-18
**Files analyzed:** 4 (1 new, 3 modified)
**Analogs found:** 4 / 4

---

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `crates/core/frontend/render/src/parley_adapter.rs` | utility / adapter | transform | `crates/core/frontend/render/src/surface/text.rs` | role-match (same crate, text processing, feature-gated) |
| `crates/core/frontend/render/src/proof.rs` | utility / snapshot builder | transform | `crates/core/frontend/render/src/proof.rs` itself | self-modify — patterns extracted below |
| `crates/core/frontend/render/src/lib.rs` | config / module facade | — | `crates/core/frontend/render/src/lib.rs` itself | self-modify |
| `crates/core/frontend/render/src/library_adapters.rs` | config / status registry | — | existing file — NO CHANGE needed per D-02 / RESEARCH.md | exact — no changes required |

---

## Pattern Assignments

### `crates/core/frontend/render/src/parley_adapter.rs` (utility/adapter, transform)

**Primary analog:** `crates/core/frontend/render/src/surface/text.rs`
**Secondary analog:** `crates/core/frontend/render/src/surface/painter/text.rs` (for selection coordinate space)

This is a new file. Copy the structural conventions from `text.rs` (feature-gated caching via `thread_local!` + `RefCell`, no public re-export of internals, self-contained module). The selection coordinate-space pattern comes directly from `painter/text.rs`.

---

**Feature-gate module declaration pattern** — copy from how `library_adapters.rs` uses `cfg(feature)`:

Declare the module in `lib.rs` (or `proof.rs`) exactly as:
```rust
// lib.rs or proof.rs — always guard the mod declaration
#[cfg(feature = "renderer-parley")]
mod parley_adapter;
```

Every public item inside `parley_adapter.rs` must itself also be `#[cfg(feature = "renderer-parley")]` or the module declaration guard is sufficient to protect them (it is — Rust skips the file entirely when the `#[cfg]` on `mod` is false). Pattern source: RESEARCH.md Pitfall 5.

---

**Thread-local cache pattern** — copy from `text.rs` lines 29-31:

```rust
// crates/core/frontend/render/src/surface/text.rs lines 29-31
thread_local! {
    static RENDERER: RefCell<TextRenderer> = RefCell::new(TextRenderer::new());
}
```

For `parley_adapter.rs` adapt to cache `FontContext` (expensive fontique font discovery, runs once):

```rust
use std::cell::RefCell;

thread_local! {
    static FONT_CX: RefCell<parley::FontContext> =
        RefCell::new(parley::FontContext::new());
}
```

`LayoutContext<()>` is cheap to create per call (it is only scratch space) — do not cache it. Only `FontContext` needs the `thread_local!` treatment.

---

**Imports pattern** — follow the existing adapter crate conventions (no `use super::*`; explicit paths):

```rust
// parley_adapter.rs top-of-file imports pattern (no analog file — derived from crate conventions)
use mesh_core_elements::WidgetNode;
use parley::{FontContext, FontWeight, LayoutContext, StyleProperty};
use parley::layout::Alignment;
use parley::AlignmentOptions;
use std::cell::RefCell;
```

---

**Core adapter function signature** — derived from `proof.rs` `focused_text_evidence()` call site (lines 195-212) and RESEARCH.md Pattern 1:

```rust
// parley_adapter.rs — primary public function
pub fn shape_text_evidence(node: &WidgetNode) -> String { ... }

// parley_adapter.rs — secondary public function (called only when anchor/focus attrs present)
pub fn parley_selection_evidence(
    layout: &parley::Layout<()>,
    node: &WidgetNode,
) -> (Option<(f32, f32)>, Option<(f32, f32)>) { ... }
```

Both functions are the only public surface. The caller in `proof.rs` calls `crate::parley_adapter::shape_text_evidence(node)`.

---

**Selection coordinate-space subtraction** — copy from `painter/text.rs` lines 361-373:

```rust
// crates/core/frontend/render/src/surface/painter/text.rs lines 361-373
let text_x = node_attr_f32(node, "_mesh_selection_text_x");
let text_y = node_attr_f32(node, "_mesh_selection_text_y");

let geometry = renderer.selection_geometry(
    ...
    (anchor_x - text_x, anchor_y - text_y),
    (focus_x - text_x, focus_y - text_y),
);
```

The Parley adapter uses the same subtraction but with `node.layout.x + padding_left` as the origin (as documented in RESEARCH.md Pitfall 3 and Code Example 2). The attribute names `_mesh_selection_text_x` / `_mesh_selection_text_y` carry the text draw origin set by the painter. If those attrs are present, prefer them; otherwise fall back to `node.layout.x + padding_left`.

---

**Non-fatal diagnostic pattern** — copy from `proof.rs` lines 183-188 (the zero-size diagnostic):

```rust
// crates/core/frontend/render/src/proof.rs lines 183-188
if node.layout.width == 0.0 || node.layout.height == 0.0 {
    snapshot.diagnostics.push(FocusedProofDiagnostic {
        node_id: Some(node.id),
        message: "focused proof node has zero-size layout".to_string(),
    });
}
```

The Parley adapter cannot push directly into `snapshot.diagnostics` because `shape_text_evidence()` only returns a `String`. Follow the RESEARCH.md recommendation (Open Question 2): thread a `diagnostics: &mut Vec<FocusedProofDiagnostic>` parameter through `focused_text_evidence()` and `collect_focused_nodes()`. This is a minimal signature addition to two private functions.

---

**Test pattern** — copy from `proof.rs` `#[cfg(test)]` module (lines 250-455). Test helper uses `WidgetNode::new(tag)` + manual attribute insertion:

```rust
// crates/core/frontend/render/src/proof.rs lines 260-267
fn node(tag: &str, id: NodeId, layout: LayoutRect) -> WidgetNode {
    let mut node = WidgetNode::new(tag);
    node.id = id;
    node.layout = layout;
    node.computed_style.width = Dimension::Px(layout.width);
    node.computed_style.height = Dimension::Px(layout.height);
    node
}
```

Gate all `parley_adapter` tests with `#[cfg(feature = "renderer-parley")]` inside the `#[cfg(test)]` block so the default test run passes without the feature.

---

### `crates/core/frontend/render/src/proof.rs` (utility/snapshot builder, transform) — MODIFY

**Analog:** itself — the existing `focused_text_evidence()` function is the injection point.

**Current implementation** (lines 195-212) — the target to modify:

```rust
// crates/core/frontend/render/src/proof.rs lines 195-212
fn focused_text_evidence(node: &WidgetNode) -> Option<FocusedTextEvidence> {
    let content = node.attributes.get("content")?.clone();
    Some(FocusedTextEvidence {
        parley_text: format!("parley_text::{content}::shape=line_break_bidi_align"),
        content,
        selection_background: node.attributes.get("_mesh_selection_background").cloned(),
        selection_foreground: node.attributes.get("_mesh_selection_foreground").cloned(),
        selection_anchor: selection_point(
            node,
            "_mesh_selection_anchor_x",
            "_mesh_selection_anchor_y",
        ),
        selection_focus: selection_point(
            node,
            "_mesh_selection_focus_x",
            "_mesh_selection_focus_y",
        ),
    })
}
```

**Modified signature** — add diagnostics parameter (following RESEARCH.md Open Question 2):

```rust
fn focused_text_evidence(
    node: &WidgetNode,
    diagnostics: &mut Vec<FocusedProofDiagnostic>,
) -> Option<FocusedTextEvidence> {
    let content = node.attributes.get("content")?.clone();

    #[cfg(feature = "renderer-parley")]
    let parley_text = crate::parley_adapter::shape_text_evidence(node, content.as_str(), diagnostics);

    #[cfg(not(feature = "renderer-parley"))]
    let parley_text = format!("parley_text::{content}::shape=line_break_bidi_align");

    Some(FocusedTextEvidence {
        parley_text,
        content,
        selection_background: node.attributes.get("_mesh_selection_background").cloned(),
        selection_foreground: node.attributes.get("_mesh_selection_foreground").cloned(),
        selection_anchor: selection_point(
            node, "_mesh_selection_anchor_x", "_mesh_selection_anchor_y",
        ),
        selection_focus: selection_point(
            node, "_mesh_selection_focus_x", "_mesh_selection_focus_y",
        ),
    })
}
```

**`collect_focused_nodes` — thread diagnostics through** (lines 154-193). Change call site at line 164 from `focused_text_evidence(node)` to `focused_text_evidence(node, &mut snapshot.diagnostics)`.

No other proof.rs changes are needed. Struct definitions, `build_focused_proof_snapshot`, `build_accesskit_update`, and existing tests are untouched.

---

### `crates/core/frontend/render/src/lib.rs` (module facade) — MODIFY (if needed)

**Analog:** current `lib.rs` lines 1-31 — the pattern is straightforward module exposition.

```rust
// crates/core/frontend/render/src/lib.rs lines 1-5 (current)
pub mod display_list;
pub mod library_adapters;
pub mod proof;
pub mod render_object;
pub mod surface;
```

Add the parley_adapter module declaration here (or in `proof.rs` — either works, but `lib.rs` is the conventional location for crate-level module declarations):

```rust
#[cfg(feature = "renderer-parley")]
mod parley_adapter;
```

Note: `mod` not `pub mod` — the adapter is `crate::`-internal only; `proof.rs` calls it via `crate::parley_adapter::shape_text_evidence(...)`. Nothing in the public API surface changes.

No new `pub use` entries are needed — `parley_adapter` symbols are crate-internal.

---

### `crates/core/frontend/render/src/library_adapters.rs` (status registry) — NO CHANGE

**Analog:** itself. The `parley` entry in `renderer_library_statuses()` (lines 22-29) already tracks `renderer-parley` status:

```rust
// crates/core/frontend/render/src/library_adapters.rs lines 22-29
RendererLibraryStatus {
    id: "parley",
    feature: "renderer-parley",
    role: "text",
    enabled: cfg!(feature = "renderer-parley"),
    default_authority: CURRENT_RENDERER_AUTHORITY,
},
```

No modifications needed per RESEARCH.md recommended file layout and decision D-02.

---

## Shared Patterns

### Feature-flag guard pattern
**Source:** `crates/core/frontend/render/src/library_adapters.rs` — `cfg!(feature = "renderer-parley")` usage throughout.
**Apply to:** Every item touching `parley::*` types in `parley_adapter.rs`, and every conditional branch in `proof.rs` that calls the adapter.

Pattern:
```rust
#[cfg(feature = "renderer-parley")]
// ... code that uses parley::* types

#[cfg(not(feature = "renderer-parley"))]
// ... fallback that produces the placeholder string
```

### Node attribute parsing pattern
**Source:** `crates/core/frontend/render/src/surface/painter/text.rs` lines 333-360 and `proof.rs` lines 215-218.
**Apply to:** `parley_adapter.rs` wherever reading `_mesh_selection_*` or `content` attributes from `WidgetNode`.

```rust
// proof.rs lines 215-218
fn selection_point(node: &WidgetNode, x_key: &str, y_key: &str) -> Option<(f32, f32)> {
    let x = node.attributes.get(x_key)?.parse::<f32>().ok()?;
    let y = node.attributes.get(y_key)?.parse::<f32>().ok()?;
    Some((x, y))
}
```

### Non-fatal diagnostics push pattern
**Source:** `crates/core/frontend/render/src/proof.rs` lines 183-188.
**Apply to:** `parley_adapter.rs` for the empty-layout / no-fonts case (RESEARCH.md Pitfall 4).

```rust
// proof.rs lines 183-188
snapshot.diagnostics.push(FocusedProofDiagnostic {
    node_id: Some(node.id),
    message: "...".to_string(),
});
```

In `parley_adapter.rs`, diagnostics arrive via a `&mut Vec<FocusedProofDiagnostic>` parameter and are pushed the same way.

### Test helper node construction
**Source:** `crates/core/frontend/render/src/proof.rs` lines 260-267.
**Apply to:** `parley_adapter.rs` `#[cfg(test)]` module — use the same `WidgetNode::new(tag)` + manual attribute insertion + manual `layout` assignment approach. Do NOT construct full render pipelines in adapter unit tests.

---

## No Analog Found

None. All four files have clear analogs in the existing codebase.

---

## Metadata

**Analog search scope:** `crates/core/frontend/render/src/`
**Files read:** `proof.rs` (456 lines), `library_adapters.rs` (107 lines), `surface/text.rs` (first 80 lines), `surface/painter/text.rs` (lines 1-420), `lib.rs` (31 lines), `Cargo.toml`
**Pattern extraction date:** 2026-05-18
