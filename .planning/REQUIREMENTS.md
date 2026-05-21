# Requirements: MESH v1.10 Skia-Centric Painter API

**Defined:** 2026-05-21
**Core Value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.

## v1 Requirements

### Painter Contract

- [ ] **PAINT-01**: Maintainer can express retained display-list output through a high-level painter API with commands for push clip, pop clip, push layer, pop layer, draw rect, draw rounded rect, draw path, draw text, draw image, draw shadow, and apply filter.
- [ ] **PAINT-02**: Maintainer can add a future backend without changing MESH widget traversal, style resolution, layout, animation state, retained display-list ordering, damage selection, module boundaries, input handling, or presentation ownership.
- [ ] **PAINT-03**: Existing widget-tree and retained-display-list render paths both route paint execution through the same painter command/backend boundary.

### Skia Backend

- [ ] **SKIA-01**: Skia owns rasterization, antialiasing, paths, rounded rects, strokes, clipping, and blend modes for core shape primitives.
- [ ] **SKIA-02**: Skia owns shadows, blur, image filters, and saveLayer/layer behavior for supported visual effects.
- [ ] **SKIA-03**: Skia owns gradients and image drawing for supported painter commands.
- [ ] **SKIA-04**: Remaining MESH-owned software fallback code for painter primitives is removed or isolated behind non-authoritative compatibility tests.

### Layer And Effect Model

- [ ] **LAYER-01**: Node styles that require opacity, clipping, filters, backdrop filters, shadows, or blend behavior lower into explicit painter layer/effect commands.
- [ ] **LAYER-02**: Damage and visual bounds include pixels affected by shadows, filters, layer effects, and clipped descendants.
- [ ] **LAYER-03**: Stacking order and z-index behavior remain owned by MESH while the backend receives already ordered painter commands.

### Backend Extensibility

- [ ] **BACKEND-01**: Painter backend traits are documented with backend obligations, unsupported-feature behavior, and parity expectations.
- [ ] **BACKEND-02**: A future Vello backend can be sketched against the painter API without introducing Skia-specific concepts into display-list data.
- [ ] **BACKEND-03**: Backend selection remains reversible and observable through renderer diagnostics or debug/profiling payloads.

### Verification And Shipped Proof

- [ ] **VERIFY-01**: Automated tests prove Skia-backed rendering for core shapes, rounded corners, strokes, paths, shadows, blur/filter effects, layer clipping, images/gradients where supported, and retained display-list replay.
- [ ] **VERIFY-02**: Shipped navigation/audio surfaces render through the Skia-centric painter API without regressions in interaction, selection, profiling, or damage behavior.
- [ ] **VERIFY-03**: Renderer ownership and migration docs describe the WebEngine/Qt-style split and the Skia-now/Vello-later backend boundary.

## v2 Requirements

### Future Backends

- **VELLO-01**: Maintainer can enable a production Vello backend that implements the painter API with parity tests against Skia.
- **VELLO-02**: Runtime can choose between Skia and Vello backends per build or configuration without changing author-facing `.mesh` behavior.

### Future Text

- **TEXT-01**: Skia or another painter backend can own more text drawing primitives when it preserves MESH selection, shaping, font, and theme behavior.

## Out of Scope

| Feature | Reason |
|---------|--------|
| Replacing the MESH render engine wholesale with Skia, Vello, Blitz, WebEngine, or Qt | Skia/Vello are painter backends; MESH must retain tree, style, layout, animation, damage, module, input, and presentation ownership. |
| Full Vello backend implementation | This milestone defines an extensible contract and Skia implementation first; Vello production parity is a later milestone. |
| Full browser/Web platform compatibility | MESH remains a shell UI framework with bounded `.mesh` semantics, not a browser engine. |
| GPU compositor replacement | Presentation and compositor integration remain owned by `mesh-core-presentation`; this milestone targets the painter boundary. |
| Broad animation-system redesign | Animation state must continue to work through the new painter boundary, but motion-fidelity redesign is separate. |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| PAINT-01 | Phase 51 | Pending |
| PAINT-02 | Phase 51 | Pending |
| PAINT-03 | Phase 52 | Pending |
| SKIA-01 | Phase 52 | Pending |
| SKIA-02 | Phase 53 | Pending |
| SKIA-03 | Phase 53 | Pending |
| SKIA-04 | Phase 52 | Pending |
| LAYER-01 | Phase 53 | Pending |
| LAYER-02 | Phase 54 | Pending |
| LAYER-03 | Phase 54 | Pending |
| BACKEND-01 | Phase 51 | Pending |
| BACKEND-02 | Phase 51 | Pending |
| BACKEND-03 | Phase 54 | Pending |
| VERIFY-01 | Phase 55 | Pending |
| VERIFY-02 | Phase 55 | Pending |
| VERIFY-03 | Phase 55 | Pending |

**Coverage:**
- v1 requirements: 16 total
- Mapped to phases: 16
- Unmapped: 0

---
*Requirements defined: 2026-05-21*
*Last updated: 2026-05-21 after initial definition*
