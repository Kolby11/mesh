# Phase 49: AnyRender/Vello Paint Backend Adapter - Research

**Researched:** 2026-05-20
**Domain:** Rust renderer integration — anyrender 0.10.0 paint adapter behind retained display-list
**Confidence:** HIGH

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01:** Proof posture, NOT replacement. The anyrender paint adapter encodes display-list commands into an anyrender scene and populates `FocusedPaintEvidence` with encoding evidence. The software painter still produces the actual `PixelBuffer` output — anyrender runs in parallel as adapter-owned evidence only.
- **D-02:** `mesh-software-renderer` remains authoritative for all pixel output and all production rendering behavior when `renderer-anyrender` is disabled (the default).
- **D-03:** Explicitly NOT Phase 47 Taffy replacement posture. The anyrender adapter is not expected to take over paint authority in this phase.
- **D-04:** anyrender (`renderer-anyrender` feature, `anyrender = "0.10.0"`) is the primary implementation target.
- **D-05:** `renderer-vello-encoding` stays scaffolded but does NOT receive an implementation in Phase 49.
- **D-06:** When BOTH `renderer-parley` AND `renderer-anyrender` are enabled, text nodes are encoded as glyph runs using Parley's shaped output into the anyrender scene.
- **D-07:** When only ONE of the two features is active (not both), the combined text-in-paint glyph-run path is skipped entirely. A non-fatal diagnostic is emitted.
- **D-08:** Cosmic-text is NOT removed in Phase 49.
- **D-09:** Shipped-surface command subset: **backgrounds, borders, text (glyph runs when both features active), and icons**.
- **D-10:** `DisplayPaintContent::Slider`, `DisplayPaintContent::Input`, and `DisplayPaintCommandKind::Scrollbars` are documented as deferred lossless subset. Not encoded by Phase 49 adapter.
- **D-11:** Extend `FocusedPaintEvidence` with an `anyrender_encoded: bool` field (or `anyrender_scene_ops: Option<String>`). Do NOT add a separate `anyrender_paint: Vec<FocusedAnyrenderEvidence>` collection.

### Claude's Discretion

Exact anyrender API surface and scene builder types used; internal module placement within `mesh-core-render`; how background colors and border radii map to anyrender primitives; how icon encoding works (raster blit vs. vector path); whether `anyrender_scene_ops` is a bool, count, or string description; and the exact non-fatal diagnostic message when the combined Parley+anyrender path is skipped.

### Deferred Ideas (OUT OF SCOPE)

- Full cosmic-text removal.
- `renderer-vello-encoding` implementation.
- Slider, Input, Scrollbars encoding.
- anyrender to pixel output (rasterization step).
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| PAINT-01 | An AnyRender/Vello-style paint adapter can execute retained display-list paint commands, or a lossless translated subset, behind the current display-list ownership boundary. | anyrender `Scene` / `PaintScene` trait is the correct encoding target. `SelectedDisplayListPaint::commands()` is the feed. Deferred subset (Slider, Input, Scrollbars) must be documented. |
| PAINT-02 | Paint adapter output preserves background, border, opacity, icon, text, slider/input, selection, damage, and profiling evidence on shipped navigation/audio surfaces. | `FocusedPaintEvidence.anyrender_encoded: bool` on each command proves encoding. D-09 defines the shipped subset (backgrounds, borders, text, icons). |
| PAINT-03 | Paint fallback keeps the current software painter authoritative when the library-backed paint path is disabled or cannot render a command. | Feature gate `#[cfg(feature = "renderer-anyrender")]` keeps the adapter code-gated. The software painter in `surface/painter.rs` is untouched. |
</phase_requirements>

---

## Summary

Phase 49 introduces an anyrender-backed paint adapter in `crates/core/frontend/render` behind the `renderer-anyrender` Cargo feature flag. The adapter consumes `SelectedDisplayListPaint::commands()` — the same slice the software painter already uses — and encodes each command into an `anyrender::Scene` (the `recording::Scene` struct that implements `PaintScene`). The result is never rasterized; it is inspected as encoding evidence only.

