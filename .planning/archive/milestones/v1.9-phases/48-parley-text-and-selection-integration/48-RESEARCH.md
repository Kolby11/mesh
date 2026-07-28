# Phase 48: Parley Text And Selection Integration - Research

**Researched:** 2026-05-18
**Domain:** Parley text shaping/layout adapter, proof evidence schema
**Confidence:** HIGH

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01:** Phase 48 adopts the Phase 46 proof posture for Parley, NOT the Phase 47 strict-replacement posture. Parley's real payoff is as input to the Vello paint backend (Phase 49). Replacing cosmic-text now and again when Vello arrives doubles the work.
- **D-02:** Keep cosmic-text as the authoritative text stack. Parley is added adapter-owned behind the `renderer-parley` Cargo feature, which already exists from Phase 46.
- **D-03:** Full cosmic-text removal is deferred to Phase 49 when Vello is also ready to consume Parley's layout output.
- **D-04:** The Parley adapter is paint/proof only — it does NOT replace the `TextMeasurer` used by Taffy layout. Layout sizing and intrinsic measurement continue to use cosmic-text's measurement path.
- **D-05:** Parley produces shaped text evidence (line positions, glyph data) for the proof snapshot but does not affect widget geometry or layout in Phase 48.
- **D-06:** The adapter should populate `FocusedTextEvidence.parley_text` in `proof.rs` for shipped navigation/audio text nodes with real Parley shaping output rather than the current placeholder string.
- **D-07:** Selection evidence (`selection_background`, `selection_foreground`, `selection_anchor`, `selection_focus`) in `FocusedTextEvidence` should be populated from Parley's cursor/line geometry where the feature is enabled, proving anchor/focus coordinates align with shaped glyph positions.
- **D-08:** When `renderer-parley` is disabled (default), all behavior is identical to Phase 47 output. No diagnostic noise from the Parley code path in the default build.
- **D-09:** When `renderer-parley` is enabled, unsupported text cases (complex emoji, unsupported script coverage, missing fontique font discovery) should surface as non-fatal diagnostics in the proof snapshot rather than panics or silent incorrect output.

### Claude's Discretion

The planner has discretion over exact module placement within `mesh-core-render`, the internal API shape of the Parley adapter struct, font discovery strategy with fontique (system fonts vs. embedded), and how Parley's `Layout` output maps to the existing `FocusedTextEvidence` schema. The proof snapshot schema in `proof.rs` should be respected — extend it minimally rather than replacing it.

### Deferred Ideas (OUT OF SCOPE)

- Full cosmic-text removal — deferred to Phase 49 alongside Vello paint backend integration.
- Parley feeding into TextMeasurer for layout sizing — deferred until Parley is authoritative for shaping (Phase 49+).
- fontique font discovery replacing fontdb/cosmic-text FontSystem — deferred to full replacement milestone.
- Audio Popover Transition Delay Polish — deferred to v1.10 animations/motion-fidelity milestone.
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| TEXT-01 | A Parley-backed text adapter shapes and lays out MESH text nodes while preserving current text content, alignment, wrapping, and measured-size behavior on shipped surfaces. | Parley `RangedBuilder` + `Layout::break_all_lines` + `layout.width()`/`layout.height()` produce shaping output; `focused_text_evidence()` is the injection point. |
| TEXT-02 | Parley-backed selection geometry preserves theme-owned selection colors, UTF-8 boundaries, anchor/focus evidence, and existing copy behavior. | `Selection::from_point`, `Cursor::from_point`, `Selection::geometry()` map anchor/focus screen coords to Parley `BoundingBox` coordinates; `FocusedTextEvidence.selection_anchor/focus` accept `(f32, f32)`. |
| TEXT-03 | Text fallback keeps the current text measurement/layout path authoritative for unsupported Parley cases and reports non-fatal diagnostics. | `#[cfg(feature = "renderer-parley")]` guards keep `cosmic-text` authoritative by default; `FocusedProofDiagnostic` carries non-fatal messages into the proof snapshot. |
</phase_requirements>

---

## Summary

