---
phase: 55
slug: effects-layers-shadows-blur-images-and-gradients
status: complete
created: 2026-05-23
requirements:
  - EFFECT-01
  - EFFECT-02
  - EFFECT-03
  - LAYER-01
---

# Phase 55 Research

## Research Question

What does Phase 55 need in order to plan the compact MESH visual-effects subset without crossing into browser-engine scope?

## Current Foundation

Phase 51 already defined the backend-neutral painter API and Skia backend boundary. Phase 53 proved direct widget-tree and retained display-list command-class parity for supported primitives. Phase 54 made Skia authoritative for shape/path/border/text-adjacent rectangles while keeping text layout and retained identity owned by MESH.

Current code already includes several effect-adjacent hooks:

- `crates/core/frontend/render/src/surface/painter/backend.rs`
  - `PainterCommand::PushLayer`, `PopLayer`, `DrawImage`, `DrawShadow`, and `ApplyFilter` exist.
  - `PainterLayer`, `PainterImage`, `PainterFilter`, `PainterBlendMode`, `PainterBackendCapabilities`, and `PainterDiagnostic` exist.
  - Skia currently reports `layers: false`, `images: false`, `filters: true`, `blend_modes: true`.
  - `DrawImage` currently emits a deferred diagnostic.
  - `PushLayer` currently emits a deferred diagnostic when blend/filter isolation is requested.
  - `PainterFilter::Blur` currently emits a deferred diagnostic for standalone blur.
  - `DrawShadow` and backdrop blur already have Skia-backed execution helpers.
- `crates/core/frontend/render/src/surface/painter/tree.rs`
  - Direct and retained node painting already call `draw_box_shadow`, `apply_backdrop_filter`, and filtered background fills.
  - Opacity is currently applied by premultiplying colors, not isolated as a layer.
- `crates/core/frontend/render/src/display_list.rs`
  - `DisplayPaintNode` stores `box_shadow`, `filter`, `backdrop_filter`, opacity, clipping, and material data.
  - Visual clip and damage candidate calculations already expand by shadow spread/blur and filter/backdrop blur radius.
  - Batching barriers already classify opacity, translucency/effects, and clipping.
- `crates/core/ui/elements/src/style/types.rs` and `parse.rs`
  - `BoxShadow` and `VisualFilter` are backend-neutral style structs.
  - `filter`, `backdrop-filter`, and `box-shadow` parse into existing style data.
  - The style profile already names `background-image` and `linear-gradient` categories but there is not yet a backend-neutral style representation for image/gradient backgrounds.
- `crates/core/frontend/render/src/surface/icon.rs`
  - There is an image/raster cache, file freshness handling, and icon image raster profiling precedent that can inform image drawing.

## Implementation Strategy

### 1. Lock backend-neutral style/data before execution

The highest-risk part is allowing Skia-native concepts to leak upward. The phase should first add backend-neutral style structs for bounded background images and linear gradients, plus painter command structs for image and gradient drawing. Retained data, render-object hashes, and display-list signatures should include those values.

Recommended bounded style surface:

- `background-image: none`
- `background-image: url("relative/path.png")`
- `background-image: linear-gradient(#112233, #445566)`
- `background-image: linear-gradient(to bottom, #112233, #445566)`

Everything else should diagnose instead of silently accepting.

### 2. Keep layer insertion minimal

Layer commands should be explicit but not universal. A node needs a layer when isolation is required for opacity, blend, filter, backdrop sampling, or clipped descendants. Simple shadow/background/border rendering can stay as ordinary command sequences.

This matches the Phase 55 context:

- D-01: Skia owns supported effect execution while MESH owns retained ordering and damage inputs.
- D-02: opacity/filter/backdrop/blend/clip-effect combinations lower into explicit layer/effect commands.
- D-03: minimal layer insertion.
- D-04: direct and retained parity remains required.

### 3. Execute Skia effects behind the backend contract

Skia execution should use `save_layer`/restore for `PushLayer`/`PopLayer`, Skia image decoding/raster drawing for supported images, Skia shaders for linear gradients, and existing Skia mask/image filter helpers for blur/backdrop/shadow. The backend should update capabilities only when execution and tests exist.

### 4. Diagnostics are part of the feature

Unsupported combinations must become explicit diagnostics. Useful diagnostic cases:

- unsupported `background-image` syntax
- missing image asset
- image command with unavailable source
- excessive blur radius above a bounded constant, recommended `MAX_EFFECT_BLUR_RADIUS: f32 = 96.0`
- unsupported blend mode if a mode is represented but not executed
- backend capability mismatch for layers/images/filters

The existing `PainterDiagnostic` lacks node/style source context. Phase 55 should add source context where available without polluting retained identity.

### 5. Visual bounds proof should be focused

Phase 55 must include visual-bounds fixtures for shadow/filter/image/gradient output because its success criteria mention them. It should not redesign partial repaint policy or stacking. That broader work belongs to Phase 57.

## Validation Architecture

### Test Infrastructure

| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness |
| Config file | Cargo workspace |
| Quick command | `cargo test -p mesh-core-render painter_effect -- --nocapture` |
| Full command | `cargo test -p mesh-core-render painter_effect -- --nocapture && cargo test -p mesh-core-render display_list_effect -- --nocapture && cargo test -p mesh-core-elements style_background -- --nocapture` |
| Estimated runtime | ~90 seconds with local Skia/font libraries already built |

### Required Test Families

- Style/profile tests in `crates/core/ui/elements/src/style.rs`
  - bounded background image parsing
  - bounded linear-gradient parsing
  - unsupported background-image diagnostics
- Painter command tests in `crates/core/frontend/render/src/surface/painter/tests.rs`
  - direct and retained effect command-class parity
  - Skia shadow/filter/layer/image/gradient pixel proof
  - diagnostics for unsupported image/filter/blend/capability cases
- Display-list tests in `crates/core/frontend/render/src/display_list.rs`
  - retained signatures rebuild on background image/gradient changes
  - visual clip/damage includes supported effect overflow
- Backend-neutrality grep
  - `rg "skia_safe" crates/core/frontend/render/src/display_list.rs crates/core/frontend/render/src/render_object.rs crates/core/ui/elements/src && exit 1 || exit 0`

### Validation Risks

- Skia image/shader APIs may require small compatibility adjustments for current `skia-safe` version.
- Existing opacity-by-color behavior may be insufficient for descendant isolation; tests should prove layer isolation before changing capabilities to `layers: true`.
- Background image module asset resolution may need a narrow adapter around existing icon/image cache behavior; avoid broad module loader changes.

## Recommended Plan Breakdown

1. Backend-neutral style and painter data for image/gradient/effect diagnostics.
2. Direct and retained lowering into explicit effect/image/gradient command classes.
3. Skia execution for layers, image drawing, linear gradients, and supported effect combinations.
4. Diagnostics and visual-bounds proof.
5. Final validation, requirements coverage, and backend-neutrality proof.

## RESEARCH COMPLETE