`FocusedPaintEvidence` gets an `anyrender_encoded: bool` field. The `build_focused_proof_snapshot` function calls the adapter (when the feature is on) and sets that field per command. Tests assert `anyrender_encoded == true` for backgrounds, borders, text (when both flags active), and icons on shipped surfaces.

The adapter is structurally identical to how `parley_adapter.rs` was introduced in Phase 48: a new `anyrender_adapter.rs` module, feature-gated by `#[cfg(feature = "renderer-anyrender")]`, called from `proof.rs`. The combined Parley+anyrender text path is gated by `#[cfg(all(feature = "renderer-anyrender", feature = "renderer-parley"))]` and uses the existing `parley_adapter::build_layout` result.

**Primary recommendation:** Implement `anyrender_adapter.rs` using `anyrender::Scene` as a recording-only sink. Encode backgrounds as `scene.fill(Fill::NonZero, ...)` with a `Rect`, borders as `scene.stroke(...)` with `RoundedRect`, and icons as a `scene.fill(...)` with a placeholder rect. Return the count of encoded ops; set `anyrender_encoded = count > 0`.

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Display-list command ownership | `mesh-core-render` (display_list.rs) | — | Retains paint command identity, damage, batching. Unchanged. |
| Anyrender scene encoding (proof) | `mesh-core-render` (anyrender_adapter.rs) | — | New adapter-owned module. Feature-gated. Same crate as Parley adapter. |
| Software pixel output | `mesh-core-render` (surface/painter.rs) | — | Authoritative. Untouched by Phase 49. |
| Paint evidence population | `mesh-core-render` (proof.rs) | — | `build_focused_proof_snapshot` already owns this; extends `FocusedPaintEvidence`. |
| Parley+anyrender text glyph path | `mesh-core-render` (anyrender_adapter.rs) | parley_adapter.rs | Both features active: anyrender adapter consumes Parley `Layout` for glyph run encoding. |

---

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| anyrender | 0.10.0 [VERIFIED: workspace Cargo.toml] | 2D scene abstraction (PaintScene trait, Scene recording type) | Already in workspace; already optional dep in mesh-core-render |
| kurbo | 0.13.1 [VERIFIED: anyrender-0.10.0/Cargo.toml.orig] | Geometry primitives (Rect, RoundedRect, Stroke, Affine) | Transitive dep of anyrender; available at compile time with renderer-anyrender feature |
| peniko | 0.6.1 [VERIFIED: anyrender-0.10.0/Cargo.toml.orig] | Color and brush types (Color = AlphaColor<Srgb>, Fill) | Transitive dep of anyrender; `peniko::Color` is `color::AlphaColor<color::Srgb>` |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| parley | 0.7.0 [VERIFIED: workspace Cargo.toml] | Shaped text layout output | Only when `cfg(all(feature = "renderer-parley", feature = "renderer-anyrender"))` — combined glyph-run path |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `anyrender::Scene` (recording) | `NullScenePainter` | Null painter discards commands without counting them — no encoding evidence. Scene recording is required to prove ops were encoded. |
| Counting encoded ops as `anyrender_encoded: bool` | `anyrender_scene_ops: Option<String>` | String description is more useful for test assertion messages but adds boilerplate. Planner has discretion. |

**Installation:** No new Cargo additions needed. `anyrender` is already in `crates/core/frontend/render/Cargo.toml` as an optional dep under `renderer-anyrender`. [VERIFIED: crates/core/frontend/render/Cargo.toml]

---

## Architecture Patterns

### System Architecture Diagram