Phase 48 adds a Parley-backed text shaping adapter to the `FocusedTextEvidence` proof path. The adapter is inserted into the `focused_text_evidence()` function in `proof.rs`, behind a `#[cfg(feature = "renderer-parley")]` guard. When the feature is disabled (default), the existing placeholder string `"parley_text::{content}::shape=line_break_bidi_align"` continues to populate `FocusedTextEvidence.parley_text`. When enabled, a real Parley `Layout<()>` is constructed with the node's text, font size, font weight, and max-width, and the resulting shaped output (line count, width, height, bidi direction) is serialized into the `parley_text` field. Selection evidence is additionally derived from Parley's `Cursor::from_point` and `Selection::geometry()` APIs when anchor/focus coordinates are available on the node.

The implementation lives entirely in `crates/core/frontend/render/`, never touches `text.rs` or `TextMeasurer`, and introduces a new module `crates/core/frontend/render/src/parley_adapter.rs` (or similar) that the `proof.rs` file conditionally calls. No `mesh-core-shell`, `mesh-core-elements`, or `TextMeasurer` trait surfaces are touched.

cosmic-text remains the sole authoritative path for text measurement, glyph rasterization, selection highlight rendering, and painter behavior. Parley produces evidence; cosmic-text produces pixels.

**Primary recommendation:** Add `src/parley_adapter.rs` to `mesh-core-render`, call it from `focused_text_evidence()` inside `#[cfg(feature = "renderer-parley")]`, and populate `FocusedTextEvidence.parley_text` with a serialized Parley layout summary. Selection evidence comes from Parley cursor/selection APIs using coordinates already present as WidgetNode attributes.

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Text shaping / line layout (proof) | Render adapter layer (`mesh-core-render`) | — | Parley adapter is proof-only, lives in the render crate behind a feature flag |
| Text measurement (layout sizing) | `TextMeasurer` via `SharedTextMeasurer` (cosmic-text) | — | Locked decision D-04: Parley must NOT replace `TextMeasurer` in Phase 48 |
| Glyph rasterization / painting | cosmic-text `TextRenderer` | — | cosmic-text stays authoritative for all pixel output |
| Selection highlight geometry (production paint) | `painter/text.rs` + cosmic-text `TextRenderer::selection_geometry` | — | Production paint path unchanged |
| Selection cursor evidence (proof only) | Parley adapter (`#[cfg(feature = "renderer-parley")]`) | — | Parley `Cursor::from_point` + `Selection::geometry()` populate proof-only fields |
| Proof snapshot construction | `proof.rs` `focused_text_evidence()` | Parley adapter (when feature on) | Injection point is `focused_text_evidence()` |
| Non-fatal diagnostics for unsupported cases | `FocusedProofDiagnostic` in `proof.rs` | — | Already exists in `FocusedProofSnapshot.diagnostics` |

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| parley | 0.7.0 | Rich text shaping, line breaking, BIDI reordering, alignment | Workspace dep locked at 0.7.0; newer 0.8.0/0.9.0 require Rust 1.88 (workspace floor is 1.85) [VERIFIED: Cargo.toml] |
| cosmic-text | 0.18 | Authoritative text measurement, glyph rasterization, selection geometry | Existing production path; must NOT be removed in Phase 48 [VERIFIED: crates/core/frontend/render/Cargo.toml] |

### Supporting (Parley re-exports)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| fontique (via parley) | re-exported | Font collection/discovery | `FontContext::new()` auto-discovers system fonts via fontique; no separate dep needed |
| swash (via parley) | re-exported | Glyph intrinsics, cluster analysis | Available via `parley::swash`; adapter can use cluster data if needed |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| parley 0.7.0 | parley 0.9.0 | 0.9.0 requires Rust 1.88 — workspace floor is 1.85; cannot upgrade without raising MSRV |

**No installation changes needed.** `parley = { workspace = true, optional = true }` already wired in `crates/core/frontend/render/Cargo.toml` under `renderer-parley` feature. [VERIFIED: Cargo.toml, render/Cargo.toml]

---

## Architecture Patterns

### System Architecture Diagram

