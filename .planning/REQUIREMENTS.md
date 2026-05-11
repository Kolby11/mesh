# Requirements: MESH v1.5 CPU Rendering Performance Improvement

**Defined:** 2026-05-10
**Core Value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.

## v1.5 Requirements

### CPU Profiling and Proof

- [x] **PERF-01**: Developers can inspect CPU render cost for tree build, style restyle, layout, render-object sync, retained display-list rebuild, paint traversal, text shaping, and icon/image raster work on each canonical benchmark scenario.
- [x] **PERF-02**: Every v1.5 optimization phase records before/after benchmark evidence on shipped surfaces using the existing canonical benchmark scenarios.
- [ ] **PERF-03**: Optimization decisions are accepted only when they improve visible smoothness on shipped shell surfaces, not merely aggregate internal counters.

### Visibility and Culling

- [ ] **CULL-01**: Fully offscreen descendants inside clipped or scrollable regions are omitted from retained paint-command generation or execution on the CPU path.
- [ ] **CULL-02**: Nodes hidden by explicit visibility, surface state, or fully ineffective opacity stop generating unnecessary CPU paint work until they become visible again.
- [ ] **CULL-03**: The renderer can choose between minimal-damage, bounding-rect, and full-surface repaint policies based on measured CPU cost instead of assuming the smallest region is always cheapest.
- [ ] **CULL-04**: Clipping and viewport rules avoid per-item CPU overhead on small primitives when a cheaper elision or coarser-boundary alternative exists.

### Retained Paint Pipeline

- [ ] **PIPE-01**: Local retained-tree changes rebuild only the affected render-object and paint-command subtrees instead of recollecting the full surface command set.
- [ ] **PIPE-02**: Transforms, scroll offsets, and reorder-only changes can update retained paint data without invalidating unrelated descendant geometry or style state.
- [ ] **PIPE-03**: Partial-damage paints visit only commands that intersect the damaged region or explicitly depend on global overlays such as tooltips or scrollbars.
- [ ] **PIPE-04**: Command ordering, clipping, and visual correctness remain stable when filtered execution skips unrelated commands.

### Raster and Resource Caching

- [ ] **CACHE-01**: SVG icons, bitmap icons, and resized image variants are cached in a retained raster form so repeated paints avoid reparsing, decoding, and rescaling unchanged assets.
- [ ] **CACHE-02**: Text and glyph caches continue to reuse unchanged layout and raster data across hover, animation, scroll, and state-driven updates.
- [ ] **CACHE-03**: Opaque vs translucent resource metadata is retained so the painter can avoid unnecessary blending and redundant background draws when content is fully opaque.

### Smoothness Guardrails

- [ ] **SMTH-01**: Canonical hover, surface open/close, pointer update, keyboard traversal, and backend update scenarios look visibly smoother on shipped surfaces after the milestone.
- [ ] **SMTH-02**: Normal shell visuals and interaction correctness remain unchanged apart from smoother rendering behavior.
- [ ] **SMTH-03**: GPU backend and parallel paint/layout remain out of scope until the CPU retained pipeline is demonstrably smooth on real surfaces.

## Future Requirements

### Deeper Diagnostics

- **VIS-01**: Add visual overlays for culled regions, filtered command hits, damage policy selection, and overdraw after the CPU pipeline is stable.
- **TRACE-01**: Persist retained-rendering and raster-cache metrics for offline comparison if live inspector proof stops being sufficient.

### Future Rendering Work

- **GPU-01**: Render retained display data through a GPU backend once the CPU path is no longer the dominant bottleneck.
- **PAR-01**: Move eligible paint/layout work to worker threads once retained ownership boundaries are explicit and mechanically safe.

## Out of Scope

| Feature | Reason |
|---------|--------|
| GPU backend implementation | The user explicitly wants CPU rendering logic improved first, and current CPU waste would hide the real GPU value. |
| Parallel paint/layout | Retained command ownership and repaint correctness should be proven on one CPU path before parallelism is introduced. |
| Broad shell UI redesign | Existing shipped surfaces remain the proof targets for visible smoothness. |
| A new benchmark harness | The canonical benchmark scenarios already exist and should remain the sole acceptance path. |
| Trace persistence or telemetry export | Live inspector metrics and benchmark proof are sufficient for this milestone. |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| PERF-01 | Phase 26 | Complete |
| PERF-02 | Phase 26 | Complete |
| CULL-01 | Phase 27 | Pending |
| CULL-02 | Phase 27 | Pending |
| CULL-04 | Phase 27 | Pending |
| PIPE-01 | Phase 28 | Pending |
| PIPE-02 | Phase 28 | Pending |
| PIPE-03 | Phase 29 | Pending |
| PIPE-04 | Phase 29 | Pending |
| CULL-03 | Phase 29 | Pending |
| CACHE-01 | Phase 30 | Pending |
| CACHE-02 | Phase 30 | Pending |
| CACHE-03 | Phase 30 | Pending |
| PERF-03 | Phase 31 | Pending |
| SMTH-01 | Phase 31 | Pending |
| SMTH-02 | Phase 31 | Pending |
| SMTH-03 | Phase 31 | Pending |

**Coverage:**
- v1.5 requirements: 17 total
- Mapped to phases: 17
- Unmapped: 0

---
*Requirements defined: 2026-05-10*
*Last updated: 2026-05-11 after Phase 26 verification passed*