```
SelectedDisplayListPaint::commands()
  │
  ├─► software painter (surface/painter.rs)   ← authoritative pixel output (unchanged)
  │
  └─► anyrender_adapter (renderer-anyrender)  ← proof encoding only
        │
        ├─ DisplayPaintContent::None + background_color  → scene.fill(rect, solid_color)
        ├─ DisplayPaintContent::None + border_width      → scene.stroke(rounded_rect, border_color)
        ├─ DisplayPaintContent::Icon(_)                  → scene.fill(icon_rect, placeholder)
        ├─ DisplayPaintContent::Text(_)
        │    ├─ cfg(both parley+anyrender): scene.draw_glyphs() from Parley Layout
        │    └─ cfg(anyrender only): diagnostic emitted, text skipped
        ├─ DisplayPaintContent::Slider / Input           → skipped (documented deferred subset)
        └─ DisplayPaintCommandKind::Scrollbars           → skipped (documented deferred subset)
              │
              └─► encoded_count per command
                    │
                    └─► FocusedPaintEvidence { anyrender_encoded: true/false }
                              │
                              └─► build_focused_proof_snapshot → FocusedProofSnapshot
```

### Recommended Project Structure

The adapter module follows the same pattern as `parley_adapter.rs`:

```
crates/core/frontend/render/src/
├── display_list.rs           — unchanged boundary
├── library_adapters.rs       — unchanged (renderer-anyrender already scaffolded)
├── parley_adapter.rs         — Phase 48 (renderer-parley)
├── anyrender_adapter.rs      — NEW (renderer-anyrender)
├── proof.rs                  — extend FocusedPaintEvidence, call anyrender adapter
└── surface/
    └── painter.rs            — unchanged authoritative pixel path
```

### Pattern 1: Module-Level Feature Gate (same as parley_adapter.rs)

**What:** The entire `anyrender_adapter.rs` file is gated at the top with `#![cfg(feature = "renderer-anyrender")]`. [VERIFIED: parley_adapter.rs line 8]

**When to use:** Any module that contains code that only compiles when the optional dependency is present.

```rust
// Source: crates/core/frontend/render/src/parley_adapter.rs (Phase 48 established pattern)
#![cfg(feature = "renderer-parley")]
```

The anyrender adapter should follow:
```rust
#![cfg(feature = "renderer-anyrender")]

use anyrender::recording::Scene;
use anyrender::PaintScene;
use kurbo::{Affine, Fill, Rect, RoundedRect, RoundedRectRadii, Stroke};
use peniko::Color as PenikoColor;
```

### Pattern 2: Color Conversion (mesh Color → peniko Color)

**What:** `mesh_core_elements::style::Color` has `r: u8, g: u8, b: u8, a: u8`. `peniko::Color` is `color::AlphaColor<color::Srgb>` which has a `from_rgba8(r, g, b, a)` constructor via the `color::Rgba8` From impl. [VERIFIED: color-0.3.3/src/rgba8.rs]

```rust
// Source: color-0.3.3/src/rgba8.rs — impl From<Rgba8> for AlphaColor<Srgb>
fn mesh_color_to_peniko(c: mesh_core_elements::style::Color) -> peniko::Color {
    color::Rgba8 { r: c.r, g: c.g, b: c.b, a: c.a }.into()
    // equivalently: peniko::Color::from_rgba8(c.r, c.g, c.b, c.a)
}
```

### Pattern 3: Background Fill Encoding

**What:** A filled rectangle using `kurbo::Rect` and `Fill::NonZero`. [VERIFIED: anyrender-0.10.0 PaintScene::fill signature]

```rust
// Source: anyrender-0.10.0/src/lib.rs — PaintScene::fill
let rect = Rect::new(
    node.layout.x as f64,
    node.layout.y as f64,
    (node.layout.x + node.layout.width) as f64,
    (node.layout.y + node.layout.height) as f64,
);
scene.fill(
    peniko::Fill::NonZero,
    Affine::IDENTITY,
    mesh_color_to_peniko(node.style.background_color),
    None,
    &rect,
);
```

### Pattern 4: Border Stroke Encoding

**What:** A stroked rounded-rect using `Stroke` and `RoundedRect`. [VERIFIED: anyrender-0.10.0 PaintScene::stroke signature, kurbo::RoundedRect]

