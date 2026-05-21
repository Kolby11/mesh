# Phase 51: Painter Contract And Backend Boundary - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-21
**Phase:** 51-Painter Contract And Backend Boundary
**Areas discussed:** Command model, backend capabilities, MESH ownership boundary, Vello compatibility, migration inventory

---

## Command Model

| Option | Description | Selected |
|--------|-------------|----------|
| Helper-shaped trait | Keep methods like `fill_rect`, `fill_rounded_rect`, and `draw_box_shadow` as the backend API. | |
| Backend-neutral command stream | Define operations such as push clip, push layer, draw rect, draw path, draw image, apply filter, pop layer. | yes |
| Skia-shaped trait | Mirror Skia canvas/paint concepts directly in the trait. | |

**User's choice:** Backend-neutral command stream, inferred from milestone prompt.
**Notes:** User explicitly wants WebEngine/Qt-style structure and future Vello support, so the display-list-to-painter contract must not leak Skia-only types.

---

## Backend Capabilities

| Option | Description | Selected |
|--------|-------------|----------|
| Silent fallback | Missing backend features render approximately without surfacing diagnostics. | |
| Capability-gated behavior | Backends advertise supported commands and unsupported features become diagnostics or controlled fallback. | yes |
| Hard fail on unsupported | Rendering aborts when a backend lacks a command. | |

**User's choice:** Capability-gated behavior, selected as the conservative default.
**Notes:** This preserves observability and leaves room for Vello to start with a supported subset.

---

## MESH Ownership Boundary

| Option | Description | Selected |
|--------|-------------|----------|
| Skia owns the render engine | Push tree traversal, layout, ordering, and damage into Skia. | |
| MESH render engine, Skia painter backend | MESH owns tree/style/layout/display-list/damage/presentation; Skia owns raster/effects/layers. | yes |
| Full external renderer adoption | Replace the pipeline with WebEngine/Qt/Blitz-style ownership. | |

**User's choice:** MESH render engine, Skia painter backend.
**Notes:** This was the central decision from the user's prompt and prior discussion.

---

## Vello Compatibility

| Option | Description | Selected |
|--------|-------------|----------|
| Implement Vello now | Build production Skia and Vello backends in the same milestone. | |
| Shape API for Vello later | Keep command data backend-neutral and document Vello mapping/capability gaps. | yes |
| Ignore Vello until later | Optimize the API only for Skia and revisit later. | |

**User's choice:** Shape API for Vello later.
**Notes:** Vello is a future backend target, not Phase 51 implementation scope.

---

## Migration Inventory

| Option | Description | Selected |
|--------|-------------|----------|
| Inventory first | Phase 51 produces a migration map from current helper calls to command types. | yes |
| Refactor immediately | Phase 51 begins broad code migration without first documenting the target command model. | |
| Defer inventory | Planner discovers migration points during Phase 52. | |

**User's choice:** Inventory first, selected as the safest planning input.
**Notes:** Current code has helper-shaped calls across `painter.rs`, `tree.rs`, `widgets.rs`, `text.rs`, and debug overlay paths.

---

## the agent's Discretion

- Exact Rust type names, module layout, and migration sequencing may be chosen by the planner.
- The planner should prefer compile-safe incremental changes over a single broad painter rewrite.

## Deferred Ideas

- Full production Vello backend.
- Broad animation and motion-fidelity redesign.
- GPU compositor replacement.
- Audio popover transition delay polish.
- Module install requirement resolution.