```
WidgetNode (tag="text", attributes: content, font_size, ...)
  │
  ▼ [proof.rs: focused_text_evidence(node)]
  │
  ├─── #[cfg(not(feature = "renderer-parley"))]
  │    └─ placeholder string → FocusedTextEvidence.parley_text
  │
  └─── #[cfg(feature = "renderer-parley")]
       └─ parley_adapter::shape_text_evidence(node)
            │
            ├─ FontContext::new()           ← fontique system fonts
            ├─ LayoutContext::new()         ← scratch space
            ├─ ranged_builder(font_cx, text, 1.0, true)
            │    .push_default(FontSize(..))
            │    .push_default(FontWeight(..))
            │    .push_default(FontStack(..))
            ├─ layout.break_all_lines(max_width)
            ├─ layout.align(max_width, Alignment::Start, ..)
            │
            ├─ serialize layout summary → FocusedTextEvidence.parley_text
            │    e.g. "parley::lines=1::w=87.3::h=18.0::bidi=ltr"
            │
            ├─ [if anchor/focus attrs present]
            │    Cursor::from_point(layout, ax, ay) → anchor_bb
            │    Cursor::from_point(layout, fx, fy) → focus_bb
            │    → FocusedTextEvidence.selection_anchor / selection_focus
            │
            └─ [on Parley failure / unsupported case]
                 FocusedProofSnapshot.diagnostics.push(FocusedProofDiagnostic)
                 fallback to placeholder string (do not panic)

cosmic-text TextRenderer (UNCHANGED)
  └─ paint_frontend_tree → PixelBuffer   ← production pixels, unmodified
```

### Recommended File Layout

```
crates/core/frontend/render/src/
├─ parley_adapter.rs        ← NEW: #[cfg(feature = "renderer-parley")] module
│                                  shape_text_evidence() + shape_selection_evidence()
├─ proof.rs                 ← MODIFY: call parley_adapter::shape_text_evidence()
│                                     inside focused_text_evidence() under cfg guard
├─ lib.rs                   ← possibly expose parley_adapter if needed (optional)
├─ library_adapters.rs      ← NO CHANGE: already tracks renderer-parley status
└─ surface/text.rs          ← NO CHANGE
```

### Pattern 1: Parley RangedBuilder Layout Flow

**What:** Build a `Layout<()>` from plain text with font properties, run line breaking, inspect dimensions.
**When to use:** Inside `parley_adapter::shape_text_evidence()` to produce shaped evidence.

```rust
// Source: parley-0.7.0/src/lib.rs (doc example), verified in registry
#[cfg(feature = "renderer-parley")]
pub fn shape_text(text: &str, font_size: f32, font_weight: u16, max_width: Option<f32>) 
    -> parley::Layout<()>
{
    use parley::{FontContext, FontWeight, LayoutContext, StyleProperty};
    use parley::layout::Alignment;
    use parley::AlignmentOptions;

    let mut font_cx = FontContext::new();          // fontique system font discovery
    let mut layout_cx: LayoutContext<()> = LayoutContext::new();
    let mut builder = layout_cx.ranged_builder(&mut font_cx, text, 1.0, true);
    
    builder.push_default(StyleProperty::FontSize(font_size));
    builder.push_default(StyleProperty::FontWeight(
        FontWeight::new(font_weight as f32)
    ));
    // FontStack can use GenericFamily::SansSerif as fallback
    
    let mut layout: parley::Layout<()> = builder.build(text);
    layout.break_all_lines(max_width);
    layout.align(max_width, Alignment::Start, AlignmentOptions::default());
    layout
}
```

**Key output fields:**
- `layout.width()` — shaped text width in logical pixels
- `layout.height()` — shaped text height in logical pixels
- `layout.len()` — number of lines
- `layout.is_rtl()` — bidi direction

### Pattern 2: Parley Cursor and Selection Evidence

**What:** Given screen anchor/focus coordinates relative to the text node, construct Parley cursor positions and extract bounding geometry.
**When to use:** Inside `shape_selection_evidence()` when `_mesh_selection_anchor_x/y` and `_mesh_selection_focus_x/y` attributes are present.