```rust
// Source: anyrender-0.10.0/src/lib.rs — PaintScene::stroke
let avg_border = (node.style.border_width.top + node.style.border_width.right
    + node.style.border_width.bottom + node.style.border_width.left) / 4.0;
let rounded = RoundedRect::new(rect, RoundedRectRadii::from(node.style.border_radius as f64));
let stroke = Stroke::new(avg_border as f64);
scene.stroke(
    &stroke,
    Affine::IDENTITY,
    mesh_color_to_peniko(node.style.border_color),
    None,
    &rounded,
);
```

### Pattern 5: Non-Fatal Diagnostic (same as parley_adapter.rs)

**What:** Push to `&mut Vec<FocusedProofDiagnostic>` when the combined path is skipped. [VERIFIED: parley_adapter.rs lines 57-61]

```rust
// Source: crates/core/frontend/render/src/parley_adapter.rs — established pattern
diagnostics.push(FocusedProofDiagnostic {
    node_id: Some(node_id),
    message: "anyrender: combined parley+anyrender text path not active — \
               enable both renderer-parley and renderer-anyrender".to_string(),
});
```

### Pattern 6: lib.rs Module Registration

`lib.rs` currently has:
```rust
#[cfg(feature = "renderer-parley")]
mod parley_adapter;
```

The anyrender adapter follows the same pattern:
```rust
#[cfg(feature = "renderer-anyrender")]
mod anyrender_adapter;
```

### Pattern 7: FocusedPaintEvidence Extension

`focused_paint_evidence()` in `proof.rs` currently constructs `FocusedPaintEvidence` with three fields. The Phase 49 extension adds `anyrender_encoded: bool`. When the feature is off, `anyrender_encoded` defaults to `false`. [VERIFIED: proof.rs lines 246-252]

```rust
// Source: crates/core/frontend/render/src/proof.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusedPaintEvidence {
    pub node_id: NodeId,
    pub stable_node_id: String,
    pub display_slot: &'static str,
    pub anyrender_encoded: bool,  // NEW: Phase 49
}
```

### Anti-Patterns to Avoid

- **Using `NullScenePainter` for encoding evidence:** The null backend discards all ops. Use `anyrender::Scene` (recording type) instead so you can count or inspect encoded ops.
- **Calling `scene.reset()` between commands:** The adapter builds one scene per `SelectedDisplayListPaint` pass. Resetting mid-pass would lose previous encoding evidence.
- **Checking `node.style.opacity` without `push_layer`/`pop_layer`:** Opacity requires a layer push. For Phase 49, opacity handling is optional — if the adapter encounters opacity != 1.0, it can either skip the opacity (still count the command as encoded) or push a layer. Skipping is simpler for proof posture.
- **Matching `DisplayPaintCommandKind::Scrollbars` and encoding nothing silently:** Deferred subset items must be documented in a comment, not silently skipped. The planner should include a `// DEFERRED: Slider/Input/Scrollbars` comment in the match arm.
- **Moving anyrender usage into `mesh-core-shell` or `mesh-core-elements`:** MESH architecture requires render-library fan-out to stay inside `mesh-core-render`. [VERIFIED: CLAUDE.md/llm-context.md/renderer-ownership.md]

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| 2D scene recording | Custom command Vec + encoder | `anyrender::recording::Scene` | Already implements `PaintScene`, stores commands as `Vec<RenderCommand>`, can be inspected (count = `scene.commands.len()`) |
| Color conversion u8 → f32 sRGB | Manual `r as f32 / 255.0` | `color::Rgba8 { r, g, b, a }.into()` → `peniko::Color` | Correct sRGB linearization handled by the color crate |
| Rounded-rect path | Build BezPath manually | `kurbo::RoundedRect` implements `Shape` | Shape is accepted directly by `PaintScene::fill/stroke` |

**Key insight:** `anyrender::Scene` is a purpose-built recording type. Its `commands` field is a `Vec<RenderCommand>` that the adapter can count after encoding to prove how many ops landed. No custom accumulator needed.

