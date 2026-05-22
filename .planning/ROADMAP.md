# Roadmap: MESH v1.10 Skia-Centric Painter API

## Milestones

- [ ] **v1.10 Skia-Centric Painter API** — Phases 51-55 planned
- [x] **v1.9 Renderer Library Integration** — Phases 46-50 shipped 2026-05-21
- [x] **v1.8 Rendering Engine Architecture** — Phases 42-45 shipped 2026-05-18

## Phase Summary

| # | Phase | Goal | Requirements | Success Criteria |
|---|-------|------|--------------|------------------|
| 51 | Painter Contract And Backend Boundary | 2/3 | In Progress|  |
| 52 | Skia Shape Primitive Migration | Route core shape, stroke, rounded-rect, path, clipping, blend, and retained replay primitives through Skia-backed painter commands. | PAINT-03, SKIA-01, SKIA-04 | 5 |
| 53 | Skia Effects, Layers, Gradients, And Images | Move shadows, blur/filter effects, saveLayer/layers, gradients, and image commands into Skia-owned primitives. | SKIA-02, SKIA-03, LAYER-01 | 5 |
| 54 | Retained Damage, Stacking, And Backend Observability | Preserve retained ordering, visual-bounds damage, backend reversibility, diagnostics, and profiling through the new painter boundary. | LAYER-02, LAYER-03, BACKEND-03 | 5 |
| 55 | Shipped-Surface Proof And Documentation | Prove the Skia-centric painter API on shipped surfaces and lock the WebEngine/Qt-style architecture docs. | VERIFY-01, VERIFY-02, VERIFY-03 | 5 |

## Phases

### Phase 51: Painter Contract And Backend Boundary

**Goal:** Define the extensible painter API that sits below the retained display list and above Skia/Vello-style backends.

**Requirements:** PAINT-01, PAINT-02, BACKEND-01, BACKEND-02

**Success criteria:**
1. `mesh-core-render` has a documented painter command model covering push/pop clip, push/pop layer, rect, rounded rect, path, text, image, shadow, filter, and backend capability behavior.
2. The painter API does not expose Skia-only types in retained display-list data or public render-object structures.
3. Existing direct paint helper calls have a migration map to painter commands.
4. Vello compatibility notes identify which commands can map cleanly, which need approximation, and which require future capability gates.

### Phase 52: Skia Shape Primitive Migration

**Goal:** Make Skia own core raster primitives and remove MESH-owned software shape fallback behavior from authoritative painter paths.

**Requirements:** PAINT-03, SKIA-01, SKIA-04

**Success criteria:**
1. Widget-tree painting and retained display-list replay both execute core shape primitives through the same painter backend boundary.
2. Rect, rounded rect, path, stroke, antialiasing, clip, and blend behavior use Skia canvas/paint/path primitives instead of per-pixel MESH implementations.
3. Border drawing uses backend stroke/fill commands and preserves current visual behavior on square and rounded borders.
4. Legacy MESH raster helpers are removed from authoritative painter paths or clearly isolated as tests/compatibility utilities.
5. Existing painter, display-list, and shipped-surface tests pass under the Nix graphics environment.

### Phase 53: Skia Effects, Layers, Gradients, And Images

**Goal:** Move visual effects and richer painter primitives into Skia-owned commands while preserving MESH style semantics.

**Requirements:** SKIA-02, SKIA-03, LAYER-01

**Success criteria:**
1. Box shadows, filter blur, backdrop-filter blur, opacity, and blend effects lower into explicit painter effect/layer commands.
2. Skia saveLayer/image-filter behavior owns supported layer effects rather than ad hoc MESH composition.
3. Gradient and image commands are represented in the painter API and implemented through Skia where current style/data support exists.
4. Unsupported effect combinations degrade through explicit diagnostics or capability records, not silent incorrect rendering.
5. Effect tests cover visual pixels outside layout bounds and clipped/layered effect behavior.

### Phase 54: Retained Damage, Stacking, And Backend Observability

**Goal:** Keep retained rendering correctness and observability intact while the backend boundary changes.

**Requirements:** LAYER-02, LAYER-03, BACKEND-03

**Success criteria:**
1. Damage and repaint selection include visual bounds from shadows, filters, layer effects, clips, and transformed descendants.
2. MESH remains responsible for z-order, stacking order, node traversal, and display-list command ordering before backend execution.
3. Backend selection and backend capability data are visible through renderer diagnostics or debug/profiling payloads.
4. Partial repaint and full-surface fallback behavior remain deterministic and covered by tests.
5. Rollback to the previous backend implementation remains possible during the milestone until Skia parity is accepted.

### Phase 55: Shipped-Surface Proof And Documentation

**Goal:** Prove the Skia-centric painter API preserves shipped behavior and document the final architecture boundary.

**Requirements:** VERIFY-01, VERIFY-02, VERIFY-03

**Success criteria:**
1. Automated tests cover Skia-backed core shapes, rounded corners, strokes, paths, shadows, blur/filter effects, layer clipping, retained display-list replay, and supported image/gradient commands.
2. Navigation bar and audio popover shipped-surface regressions pass with the new painter API.
3. Selection rendering, text measurement/drawing handoff, profiling payloads, and damage metrics remain compatible with existing debug proof.
4. Renderer ownership, renderer migration, and render crate docs describe the WebEngine/Qt-style split: MESH render engine, Skia painter backend, Vello future backend.
5. Requirements traceability is complete and every v1.10 requirement is mapped to exactly one phase.

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 51. Painter Contract And Backend Boundary | v1.10 | 3/3 | Ready to execute | — |
| 52. Skia Shape Primitive Migration | v1.10 | 0/? | Pending | — |
| 53. Skia Effects, Layers, Gradients, And Images | v1.10 | 0/? | Pending | — |
| 54. Retained Damage, Stacking, And Backend Observability | v1.10 | 0/? | Pending | — |
| 55. Shipped-Surface Proof And Documentation | v1.10 | 0/? | Pending | — |

## Deferred Context

- Full Vello backend production work is deferred until the painter API and Skia implementation prove the contract.
- Animation and motion-fidelity polish remains separate unless required to preserve existing animation behavior through the painter boundary.
- Audio popover transition delay polish remains accepted debt for a later animation milestone.
- Module install requirement resolution remains separate from painter backend architecture.
- Paused v1.6 keybind dispatch, conflict diagnostics, and accessibility proof remain separate from renderer paint backend work.

Run `$gsd-discuss-phase 51` to start the first phase.
