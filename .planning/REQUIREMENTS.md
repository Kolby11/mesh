# Requirements: MESH v1.10 Painter Engine

**Defined:** 2026-05-21
**Expanded:** 2026-05-22
**Core Value:** MESH should let plugin authors build distinctive shell UI and
service integrations while the shell stays observable, deterministic, and
responsive on real interaction paths.

## v1 Requirements

### Painter Contract

- [x] **PAINT-01**: Maintainer can express retained display-list output through a high-level painter API with commands for push clip, pop clip, push layer, pop layer, draw rect, draw rounded rect, draw path, draw text, draw image, draw shadow, and apply filter.
- [x] **PAINT-02**: Maintainer can add a future backend without changing MESH widget traversal, style resolution, layout, animation state, retained display-list ordering, damage selection, module boundaries, input handling, or presentation ownership.
- [x] **PAINT-03**: Existing widget-tree and retained-display-list render paths both route paint execution through the same painter command/backend boundary.

### XML/CSS/Token Style Profile

- [x] **STYLE-01**: Maintainer has a documented bounded style profile for MESH's XML/.mesh, CSS-like syntax, and theme tokens, covering supported visual properties and explicitly excluding arbitrary browser CSS.
- [x] **STYLE-02**: Existing token references and shipped module styles continue to resolve through the current theme/token pipeline while painter-relevant values lower into backend-neutral render data.
- [x] **STYLE-03**: Unsupported or ambiguous web-style properties produce diagnostics instead of being silently accepted with missing visual behavior.

### Elements And Display-List Lowering

- [x] **ELEM-01**: Supported MESH elements and controls lower into backend-neutral painter commands without losing retained node identity, style state, layout data, or accessibility metadata.
- [x] **ELEM-02**: Direct widget-tree painting and retained display-list replay produce equivalent painter command classes for the same node/style inputs.

### Skia Backend

- [ ] **SKIA-01**: Skia owns rasterization, antialiasing, paths, rounded rects, strokes, clipping, and blend modes for core shape primitives.
- [ ] **SKIA-04**: Remaining MESH-owned software fallback code for painter primitives is removed or isolated behind non-authoritative compatibility tests.

### Text Boundary

- [ ] **TEXT-01**: The painter engine preserves current text measurement, drawing, and theme-owned selection behavior while allowing text-adjacent rectangles and future text primitives to route through the painter API.

### Effects, Layers, Images, And Gradients

- [ ] **EFFECT-01**: Box shadows, blur, backdrop-filter blur, opacity, and blend behavior lower into explicit painter layer/effect commands.
- [ ] **EFFECT-02**: Gradients and images are represented in backend-neutral painter data with source/lifetime rules compatible with current module assets and style/token data.
- [ ] **EFFECT-03**: Unsupported effect combinations, excessive blur, missing assets, or backend capability gaps emit explicit diagnostics.
- [ ] **LAYER-01**: Node styles that require opacity, clipping, filters, backdrop filters, shadows, or blend behavior lower into explicit painter layer/effect commands.

### Animation And Transitions

- [ ] **ANIM-01**: Existing CSS/token keyframes and transitions remain compatible with the painter engine.
- [ ] **ANIM-02**: Paint-only animation updates for color, opacity, transform, shadow, filter, border, and related visual properties avoid full layout when geometry does not change.
- [ ] **ANIM-03**: Animated visual bounds and damage include effect overflow and transformed pixels.

### Damage, Stacking, And Visual Bounds

- [ ] **LAYER-02**: Damage and visual bounds include pixels affected by shadows, filters, layer effects, transforms, images, gradients, and clipped descendants.
- [ ] **LAYER-03**: Stacking order and z-index behavior remain owned by MESH while the backend receives already ordered painter commands.
- [ ] **DAMAGE-01**: Partial repaint and full-surface fallback behavior remain deterministic for layered/effect-heavy surfaces.
- [ ] **DAMAGE-02**: Profiling distinguishes layout, paint, effect-overflow, command filtering, and fallback-promotion behavior.

### Backend Extensibility And Observability

- [x] **BACKEND-01**: Painter backend traits are documented with backend obligations, unsupported-feature behavior, and parity expectations.
- [x] **BACKEND-02**: A future Vello backend can be sketched against the painter API without introducing Skia-specific concepts into display-list data.
- [ ] **BACKEND-03**: Backend selection remains reversible and observable through renderer diagnostics or debug/profiling payloads.
- [ ] **OBS-01**: Painter diagnostics include backend id, unsupported feature id, concise message, and source node/style context where available without polluting retained identity.
- [ ] **OBS-02**: Backend capabilities and rollback behavior are documented and covered by tests before Skia parity is accepted.