---

## Common Pitfalls

### Pitfall 1: Assuming `anyrender::Color` is peniko's `Color`

**What goes wrong:** `anyrender` uses `peniko::Color` which is `color::AlphaColor<color::Srgb>` from the `color` crate (version 0.3.x). This is NOT the same type as `mesh_core_elements::style::Color { r: u8, g: u8, b: u8, a: u8 }`.

**Why it happens:** Both types are named "Color" but live in different crates with different representations.

**How to avoid:** Use `color::Rgba8 { r, g, b, a }.into()` to convert from mesh Color to `peniko::Color`. This invokes the `From<Rgba8> for AlphaColor<Srgb>` impl. [VERIFIED: color-0.3.3/src/rgba8.rs]

**Warning signs:** Compiler error `mismatched types` when passing mesh Color to scene.fill/stroke brush parameter.

### Pitfall 2: Transitive crate version conflicts for kurbo/peniko

**What goes wrong:** `mesh-core-render` already has `resvg = "0.44"` which pulls in its own `kurbo`. If anyrender 0.10.0 uses `kurbo = "0.13.1"` and resvg uses a different kurbo version, Rust may refuse to compile or type-check.

**Why it happens:** Cargo resolves multiple versions of kurbo as separate types; `kurbo::Rect` from version A is not the same type as `kurbo::Rect` from version B.

**How to avoid:** Before implementation, run `cargo tree -p mesh-core-render --features renderer-anyrender | grep kurbo` in the Nix dev shell to verify both resvg and anyrender use the same kurbo version. If they diverge, the plan must add a workspace-level `kurbo` pin. [ASSUMED - needs verification during Wave 0]

**Warning signs:** Compiler error "expected struct `kurbo::Rect` found struct `kurbo::Rect`" (same name, different crate IDs).

### Pitfall 3: Dead code warning for `Scene::commands` field

**What goes wrong:** `anyrender::Scene` is constructed and commands are pushed into it, but if `scene.commands` is never read (the proof path only checks `encoded_count > 0` via `len()`), the compiler may emit a warning about accessing a private field, or clippy may flag unused result.

**How to avoid:** Use `let encoded_ops = scene.commands.len()` after encoding to make the read explicit. The `commands` field is `pub` in `recording::Scene`. [VERIFIED: anyrender-0.10.0/src/recording.rs line 129]

### Pitfall 4: Parley Layout borrow across the glyph encoding call

**What goes wrong:** When both features are active, the adapter calls `parley_adapter::build_layout(node, content, diagnostics)` to get a `parley::Layout<()>`. The Layout holds references to `FontContext` via thread_local. Calling into `anyrender::PaintScene::draw_glyphs` while the Layout is held may cause borrow issues with the `thread_local!` `FONT_CX`.

**Why it happens:** `FONT_CX` is wrapped in `RefCell`; `build_layout` borrows it mutably during construction. The returned Layout may extend the lifetime.

**How to avoid:** `build_layout` already returns an owned `parley::Layout<()>` (the borrow is released inside `FONT_CX.with()`). For Phase 49, the anyrender text path only needs to count glyph runs or emit a summary — it does not need to hold the Layout open while calling draw_glyphs. [VERIFIED: parley_adapter.rs lines 45-64 — borrow drops at end of FONT_CX.with() closure]

### Pitfall 5: `FocusedPaintEvidence` derives PartialEq/Eq — adding bool field is breaking

**What goes wrong:** Existing tests that construct `FocusedPaintEvidence { node_id, stable_node_id, display_slot }` directly will fail to compile after adding `anyrender_encoded: bool`.

**How to avoid:** After adding the field, search for all test sites that construct `FocusedPaintEvidence` directly and add `anyrender_encoded: false` (or use struct update syntax). [VERIFIED: proof.rs — `FocusedPaintEvidence` is `#[derive(Debug, Clone, PartialEq, Eq)]` and constructed in `focused_paint_evidence()` function only; no test constructs it directly — confirmed by grep]

