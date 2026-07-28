# Phase 49: AnyRender/Vello Paint Backend Adapter - Pattern Map

**Mapped:** 2026-05-20
**Files analyzed:** 5 (3 new/modified source files + 2 doc files)
**Analogs found:** 4 / 5

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/core/frontend/render/src/anyrender_adapter.rs` | adapter/utility | transform (display-list → scene encoding) | `crates/core/frontend/render/src/parley_adapter.rs` | exact |
| `crates/core/frontend/render/src/proof.rs` | utility/evidence | transform | `crates/core/frontend/render/src/proof.rs` (self — extension) | self-modification |
| `crates/core/frontend/render/src/lib.rs` | config/wiring | — | `crates/core/frontend/render/src/lib.rs` (self — extension) | self-modification |
| `crates/core/frontend/render/src/parley_adapter.rs` | adapter/utility | transform | self | self-modification (visibility only) |
| `docs/renderer-migration.md` / `docs/renderer-ownership.md` | docs | — | existing Phase 46/48 entries | partial match |

---

## Pattern Assignments

### `crates/core/frontend/render/src/anyrender_adapter.rs` (new adapter module)

**Analog:** `crates/core/frontend/render/src/parley_adapter.rs`

**Module-level feature gate** (parley_adapter.rs line 8):
```rust
#![cfg(feature = "renderer-anyrender")]
```

The anyrender adapter opens with this exact pattern — entire file is dead unless the feature is on.

**Doc comment header** (parley_adapter.rs lines 1-7):
```rust
//! Parley text shaping adapter — proof-only evidence path.
//!
//! Adapter-owned per Phase 48 D-01/D-02: produces shaped text evidence for
//! `FocusedTextEvidence.parley_text` when the `renderer-parley` feature is on.
//! cosmic-text remains the authoritative production path (D-04). Never panics —
//! unsupported cases push a non-fatal `FocusedProofDiagnostic` (D-09).
```

Copy this pattern verbatim, substituting Phase 49 and anyrender specifics.

**Imports pattern** — derive from context; keys types to import:
```rust
#![cfg(feature = "renderer-anyrender")]

use anyrender::recording::Scene;
use anyrender::PaintScene;
use kurbo::{Affine, Rect, RoundedRect, RoundedRectRadii, Stroke};
use peniko::Fill;

