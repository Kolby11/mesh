# Phase 51 Patterns: Painter Contract And Backend Boundary

**Date:** 2026-05-21
**Status:** Complete

## Pattern Summary

Phase 51 should extend existing renderer patterns rather than replacing them. The existing retained display-list code is the model for MESH-owned ordering, damage, and identity; the existing `PaintBackend` is the model for backend ownership; and the existing renderer migration docs are the model for explicit rollback and observability.

## File Mapping

| New/Changed Surface | Closest Existing Pattern | Notes |
|---------------------|--------------------------|-------|
| `surface/painter/backend.rs` | Existing `PaintBackend` / `SkiaPaintBackend` | Evolve helper-shaped trait into command execution contract and capability reporting. |
| `surface/painter.rs` | Existing `FrontendRenderEngine` backend field and helper methods | Keep compatibility wrappers here while call sites migrate. |
| `surface/painter/tree.rs` | Current direct and retained paint traversal | Should lower node visuals into commands without absorbing traversal/layout ownership into backend. |
| `surface/painter/widgets.rs` | Current slider/input/icon primitives | Widget-specific paint remains MESH-owned; primitive drawing lowers to commands. |
| `surface/painter/text.rs` | Current `TextRenderer` handoff and selection fills | Text may remain delegated; selection rectangles should use painter commands. |
| `display_list.rs` | `DisplayPaintCommand`, `RetainedDisplayList`, metrics/damage types | Retained data must remain backend-neutral and should not import Skia types. |
| `render_object.rs` | Render-object sync and material hashing | Must remain MESH-owned; backend commands should not affect tree/material identity semantics. |
| `docs/renderer-migration.md` | Existing migration/rollback gates | Add migration map and backend-neutral contract details here. |
| `docs/renderer-ownership.md` | Existing ownership boundary doc | Keep WebEngine/Qt-style split explicit. |

## Existing Local Conventions

- Renderer-facing structs use explicit Rust value types rather than trait objects unless runtime polymorphism is needed.
- Tests live near the painter implementation in `surface/painter/tests.rs` when testing backend behavior.
- Nix-backed test commands are used for render tests that need native graphics/font dependencies.
- Migration docs should explain reversibility and observability whenever renderer architecture changes.

## Naming Guidance

Prefer names that describe MESH paint semantics:

- `PainterCommand`
- `PainterPaint`
- `PainterPath`
- `PainterLayer`
- `PainterClip`
- `PainterFilter`
- `PainterBackendCapabilities`
- `PainterDiagnostic`

Avoid names that leak a concrete backend into retained/public structures:

- `SkiaPaintCommand`
- `SkiaPath`
- `SkiaLayer`
- `SaveLayerRec`
- `RRect`
- `ImageFilter`

Skia-specific names are acceptable inside `SkiaPaintBackend` implementation blocks only.

## Compatibility Wrapper Pattern

During Phase 51, helper methods can stay as compatibility adapters:

1. Build a small command slice from the old helper arguments.
2. Call `execute_commands`.
3. Preserve existing behavior until later phases migrate call sites.
4. Emit diagnostics only when a command is unsupported or approximated.

This lets Phase 52 and Phase 53 migrate call sites incrementally without changing the backend contract again.