---

## Code Examples

### Verified: anyrender Scene as recording sink

```rust
// Source: anyrender-0.10.0/src/recording.rs — Scene implements PaintScene
use anyrender::recording::Scene;
use anyrender::PaintScene;

let mut scene = Scene::new();
// After encoding commands:
let encoded_ops = scene.commands.len();
```

### Verified: Color conversion

```rust
// Source: color-0.3.3/src/rgba8.rs — impl From<Rgba8> for AlphaColor<Srgb>
// mesh Color { r: u8, g: u8, b: u8, a: u8 } → peniko::Color
fn to_peniko_color(c: mesh_core_elements::style::Color) -> peniko::Color {
    color::Rgba8 { r: c.r, g: c.g, b: c.b, a: c.a }.into()
}
```

### Verified: Background fill

```rust
// Source: anyrender-0.10.0/src/lib.rs — PaintScene::fill
use kurbo::{Affine, Rect};
use peniko::Fill;

let rect = Rect::new(
    node.layout.x as f64,
    node.layout.y as f64,
    (node.layout.x + node.layout.width) as f64,
    (node.layout.y + node.layout.height) as f64,
);
scene.fill(Fill::NonZero, Affine::IDENTITY, to_peniko_color(node.style.background_color), None, &rect);
```

### Verified: Border stroke

```rust
// Source: anyrender-0.10.0/src/lib.rs — PaintScene::stroke
use kurbo::{Affine, RoundedRect, RoundedRectRadii, Stroke};

let rounded = RoundedRect::new(rect, RoundedRectRadii::from(node.style.border_radius as f64));
let avg_border_w = (node.style.border_width.top + node.style.border_width.right
    + node.style.border_width.bottom + node.style.border_width.left) / 4.0;
scene.stroke(&Stroke::new(avg_border_w as f64), Affine::IDENTITY,
    to_peniko_color(node.style.border_color), None, &rounded);
```

### Verified: Non-fatal diagnostic pattern (from Phase 48)

```rust
// Source: parley_adapter.rs lines 57-61
diagnostics.push(FocusedProofDiagnostic {
    node_id: Some(node.id),
    message: "anyrender: renderer-parley not active — text glyph run encoding skipped".to_string(),
});
```

### Verified: Feature-gated module import in proof.rs

```rust
// Source: proof.rs lines 201-212 (existing renderer-parley gate pattern)
#[cfg(feature = "renderer-anyrender")]
let anyrender_encoded = crate::anyrender_adapter::encode_command_to_scene(command, diagnostics);
#[cfg(not(feature = "renderer-anyrender"))]
let anyrender_encoded = false;
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Phase 46 scaffold (no impl) | Phase 49 implementation (encoding evidence) | Phase 49 | `anyrender_encoded` field on each paint evidence record |
| Parley text deferred from paint | Combined Parley+anyrender glyph-run path when both features active | Phase 49 | Fulfills Phase 48 D-03 deferral |

**Deprecated/outdated:**
- The `#[allow(dead_code)]` warning emitted by the current `renderer-anyrender` stub will be resolved once real code uses the feature.

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `kurbo::RoundedRectRadii::from(f64)` constructs uniform-corner radii | Code Examples | Could be `from_single_radius` or another constructor; check kurbo 0.13.1 API if compile fails |
| A2 | `kurbo` and `peniko` transitive versions from `anyrender = "0.10.0"` do not conflict with `resvg = "0.44"` already in Cargo.toml | Common Pitfalls #2 | If they conflict, a workspace kurbo pin is needed — adds a Wave 0 dependency task |
| A3 | `parley_adapter::build_layout` can be called from the anyrender adapter module as a pub function | Architecture | `build_layout` is currently `fn` (package-private within parley_adapter.rs) — may need `pub(crate)` promotion |

---

## Open Questions