```rust
// Source: parley-0.7.0/src/editing/cursor.rs, selection.rs — verified in registry
#[cfg(feature = "renderer-parley")]
fn selection_evidence(layout: &parley::Layout<()>, ax: f32, ay: f32, fx: f32, fy: f32)
    -> Option<((f32, f32), (f32, f32))>
{
    use parley::editing::Cursor;
    let anchor = Cursor::from_point(layout, ax, ay);
    let focus  = Cursor::from_point(layout, fx, fy);
    // cursor.geometry returns a BoundingBox (min_x, min_y, max_x, max_y)
    let a_bb = anchor.geometry(layout, 1.0);
    let f_bb = focus.geometry(layout, 1.0);
    Some((
        (a_bb.min.x, a_bb.min.y),
        (f_bb.min.x, f_bb.min.y),
    ))
}
```

### Pattern 3: Integration into focused_text_evidence()

**What:** Guard with `#[cfg(feature = "renderer-parley")]` so default builds are unchanged.
**When to use:** In `proof.rs` `focused_text_evidence()` function.

```rust
// Source: existing proof.rs structure
fn focused_text_evidence(node: &WidgetNode) -> Option<FocusedTextEvidence> {
    let content = node.attributes.get("content")?.clone();
    
    #[cfg(feature = "renderer-parley")]
    let parley_shaped = crate::parley_adapter::shape_text_evidence(node);
    
    #[cfg(not(feature = "renderer-parley"))]
    let parley_shaped = format!("parley_text::{content}::shape=line_break_bidi_align");

    Some(FocusedTextEvidence {
        parley_text: parley_shaped,
        content,
        selection_background: node.attributes.get("_mesh_selection_background").cloned(),
        selection_foreground: node.attributes.get("_mesh_selection_foreground").cloned(),
        selection_anchor: selection_point(node, "_mesh_selection_anchor_x", "_mesh_selection_anchor_y"),
        selection_focus: selection_point(node, "_mesh_selection_focus_x", "_mesh_selection_focus_y"),
    })
}
```

### Pattern 4: Non-Fatal Diagnostics for Unsupported Cases

**What:** Push a `FocusedProofDiagnostic` into the snapshot when Parley encounters an unsupported case, then return a fallback value instead of panicking.
**When to use:** In `parley_adapter.rs` when `FontContext::new()` finds no fonts or when line count is 0 for non-empty text.

```rust
// The proof snapshot already has a Vec<FocusedProofDiagnostic> — use it
// FocusedProofDiagnostic { node_id: Option<NodeId>, message: String }
// However, shape_text_evidence() returns a String, not a snapshot.
// Approach: Return a Result or use a dedicated diagnostics output parameter.
// Simpler: Return a (String, Option<String>) where the Option is a diagnostic.
// The caller in proof.rs injects diagnostics into snapshot.diagnostics.
```

**Note for planner:** `focused_text_evidence()` currently returns `Option<FocusedTextEvidence>`, not `FocusedProofSnapshot`. The function does not have access to the snapshot's `diagnostics` vec. The planner should decide the cleanest approach: either (a) return `(FocusedTextEvidence, Vec<FocusedProofDiagnostic>)` from the Parley adapter and thread diagnostics back to `collect_focused_nodes()`, or (b) use a thread-local or parameter-passed mutable diagnostic collector. Option (b) with a mutable `diagnostics: &mut Vec<FocusedProofDiagnostic>` parameter added to `focused_text_evidence()` is the least-invasive approach.

### Anti-Patterns to Avoid

- **Calling `text.rs` from `parley_adapter.rs`:** The adapter must not depend on `TextRenderer` or cosmic-text. It uses only `parley::*` types and the node's attributes.
- **Modifying `TextMeasurer` or `SharedTextMeasurer`:** Locked decision D-04 prohibits any changes to the measurement path.
- **Using `parley_adapter` without `#[cfg(feature = "renderer-parley")]`:** Every item in `parley_adapter.rs` must be guarded; the module itself should be `#[cfg(feature = "renderer-parley")] mod parley_adapter;` in `lib.rs` or `proof.rs`.
- **FontContext/LayoutContext per proof call:** Both types are documented as "global resource, one per app." Creating them per-call is expensive. The adapter should use `std::sync::OnceLock<Mutex<...>>` to cache them, or accept them as parameters. For proof-only/test paths a per-call allocation is acceptable but should be noted.
- **Panicking on empty font discovery:** `FontContext::new()` may find zero fonts in CI/headless environments. Always guard with `if layout.is_empty()` and emit a diagnostic rather than unwrapping.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Text line breaking and BIDI | Custom line-wrap logic | `layout.break_all_lines(max_width)` | Parley handles Unicode BIDI, soft/hard breaks, word-break policy |
| Cursor hit testing from screen coords | Custom hit-test loop | `Cursor::from_point(layout, x, y)` | Handles RTL affinity, line boundaries, cluster boundaries |
| Selection bounding boxes | Custom rect computation | `Selection::geometry(layout)` → `Vec<(BoundingBox, usize)>` | Handles multi-line spans, explicit newlines, trailing whitespace signals |
| Font weight mapping | Custom u16 → weight struct | `FontWeight::new(weight as f32)` | Parley's `FontWeight` accepts numeric weight values directly |