### Verification And Shipped Proof

- [ ] **VERIFY-01**: Automated tests prove the supported painter-engine subset: style profile, element lowering, core shapes, rounded corners, strokes, paths, shadows, blur/filter effects, layer clipping, images, gradients, animations, damage, and retained display-list replay.
- [ ] **VERIFY-02**: Shipped navigation/audio surfaces render through the painter engine without regressions in interaction, selection, profiling, diagnostics, or damage behavior.
- [ ] **VERIFY-03**: Renderer ownership and migration docs describe the bounded WebEngine/Qt-style split: MESH render engine, XML/CSS/token style profile, animation state, Skia painter backend, presentation, and future Vello backend.

## v2 Requirements

### Future Backends

- **VELLO-01**: Maintainer can enable a production Vello backend that implements the painter API with parity tests against Skia.
- **VELLO-02**: Runtime can choose between Skia and Vello backends per build or configuration without changing author-facing `.mesh` behavior.

### Future Text

- **TEXT-02**: Skia, Parley, or another painter/text backend can own more text drawing primitives when it preserves MESH selection, shaping, font, accessibility, and theme behavior.

### Future Style Profile Expansion

- **STYLE-04**: Maintainer can expand the bounded CSS profile only through documented parser/resolver diagnostics, compatibility fixtures, and shipped-surface proof.

## Out of Scope

| Feature | Reason |
|---------|--------|
| Replacing the MESH render engine wholesale with Skia, Vello, Blitz, WebEngine, or Qt | Skia/Vello are painter backends; MESH must retain tree, style, layout, animation, damage, module, input, and presentation ownership. |
| Full browser/Web platform compatibility | MESH remains a shell UI framework with bounded XML/.mesh and CSS-like semantics, not a browser engine. |
| Arbitrary HTML parsing, DOM APIs, network/resource loading, browser layout modes, or web compatibility quirks | These would bloat the engine and undermine deterministic shell UI goals. |
| Full Vello backend implementation | This milestone defines and proves the bounded painter engine with Skia first; Vello production parity is later. |
| GPU compositor replacement | Presentation and compositor integration remain owned by `mesh-core-presentation`; this milestone targets the painter boundary. |
| Broad animation-system redesign beyond supported visual-property integration | Phase 56 preserves and routes current animation behavior; new motion semantics are future profile work. |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| PAINT-01 | Phase 51 | Complete |
| PAINT-02 | Phase 51 | Complete |
| PAINT-03 | Phase 53 | Complete |
| STYLE-01 | Phase 52 | Complete |
| STYLE-02 | Phase 52 | Complete |
| STYLE-03 | Phase 52 | Complete |
| ELEM-01 | Phase 53 | Complete |
| ELEM-02 | Phase 53 | Complete |
| SKIA-01 | Phase 54 | Pending |
| SKIA-04 | Phase 54 | Pending |
| TEXT-01 | Phase 54 | Pending |
| EFFECT-01 | Phase 55 | Pending |
| EFFECT-02 | Phase 55 | Pending |
| EFFECT-03 | Phase 55 | Pending |
| LAYER-01 | Phase 55 | Pending |
| ANIM-01 | Phase 56 | Pending |
| ANIM-02 | Phase 56 | Pending |
| ANIM-03 | Phase 56 | Pending |
| LAYER-02 | Phase 57 | Pending |
| LAYER-03 | Phase 57 | Pending |
| DAMAGE-01 | Phase 57 | Pending |
| DAMAGE-02 | Phase 57 | Pending |
| BACKEND-01 | Phase 51 | Complete |
| BACKEND-02 | Phase 51 | Complete |
| BACKEND-03 | Phase 58 | Pending |
| OBS-01 | Phase 58 | Pending |
| OBS-02 | Phase 58 | Pending |
| VERIFY-01 | Phase 59 | Pending |
| VERIFY-02 | Phase 59 | Pending |
| VERIFY-03 | Phase 59 | Pending |

**Coverage:**
- v1 requirements: 30 total
- Mapped to phases: 30
- Unmapped: 0

---
*Requirements expanded: 2026-05-22 for the painter engine roadmap*