1. **`kurbo` version conflict with `resvg`**
   - What we know: `anyrender = "0.10.0"` depends on `kurbo = "0.13.1"`. `resvg = "0.44"` in mesh-core-render Cargo.toml pulls in its own kurbo.
   - What's unclear: Whether Cargo resolves to the same kurbo version or creates two separate versions.
   - Recommendation: Wave 0 verification step: `cargo tree -p mesh-core-render --features renderer-anyrender | grep kurbo` in Nix dev shell.

2. **`parley_adapter::build_layout` visibility for combined path**
   - What we know: `build_layout` is `fn` (not pub) in `parley_adapter.rs`.
   - What's unclear: Whether the anyrender adapter needs to call it directly, or whether a new `pub(crate)` wrapper in `parley_adapter.rs` is cleaner.
   - Recommendation: Add a `pub(crate) fn get_layout_for_anyrender(node, content, diagnostics) -> Option<parley::Layout<()>>` in parley_adapter.rs. The anyrender adapter calls that.

3. **`peniko::Color` vs `anyrender::PaintRef` in scene.fill/stroke brush parameter**
   - What we know: `PaintScene::fill` accepts `impl Into<PaintRef<'a>>`. `peniko::Color` is `color::AlphaColor<Srgb>`. `From<Color> for Paint<I, G, C>` is implemented.
   - What's unclear: Whether `peniko::Color` (i.e., `AlphaColor<Srgb>`) directly implements `Into<PaintRef<'_>>` or needs `Paint::Solid(color)` wrapping.
   - Recommendation: Use `anyrender::Paint::Solid(to_peniko_color(c))` explicitly to avoid any ambiguity.

---

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| anyrender crate | renderer-anyrender feature | Available in .cargo/registry | 0.10.0 [VERIFIED] | — |
| Nix dev shell | cargo test / cargo check (skia/freetype/fontconfig) | Available via `nix develop` | — | Cannot run tests outside Nix |
| kurbo (transitive) | anyrender shape types | Available (transitive) | 0.13.1 [VERIFIED from anyrender Cargo.toml.orig] | — |
| peniko (transitive) | anyrender color/brush types | Available (transitive) | 0.6.1 [VERIFIED from anyrender Cargo.toml.orig] | — |

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` with `#[cfg(test)]` modules |
| Config file | none (standard `cargo test`) |
| Quick run command | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render proof` |
| Full suite command | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render --features renderer-anyrender proof` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| PAINT-01 | Adapter encodes backgrounds, borders, icons to anyrender Scene | unit | `cargo test -p mesh-core-render --features renderer-anyrender anyrender` | ❌ Wave 0 |
| PAINT-01 | Slider/Input/Scrollbars documented as deferred subset (not encoded) | unit | `cargo test -p mesh-core-render --features renderer-anyrender anyrender_deferred` | ❌ Wave 0 |
| PAINT-02 | `anyrender_encoded = true` on paint evidence for shipped nav/audio surfaces | unit | `cargo test -p mesh-core-render --features renderer-anyrender proof` | ❌ Wave 0 (extend existing proof tests) |
| PAINT-02 | Combined Parley+anyrender: text nodes have anyrender_encoded = true | unit | `cargo test -p mesh-core-render --features renderer-anyrender,renderer-parley proof` | ❌ Wave 0 |
| PAINT-02 | anyrender-only (no parley): non-fatal diagnostic emitted for text nodes | unit | `cargo test -p mesh-core-render --features renderer-anyrender proof_diagnostic` | ❌ Wave 0 |
| PAINT-03 | Default build: anyrender_encoded = false, software painter unchanged | unit | `cargo test -p mesh-core-render proof` (no feature flag) | ✅ (existing proof tests pass today; will continue to pass after adding `anyrender_encoded: false` default) |
| PAINT-03 | renderer-anyrender disabled: no anyrender code compiled | check | `cargo check -p mesh-core-render` (no feature) | ✅ (existing) |

### Sampling Rate