**Key insight:** Parley's `editing` module exposes `Cursor::from_point` and `Selection::geometry` precisely for building evidence from screen coordinates — do not reimplement cursor hit testing.

---

## Common Pitfalls

### Pitfall 1: FontContext Created Per Node
**What goes wrong:** Proof build creates a `FontContext::new()` for every text node, triggering fontique system font enumeration on every proof call.
**Why it happens:** `FontContext::new()` performs I/O (reads system font directories). In a 250ms poll cycle with multiple text nodes, this becomes 4+ I/O scans per second.
**How to avoid:** Cache `FontContext` and `LayoutContext<()>` in a `std::sync::OnceLock<Mutex<(FontContext, LayoutContext<()>)>>` static inside `parley_adapter.rs`. For Phase 48's proof-only path, even a simple `thread_local!` is acceptable since proof is not called from multiple threads simultaneously.
**Warning signs:** Proof snapshot construction takes > 5ms in profiling.

### Pitfall 2: Parley Alignment Enum Mismatch
**What goes wrong:** `TextAlign::Left/Center/Right` from `mesh-core-elements` is mapped incorrectly to Parley's `Alignment`.
**Why it happens:** Parley uses `Alignment::Start`/`Alignment::Middle`/`Alignment::End` not `Left`/`Center`/`Right` — the names differ from what MESH's `TextAlign` enum uses.
**How to avoid:** Map explicitly: `TextAlign::Left → Alignment::Start`, `TextAlign::Center → Alignment::Middle`, `TextAlign::Right → Alignment::End`. RTL text requires separate handling (see `layout.is_rtl()`).
**Warning signs:** Shaped width evidence mismatches expected center/right alignment in proof assertions.

### Pitfall 3: Selection Coordinates in Wrong Space
**What goes wrong:** Anchor/focus coordinates passed to `Cursor::from_point` are in surface space rather than text-node-local space.
**Why it happens:** `_mesh_selection_anchor_x/y` attributes store surface-relative coordinates. The text node's layout origin (`node.layout.x`, `node.layout.y`) and padding must be subtracted before passing to Parley.
**How to avoid:** Offset coordinates: `ax = anchor_x - node.layout.x - padding_left`, same for y. Compare with how cosmic-text's `selection_geometry()` in `painter/text.rs` subtracts `_mesh_selection_text_x/y` before calling `renderer.selection_geometry(...)`.
**Warning signs:** `Cursor::from_point` returns the first or last cluster regardless of where the user clicks.

### Pitfall 4: Empty Layout on CI
**What goes wrong:** In headless CI with no system fonts, `FontContext::new()` returns a collection with no fonts. Parley produces an empty layout (`layout.len() == 0`, `layout.width() == 0.0`). If the adapter asserts or unwraps, CI tests fail.
**Why it happens:** fontique reads `/usr/share/fonts` and `~/.local/share/fonts`. These directories may be empty in minimal CI containers.
**How to avoid:** Check `layout.len() == 0 && !text.is_empty()` after `break_all_lines`. Emit a `FocusedProofDiagnostic` with message `"parley: no fonts found for text shaping"` and fall back to the placeholder string. Do not panic.
**Warning signs:** `test_parley_shapes_navigation_bar_text` fails in CI with a 0-width layout.

