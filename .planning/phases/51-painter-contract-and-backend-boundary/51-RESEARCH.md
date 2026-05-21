# Phase 51 Research: Painter Contract And Backend Boundary

**Date:** 2026-05-21
**Status:** Complete

## Executive Summary

Phase 51 should define MESH's painter boundary as a backend-neutral command contract below the retained display list and above concrete paint libraries. Skia should be the first authoritative backend, but retained data and render-object structures must remain free of Skia-only types so Vello can later implement the same contract with explicit capability differences.

The current code already has a useful starting point in `PaintBackend` and `SkiaPaintBackend`, but the trait is helper-shaped: `fill_rect`, `fill_rounded_rect`, `stroke_rounded_rect`, `draw_box_shadow`, and `apply_backdrop_filter` mirror today's call sites instead of representing a durable paint command stream. The next step is to introduce an explicit command model and backend capability contract while preserving compatibility wrappers during migration.

## Current Architecture Findings

- `RetainedDisplayList` owns command ordering, damage metrics, subtree reuse, and repaint policy. This must remain MESH-owned.
- `DisplayPaintCommand` currently stores widget/display-list state rather than backend-ready paint primitives. It should lower into painter commands, not become Skia-shaped.
- `FrontendRenderEngine` owns the active backend and is the natural compatibility layer for old helper calls during the migration.
- `PaintBackend` exists and already allows a Skia-backed implementation, but its methods should be replaced or supplemented with a `PainterCommand` execution contract.
- Existing painter tests cover pluggable backend construction, clipping, rounded rects, shadows, blur, and retained replay. They can be extended with contract and lowering tests before broader primitive migration.

## Recommended Contract Shape

Define backend-neutral value types in `crates/core/frontend/render/src/surface/painter/backend.rs` or a sibling module:

- `PainterCommand`
- `PainterClip`
- `PainterLayer`
- `PainterPaint`
- `PainterPath`
- `PainterImage`
- `PainterFilter`
- `PainterBlendMode`
- `PainterBackendCapabilities`
- `UnsupportedPainterFeature`
- `PainterDiagnostic`

The first command set should cover:

- `PushClip`
- `PopClip`
- `PushLayer`
- `PopLayer`
- `DrawRect`
- `DrawRoundedRect`
- `DrawPath`
- `DrawText`
- `DrawImage`
- `DrawShadow`
- `ApplyFilter`

Backend execution should be command-based:

```rust
pub(crate) trait PaintBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn capabilities(&self) -> PainterBackendCapabilities;
    fn execute_commands(
        &self,
        buffer: &mut PixelBuffer,
        commands: &[PainterCommand],
        diagnostics: &mut Vec<PainterDiagnostic>,
    );
}
```

Compatibility helper methods may remain temporarily, but should be implemented by building command slices and calling `execute_commands`.

## Backend Capability Model

Capabilities should distinguish:

- Supported directly by backend.
- Supported with controlled approximation.
- Unsupported and diagnosed.
- Deferred by phase, with no production reliance.

Unsupported visual behavior must not silently disappear. The backend should emit diagnostics that can later feed renderer debug/profiling payloads.

## Vello Compatibility Notes

Clean mapping candidates:

- Solid rects and rounded rects.
- Paths with fill/stroke.
- Basic clipping.
- Basic layers.
- Gradients and images, subject to image source representation.

Approximation or capability-gated candidates:

- Complex image filters.
- Backdrop filters.
- Blur/shadow combinations.
- SaveLayer behavior that depends on Skia-specific image-filter composition.
- Text primitives until MESH decides whether text remains outside the backend or is represented as glyph runs.

The contract should not expose `skia_safe::Canvas`, `Paint`, `Path`, `RRect`, `ImageFilter`, or `SaveLayerRec` outside the Skia backend.

## Migration Map Target

| Current Helper | Target Command Path |
|----------------|---------------------|
| `fill_rect_clipped` | `DrawRect` inside active clip |
| `fill_rounded_rect_clipped` | `DrawRoundedRect` inside active clip |
| `stroke_rounded_rect_clipped` | `DrawRoundedRect` with stroke paint |
| `draw_box_shadow` | `DrawShadow`, later Skia mask/image filter |
| `apply_backdrop_filter` | `PushLayer` / `ApplyFilter` / `PopLayer` |
| Selection fills | `DrawRect` |
| Debug overlay fills | `DrawRect` through compatibility wrapper |
| Widget control primitives | Lower to command stream before backend execution |

## Validation Architecture

Validation for Phase 51 should focus on contract integrity rather than pixel parity:

- Compile check for `mesh-core-render`.
- Unit tests proving the command model can represent the required command set.
- Tests proving compatibility helpers lower to painter commands.
- Static checks that retained display-list and render-object structures do not import `skia_safe`.
- Documentation checks for migration map and Vello compatibility notes.

Pixel parity belongs primarily to Phases 52-55.

## Risks

- A command model that mirrors Skia too closely will make Vello hard to add later.
- A command model that is too abstract will force each backend to reconstruct MESH semantics. Keep commands visual and concrete.
- Migrating helper methods too early can destabilize shipped rendering. Phase 51 should define the boundary and introduce compatibility wrappers; later phases should perform broad primitive migration.