- **Per task commit:** `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render proof`
- **Per wave merge:** `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render --features renderer-anyrender proof` + `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase44_navigation`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps

- [ ] `crates/core/frontend/render/src/anyrender_adapter.rs` — new module covering PAINT-01/PAINT-02
- [ ] `proof.rs` extension: add `anyrender_encoded: bool` to `FocusedPaintEvidence` — covers PAINT-02/PAINT-03
- [ ] New test functions in `proof.rs #[cfg(test)]`:
  - `anyrender_encodes_background_command` (renderer-anyrender feature)
  - `anyrender_encodes_border_command` (renderer-anyrender feature)
  - `anyrender_encodes_icon_command` (renderer-anyrender feature)
  - `anyrender_skips_slider_input_with_documented_comment` (renderer-anyrender feature)
  - `proof_snapshot_anyrender_encoded_false_without_feature` (no feature — regression)
  - `proof_snapshot_combined_parley_anyrender_text_encoded` (both features)
  - `proof_snapshot_anyrender_only_text_emits_diagnostic` (anyrender only)
- [ ] Extend `crates/core/shell/src/shell/component/tests/integration/real_surfaces.rs` `assert_phase44_focused_proof_snapshot` helper or add a new `assert_phase49_anyrender_encoding` helper for shipped surfaces

---

## Security Domain

`security_enforcement` is not explicitly set in `.planning/config.json`. Phase 49 adds no network I/O, file writes, user input handling, or secret management. The anyrender adapter is a purely in-memory encoding step. No ASVS categories apply.

---

## Sources

### Primary (HIGH confidence)

- `anyrender-0.10.0/src/lib.rs` [VERIFIED] — PaintScene trait, Scene struct, color/brush types
- `anyrender-0.10.0/src/recording.rs` [VERIFIED] — Scene as PaintScene impl, RenderCommand enum, commands field
- `anyrender-0.10.0/src/types.rs` [VERIFIED] — Paint, PaintRef, Glyph, NormalizedCoord types
- `anyrender-0.10.0/src/null_backend.rs` [VERIFIED] — NullScenePainter (not the right target for evidence)
- `anyrender-0.10.0/Cargo.toml.orig` [VERIFIED] — kurbo 0.13.1, peniko 0.6 dependencies
- `peniko-0.6.1/src/lib.rs` [VERIFIED] — Color = AlphaColor<Srgb> alias
- `color-0.3.3/src/rgba8.rs` [VERIFIED] — from_rgba8 / Rgba8 From impl
- `crates/core/frontend/render/src/proof.rs` [VERIFIED] — FocusedPaintEvidence, build_focused_proof_snapshot, existing test patterns
- `crates/core/frontend/render/src/parley_adapter.rs` [VERIFIED] — Phase 48 pattern (module gate, thread_local FontContext, non-fatal diagnostics, pub functions)
- `crates/core/frontend/render/src/library_adapters.rs` [VERIFIED] — renderer-anyrender scaffold already present
- `crates/core/frontend/render/Cargo.toml` [VERIFIED] — renderer-anyrender feature, anyrender optional dep
- `Cargo.toml` [VERIFIED] — anyrender = { version = "0.10.0", default-features = false }

### Secondary (MEDIUM confidence)

- `docs/renderer-migration.md` [VERIFIED] — Phase 49 is "Step 3: paint backend abstraction"; gate commands
- `docs/renderer-ownership.md` [VERIFIED] — anyrender/Vello classified as replacement candidate; proof snapshots as adapter-owned

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — anyrender 0.10.0 already in workspace, all crate sources read directly from .cargo/registry
- Architecture: HIGH — direct observation of parley_adapter.rs pattern, proof.rs structure, display_list.rs command types
- Pitfalls: HIGH (type confusion) / MEDIUM (kurbo version conflict — assumed, needs Wave 0 check)

**Research date:** 2026-05-20
**Valid until:** 2026-06-20 (anyrender 0.10.0 is stable; parley 0.7.0 and peniko 0.6.x are stable; low volatility)