### Pitfall 5: Feature Guard Missing on `mod parley_adapter`
**What goes wrong:** `mod parley_adapter;` in `lib.rs` or `proof.rs` without a `#[cfg(feature = "renderer-parley")]` attribute causes a compile error when the feature is disabled because the file uses `parley::*` types.
**Why it happens:** Rust resolves module declarations unconditionally by default.
**How to avoid:** Always declare: `#[cfg(feature = "renderer-parley")] mod parley_adapter;` and guard every public item inside with `#[cfg(feature = "renderer-parley")]` if the module boundary is split.
**Warning signs:** `cargo check` (default features) fails with "unresolved import parley".

---

## Code Examples

Verified patterns from official sources and codebase inspection:

### Build a Parley Layout from WidgetNode Attributes
```rust
// Source: parley-0.7.0/src/lib.rs example + codebase attribute conventions
#[cfg(feature = "renderer-parley")]
pub fn shape_text_evidence(node: &mesh_core_elements::WidgetNode) -> String {
    use parley::{FontContext, FontWeight, LayoutContext, StyleProperty};
    use parley::layout::Alignment;
    use parley::AlignmentOptions;
    use std::sync::OnceLock;
    use std::sync::Mutex;

    let content = match node.attributes.get("content") {
        Some(c) => c.as_str(),
        None => return format!("parley_text::empty"),
    };
    let font_size = node.computed_style.font_size.max(1.0);
    let font_weight = node.computed_style.font_weight;
    let max_width = if node.layout.width > 0.0 { Some(node.layout.width) } else { None };

    // Cache FontContext — fontique font discovery is expensive
    static FONT_CX: OnceLock<Mutex<FontContext>> = OnceLock::new();
    let font_cx_guard = FONT_CX.get_or_init(|| Mutex::new(FontContext::new()));
    let mut font_cx = font_cx_guard.lock().unwrap();

    let mut layout_cx: LayoutContext<()> = LayoutContext::new();
    let mut builder = layout_cx.ranged_builder(&mut *font_cx, content, 1.0, true);
    builder.push_default(StyleProperty::FontSize(font_size));
    builder.push_default(StyleProperty::FontWeight(FontWeight::new(font_weight as f32)));
    
    let mut layout: parley::Layout<()> = builder.build(content);
    layout.break_all_lines(max_width);
    layout.align(max_width, Alignment::Start, AlignmentOptions::default());

    if layout.len() == 0 && !content.is_empty() {
        // No fonts discovered — return diagnostic marker
        return format!("parley_text::{content}::no_fonts");
    }

    format!(
        "parley::lines={}::w={:.1}::h={:.1}::bidi={}",
        layout.len(),
        layout.width(),
        layout.height(),
        if layout.is_rtl() { "rtl" } else { "ltr" },
    )
}
```