use crate::display_list::{DisplayPaintCommandKind, DisplayPaintContent};
use crate::proof::FocusedProofDiagnostic;
use crate::{DisplayPaintCommand};
```

**Color conversion helper** — no analog in codebase; pattern from RESEARCH.md (verified against color-0.3.3/src/rgba8.rs):
```rust
fn to_peniko_color(c: mesh_core_elements::style::Color) -> peniko::Color {
    color::Rgba8 { r: c.r, g: c.g, b: c.b, a: c.a }.into()
}
```

**Non-fatal diagnostic pattern** (parley_adapter.rs lines 57-61):
```rust
diagnostics.push(FocusedProofDiagnostic {
    node_id: Some(node.id),
    message: format!("parley: no fonts found for text shaping (node {:?})", node.id),
});
```

Apply the same `FocusedProofDiagnostic { node_id: Some(...), message: "...".to_string() }` construction for the "combined parley+anyrender path not active" case.

**Public function signature pattern** (parley_adapter.rs line 78):
```rust
pub fn shape_text_evidence(
    node: &WidgetNode,
    content: &str,
    diagnostics: &mut Vec<FocusedProofDiagnostic>,
) -> String {
```

The anyrender adapter should expose:
```rust
/// Encode a display-list paint command into an anyrender recording Scene.
/// Returns the count of scene ops encoded (0 = nothing encoded or deferred subset).
/// Pushes non-fatal diagnostics for skipped cases; never panics.
pub fn encode_command_to_scene(
    command: &DisplayPaintCommand,
    diagnostics: &mut Vec<FocusedProofDiagnostic>,
) -> usize {
```

**Combined feature gate for Parley+anyrender text path** (pattern from proof.rs lines 201-218):
```rust
#[cfg(all(feature = "renderer-anyrender", feature = "renderer-parley"))]
{
    // encode glyph runs using parley_adapter output
}

#[cfg(all(feature = "renderer-anyrender", not(feature = "renderer-parley")))]
{
    diagnostics.push(FocusedProofDiagnostic {
        node_id: Some(command.node.id),
        message: "anyrender: combined parley+anyrender text path not active — \
                  enable both renderer-parley and renderer-anyrender".to_string(),
    });
}
```

**Deferred subset comment pattern** — deferred items must not be silently skipped; use a doc comment in the match arm:
```rust
DisplayPaintContent::Slider(_) | DisplayPaintContent::Input(_) => {
    // DEFERRED: Slider/Input encoding is documented as a lossless subset per PAINT-01.
    // Not encoded by the Phase 49 adapter.
    0
}
DisplayPaintCommandKind::Scrollbars => {
    // DEFERRED: Scrollbars encoding is documented as a lossless subset per PAINT-01.
    0
}
```

**Test module structure** (parley_adapter.rs lines 152-262):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::{Dimension, LayoutRect, WidgetNode};

    fn text_node(content: &str, width: f32) -> WidgetNode {
        let mut node = WidgetNode::new("text");
        node.attributes.insert("content".to_string(), content.to_string());
        node.layout = LayoutRect { x: 0.0, y: 0.0, width, height: 18.0 };
        // ... set computed_style fields
        node
    }

    #[test]
    fn parley_shapes_text_to_lines_width_height() {
        let node = text_node("Hello", 200.0);
        let mut diagnostics = Vec::new();
        let result = shape_text_evidence(&node, "Hello", &mut diagnostics);
        // assert result or diagnostic — never assert panic
    }
}
```

The anyrender adapter tests should follow the same structure: a helper to build a `DisplayPaintCommand` with a specific `DisplayPaintContent`, then assert the returned `usize` count. Tests should be headless-safe (no GPU, no rasterization — encoding to `anyrender::recording::Scene` is CPU-only).

---

### `crates/core/frontend/render/src/proof.rs` (extend `FocusedPaintEvidence`)

**Analog:** `crates/core/frontend/render/src/proof.rs` (self-modification at lines 45-50 and 246-252)

**Current struct** (proof.rs lines 45-50):
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusedPaintEvidence {
    pub node_id: NodeId,
    pub stable_node_id: String,
    pub display_slot: &'static str,
}
```

**After Phase 49 extension:**
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusedPaintEvidence {
    pub node_id: NodeId,
    pub stable_node_id: String,
    pub display_slot: &'static str,
    pub anyrender_encoded: bool,  // Phase 49: true when adapter encoded this command
}
```

**Current construction site** (proof.rs lines 246-252):
```rust
fn focused_paint_evidence(command: &DisplayPaintCommand) -> FocusedPaintEvidence {
    FocusedPaintEvidence {
        node_id: command.node.id,
        stable_node_id: command.node.id.to_string(),
        display_slot: display_slot_for_command(command),
    }
}
```

**After Phase 49 — add feature-gated anyrender call:**
```rust
fn focused_paint_evidence(
    command: &DisplayPaintCommand,
    diagnostics: &mut Vec<FocusedProofDiagnostic>,
) -> FocusedPaintEvidence {
    #[cfg(feature = "renderer-anyrender")]
    let anyrender_encoded = crate::anyrender_adapter::encode_command_to_scene(command, diagnostics) > 0;
    #[cfg(not(feature = "renderer-anyrender"))]
    let anyrender_encoded = false;

    FocusedPaintEvidence {
        node_id: command.node.id,
        stable_node_id: command.node.id.to_string(),
        display_slot: display_slot_for_command(command),
        anyrender_encoded,
    }
}
```

Note: `focused_paint_evidence` gains a `diagnostics` parameter; its call site in `build_focused_proof_snapshot` (proof.rs line 135-139) must be updated accordingly:

**Current call site** (proof.rs lines 135-139):
```rust
snapshot.paint = selected_paint
    .commands()
    .iter()
    .map(focused_paint_evidence)
    .collect();
```

**After Phase 49 — pass diagnostics:**
```rust
snapshot.paint = selected_paint
    .commands()
    .iter()
    .map(|cmd| focused_paint_evidence(cmd, &mut snapshot.diagnostics))
    .collect();
```

**Test impact:** Existing tests in `proof.rs #[cfg(test)]` that call `build_focused_proof_snapshot` do NOT directly construct `FocusedPaintEvidence` (verified: construction is only in `focused_paint_evidence()`). The addition of `anyrender_encoded: false` as a default will not break existing tests on the default build. Existing assertion `snapshot.paint[0].stable_node_id == "42"` (proof.rs line 336) will continue to pass.

New tests to add in the `#[cfg(test)]` block, each gated appropriately:
- `#[cfg(feature = "renderer-anyrender")] fn anyrender_encoded_true_for_background_command` — build a node with opaque background, assert `paint[0].anyrender_encoded == true`
- `#[cfg(not(feature = "renderer-anyrender"))] fn anyrender_encoded_false_without_feature` — assert `paint[0].anyrender_encoded == false` on the existing test nodes

---

### `crates/core/frontend/render/src/lib.rs` (register new module)

**Analog:** `crates/core/frontend/render/src/lib.rs` lines 4-5 (self-modification):
```rust
#[cfg(feature = "renderer-parley")]
mod parley_adapter;
```

**After Phase 49 — add anyrender module registration:**
```rust
#[cfg(feature = "renderer-parley")]
mod parley_adapter;

#[cfg(feature = "renderer-anyrender")]
mod anyrender_adapter;
```

No public exports needed — `anyrender_adapter` is called from `proof.rs` via `crate::anyrender_adapter::encode_command_to_scene(...)`, same as `parley_adapter` is called from `proof.rs` via `crate::parley_adapter::shape_text_with_selection_evidence(...)`.

---

### `crates/core/frontend/render/src/parley_adapter.rs` (promote `build_layout` visibility)

**Current visibility** (parley_adapter.rs line 29):
```rust
fn build_layout(
    node: &WidgetNode,
    content: &str,
    diagnostics: &mut Vec<FocusedProofDiagnostic>,
) -> Option<parley::Layout<()>> {
```

**Recommended change** — promote to `pub(crate)` or add a thin wrapper:
```rust
/// Wrapper for anyrender adapter: build and return a Parley Layout for text encoding.
/// Only available when both renderer-parley and renderer-anyrender are active.
#[cfg(feature = "renderer-anyrender")]
pub(crate) fn get_layout_for_anyrender(
    node: &WidgetNode,
    content: &str,
    diagnostics: &mut Vec<FocusedProofDiagnostic>,
) -> Option<parley::Layout<()>> {
    build_layout(node, content, diagnostics)
}
```

This follows the existing crate-internal visibility used by the parley adapter module's own public functions (`shape_text_evidence`, `shape_text_with_selection_evidence` are `pub` and called from `proof.rs` via `crate::parley_adapter::`).

---

## Shared Patterns

### Feature-Gated Module Declaration
**Source:** `crates/core/frontend/render/src/lib.rs` lines 4-5
**Apply to:** `anyrender_adapter.rs` registration in `lib.rs`
```rust
#[cfg(feature = "renderer-parley")]
mod parley_adapter;
```

### Module-Level `#![cfg(...)]` Gate
**Source:** `crates/core/frontend/render/src/parley_adapter.rs` line 8
**Apply to:** Top of `anyrender_adapter.rs`
```rust
#![cfg(feature = "renderer-parley")]
```

### Non-Fatal Diagnostic Push
**Source:** `crates/core/frontend/render/src/parley_adapter.rs` lines 57-61
**Apply to:** All skipped/unsupported cases in `anyrender_adapter.rs`
```rust
diagnostics.push(FocusedProofDiagnostic {
    node_id: Some(node.id),
    message: format!("..."),
});
```

### Feature-Paired `#[cfg]` / `#[cfg(not(...))]` Blocks in `proof.rs`
**Source:** `crates/core/frontend/render/src/proof.rs` lines 201-228
**Apply to:** `focused_paint_evidence()` for `anyrender_encoded` computation
```rust
#[cfg(feature = "renderer-parley")]
let (parley_text, ...) = { crate::parley_adapter::shape_text_with_selection_evidence(...) };

#[cfg(not(feature = "renderer-parley"))]
let (parley_text, ...) = { ... };
```

### `DisplayPaintContent` Match Pattern
**Source:** `crates/core/frontend/render/src/proof.rs` lines 254-273 (`display_slot_for_command`)
**Apply to:** The match arm in `encode_command_to_scene`
```rust
fn display_slot_for_command(command: &DisplayPaintCommand) -> &'static str {
    match &command.node.content {
        DisplayPaintContent::Text(_) => "Text",
        DisplayPaintContent::Icon(_) => "Icon",
        DisplayPaintContent::Slider(_) | DisplayPaintContent::Input(_) => "Generic",
        DisplayPaintContent::None => {
            if command.node.style.border_width.top > 0.0 || ... {
                "Border"
            } else if command.node.style.background_color.a > 0 {
                "Background"
            } else {
                "Generic"
            }
        }
    }
}
```

The anyrender adapter's match should use the same `DisplayPaintContent` variants and the same border/background detection logic from `DisplayPaintStyle`.

### `RendererLibraryStatus` Tracking (no change needed)
**Source:** `crates/core/frontend/render/src/library_adapters.rs` lines 37-43
The anyrender entry is already scaffolded:
```rust
RendererLibraryStatus {
    id: "anyrender",
    feature: "renderer-anyrender",
    role: "paint-experimental",
    enabled: cfg!(feature = "renderer-anyrender"),
    default_authority: CURRENT_RENDERER_AUTHORITY,
},
```
No modification needed to `library_adapters.rs` for Phase 49.

---

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `docs/renderer-migration.md` | docs | — | No pattern to copy; update the anyrender adoption status entry using Phase 46/48 text as style guide |
| `docs/renderer-ownership.md` | docs | — | No pattern to copy; update proof-snapshot boundary section using Phase 48 entry as style guide |

---

## Key Type Reference

From `crates/core/frontend/render/src/display_list.rs`:

- `DisplayPaintCommand { node: DisplayPaintNode, clip: DisplayListClip, kind: DisplayPaintCommandKind }`
- `DisplayPaintNode { id: NodeId, layout: LayoutRect, style: DisplayPaintStyle, content: DisplayPaintContent, scrollbars: DisplayScrollbars }`
- `DisplayPaintStyle { background_color: Color, border_color: Color, border_width: Edges, border_radius: f32, color: Color, padding: Edges, font_size: f32, ... }`
- `DisplayPaintContent::None | Text(DisplayTextPaint) | Input(DisplayInputPaint) | Slider(DisplaySliderPaint) | Icon(DisplayIconPaint)`
- `DisplayPaintCommandKind::Node | Scrollbars`
- `DisplayIconPaint { src: Option<String>, name: Option<String>, size: Option<u32> }`

From `crates/core/frontend/render/src/proof.rs`:

- `FocusedPaintEvidence { node_id, stable_node_id, display_slot }` — extend with `anyrender_encoded: bool`
- `FocusedProofDiagnostic { node_id: Option<NodeId>, message: String }` — push for skipped paths
- `build_focused_proof_snapshot(root, render_dirty, display_metrics, selected_paint) -> FocusedProofSnapshot` — call site for `focused_paint_evidence`

---

## Metadata

**Analog search scope:** `crates/core/frontend/render/src/`
**Files scanned:** `parley_adapter.rs`, `proof.rs`, `lib.rs`, `library_adapters.rs`, `display_list.rs`
**Pattern extraction date:** 2026-05-20