### Derive Selection Evidence from Parley Cursors
```rust
// Source: parley-0.7.0/src/editing/cursor.rs::from_point + geometry
#[cfg(feature = "renderer-parley")]
pub fn parley_selection_evidence(
    layout: &parley::Layout<()>,
    node: &mesh_core_elements::WidgetNode,
) -> (Option<(f32, f32)>, Option<(f32, f32)>) {
    use parley::editing::Cursor;

    let ax = node.attributes.get("_mesh_selection_anchor_x")
        .and_then(|v| v.parse::<f32>().ok());
    let ay = node.attributes.get("_mesh_selection_anchor_y")
        .and_then(|v| v.parse::<f32>().ok());
    let fx = node.attributes.get("_mesh_selection_focus_x")
        .and_then(|v| v.parse::<f32>().ok());
    let fy = node.attributes.get("_mesh_selection_focus_y")
        .and_then(|v| v.parse::<f32>().ok());

    // Translate to text-local space (subtract node origin and padding)
    let pad_left = node.computed_style.padding.left;
    let pad_top  = node.computed_style.padding.top;
    let origin_x = node.layout.x + pad_left;
    let origin_y = node.layout.y + pad_top;

    let anchor = ax.zip(ay).map(|(x, y)| {
        let cursor = Cursor::from_point(layout, x - origin_x, y - origin_y);
        let bb = cursor.geometry(layout, 1.0);
        (bb.min.x, bb.min.y)
    });
    let focus = fx.zip(fy).map(|(x, y)| {
        let cursor = Cursor::from_point(layout, x - origin_x, y - origin_y);
        let bb = cursor.geometry(layout, 1.0);
        (bb.min.x, bb.min.y)
    });
    (anchor, focus)
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Hand-rolled line-break loop | Parley `break_all_lines()` | Phase 48 (proof only) | Proves BIDI-correct shaping without touching production paths |
| Placeholder `parley_text` string | Real Parley shaped output | Phase 48 (feature-gated) | Phase 49 can trust the proof schema when consuming Parley for Vello |

**Deprecated/outdated in context of this phase:**
- `parley_text: format!("parley_text::{content}::shape=line_break_bidi_align")` — the current placeholder in `focused_text_evidence()`. This is the target to replace when `renderer-parley` is enabled.

---

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| parley 0.7.0 | renderer-parley feature | Already in workspace Cargo.toml | 0.7.0 | — |
| cargo --features renderer-parley | CI gate | Verified compiles clean | n/a | — |
| System fonts (fontique) | Full shaping evidence | Present on dev machine; absent in minimal CI | varies | Emit FocusedProofDiagnostic + placeholder |

**`cargo check -p mesh-core-render --features renderer-parley` produces 0 errors.** [VERIFIED in this session]

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` (cargo test) |
| Config file | none |
| Quick run command | `cargo test -p mesh-core-render` |
| Full suite command | `cargo test -p mesh-core-render --features renderer-parley && cargo test -p mesh-core-render` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| TEXT-01 | Parley adapter shapes text and produces non-placeholder `parley_text` when feature enabled | unit | `cargo test -p mesh-core-render --features renderer-parley parley_adapter` | ❌ Wave 0 |
| TEXT-01 | `parley_text` is preserved as placeholder when feature disabled | unit | `cargo test -p mesh-core-render proof` | ✅ existing |
| TEXT-01 | Layout width/height in `parley_text` field matches expected proportions for shipped text | unit | `cargo test -p mesh-core-render --features renderer-parley parley_shapes` | ❌ Wave 0 |
| TEXT-02 | Parley selection evidence derives anchor/focus coords from Parley cursor geometry | unit | `cargo test -p mesh-core-render --features renderer-parley parley_selection` | ❌ Wave 0 |
| TEXT-02 | Selection colors are preserved unchanged through Parley adapter path | unit | `cargo test -p mesh-core-render --features renderer-parley` | ❌ Wave 0 |
| TEXT-03 | Default build (no feature) produces no parley-related compile errors or diagnostics | compile + test | `cargo test -p mesh-core-render` | ✅ existing (confirmed clean) |
| TEXT-03 | Empty layout on missing fonts emits FocusedProofDiagnostic, not panic | unit | `cargo test -p mesh-core-render --features renderer-parley parley_no_fonts` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p mesh-core-render`
- **Per wave merge:** `cargo test -p mesh-core-render && cargo test -p mesh-core-render --features renderer-parley`
- **Phase gate:** Both default and renderer-parley feature paths green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `crates/core/frontend/render/src/parley_adapter.rs` — covers TEXT-01, TEXT-02, TEXT-03
- [ ] Tests in `parley_adapter.rs` `#[cfg(test)]` module — `parley_shapes_text_to_lines_width_height`, `parley_selection_evidence_maps_anchor_focus`, `parley_no_fonts_emits_diagnostic_not_panic`

---

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — |
| V3 Session Management | no | — |
| V4 Access Control | no | — |
| V5 Input Validation | yes | Text content from WidgetNode attributes; Parley accepts arbitrary UTF-8 strings without injection risk |
| V6 Cryptography | no | — |

### Known Threat Patterns

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Malformed UTF-8 in node attribute | Tampering | Rust `String` guarantees valid UTF-8; no additional check needed |
| Very long text content causing OOM in Parley | DoS | Parley processes each node in proof path; proof is not a user-facing input surface; no additional mitigation needed |

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `Cursor::geometry()` returns a `BoundingBox` with `.min.x` and `.min.y` fields | Code Examples | Medium — BoundingBox struct may have different field names; verify against parley-0.7.0/src/util.rs |
| A2 | `FontContext::new()` is the correct constructor for the proof path (no manual font registration needed) | Standard Stack | Low — system fonts suffice for proof evidence; embedded fonts not needed |
| A3 | `node.computed_style.font_weight` is a `u16` (matching cosmic-text's convention) | Code Examples | Low — confirmed by TextLayoutKey struct in text.rs |
| A4 | Serializing Parley output as a `String` (e.g. `"parley::lines=1::w=87.3..."`) is the expected format for `FocusedTextEvidence.parley_text` | Architecture Patterns | Medium — CONTEXT.md only says "real Parley shaping output" not a specific format; planner has discretion |

---

## Open Questions

1. **FontContext caching strategy**
   - What we know: `FontContext::new()` is expensive; Parley docs say "one per app."
   - What's unclear: Whether `OnceLock<Mutex<FontContext>>` is the right approach or if the planner should thread it through from `FrontendRenderEngine`.
   - Recommendation: Use `thread_local! { static PARLEY_FONT_CX: RefCell<FontContext> }` for Phase 48's proof-only path. Thread injection is Phase 49 scope.

2. **Diagnostic threading from `focused_text_evidence()` to `FocusedProofSnapshot`**
   - What we know: `focused_text_evidence()` returns `Option<FocusedTextEvidence>`, not a diagnostics bag.
   - What's unclear: Whether to add a `diagnostics: &mut Vec<FocusedProofDiagnostic>` parameter or use a separate pass.
   - Recommendation: Pass `diagnostics: &mut Vec<FocusedProofDiagnostic>` through `collect_focused_nodes()` and `focused_text_evidence()`. Minimal signature change, cleanly threaded.

3. **`parley_text` string format**
   - What we know: The field is `String`; existing tests assert `.is_some()` and color fields, not the parley_text string content.
   - What's unclear: Whether a structured string or a human-readable debug format is preferred.
   - Recommendation: Use `format!("parley::lines={}::w={:.1}::h={:.1}::bidi={}",...)` — greppable, testable, Phase 49-readable.

---

## Sources

### Primary (HIGH confidence)
- `parley-0.7.0/src/lib.rs` [VERIFIED] — Usage example, FontContext/LayoutContext/RangedBuilder API
- `parley-0.7.0/src/layout/layout.rs` [VERIFIED] — `Layout::break_all_lines`, `Layout::align`, `Layout::width/height/len/is_rtl`
- `parley-0.7.0/src/editing/cursor.rs` [VERIFIED] — `Cursor::from_point`, `Cursor::geometry`
- `parley-0.7.0/src/editing/selection.rs` [VERIFIED] — `Selection::from_point`, `Selection::geometry`
- `parley-0.7.0/src/font.rs` [VERIFIED] — `FontContext::new()` wraps fontique Collection
- `parley-0.7.0/src/context.rs` [VERIFIED] — `LayoutContext::ranged_builder` signature
- `crates/core/frontend/render/src/proof.rs` [VERIFIED] — `focused_text_evidence()`, `FocusedTextEvidence` schema
- `crates/core/frontend/render/src/library_adapters.rs` [VERIFIED] — renderer-parley status tracking
- `crates/core/frontend/render/Cargo.toml` [VERIFIED] — `renderer-parley = ["dep:parley"]` feature
- `Cargo.toml` [VERIFIED] — `parley = { version = "0.7.0", default-features = false, features = ["std"] }`
- `cargo check -p mesh-core-render --features renderer-parley` [VERIFIED] — 0 errors, 1 warning (unrelated)

### Secondary (MEDIUM confidence)
- `crates/core/frontend/render/src/surface/painter/text.rs` [VERIFIED] — selection coordinate space conventions (text_x/text_y subtraction pattern)
- `crates/core/shell/src/shell/component/tests/restyle/selection.rs` [VERIFIED] — existing tests that reference `parley_text` field

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — Cargo.toml verified, feature compiles clean
- Architecture: HIGH — All integration points verified in codebase
- Pitfalls: HIGH — Derived from parley source inspection and existing codebase patterns
- Parley API specifics: HIGH — Verified in registry source (parley-0.7.0)
- BoundingBox field names: MEDIUM — Not separately verified (see A1)

**Research date:** 2026-05-18
**Valid until:** 2026-06-18 (parley 0.7.0 is pinned in workspace; API is stable for this version)
