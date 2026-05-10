# Feature Research

**Domain:** CPU-side retained renderer optimization for shell UI
**Researched:** 2026-05-10
**Confidence:** HIGH

## Feature Landscape

### Table Stakes (Users Expect These)

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Dirty-subtree paint retention | Retained rendering should not recollect the whole surface on local changes | HIGH | Local code still rebuilds paint-command structures across the tree when retained generation changes. |
| Damage-scoped paint execution | Partial-damage rendering should avoid scanning unrelated commands | HIGH | Current display-list paint still walks all commands and clips late. |
| Viewport and visibility pruning | Offscreen or hidden content should not keep consuming CPU | MEDIUM | Qt guidance favors explicit visibility and viewport-aware omission over expensive general-purpose occlusion logic. |
| Retained text/icon/image raster caches | Repeated renders should not reparse or reraster unchanged assets | MEDIUM | Text has a start; SVG/icon/image paths still have obvious cache gaps. |
| Benchmark-backed smoothness proof | Optimization must improve real surfaces, not just internal counters | MEDIUM | Existing canonical scenarios already provide the acceptance harness. |

### Differentiators (Competitive Advantage)

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Heuristic repaint policy switching | Lets MESH choose minimal, bounding-rect, or full-surface repaint based on measured CPU cost | MEDIUM | Directly informed by Qt’s “damage is a policy choice” lesson. |
| Retained transform/scroll roots for cheap repeated motion | Makes scroll and animation-heavy surfaces smoother without GPU work | HIGH | Mirrors Qt batch-root behavior conceptually, but on the CPU command pipeline. |
| Debug counters for cull skips and raster-cache misses | Makes future performance work explainable instead of guesswork | LOW | Fits existing debug inspector and profiling model. |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| GPU backend now | Feels like the obvious route to “faster rendering” | Expands scope and can mask wasteful CPU-side invalidation and paint logic | Finish CPU retained pipeline first |
| Per-item clipping everywhere | Looks like an easy way to limit drawing | Qt warns clipping breaks batching/state locality and adds overhead | Use viewport-aware omission, elision, and coarser clip boundaries |
| Trace persistence in this milestone | Sounds helpful for deep performance analysis | Adds tooling scope before the base renderer feels smooth | Keep live inspector metrics and canonical benchmark proof only |

## Feature Dependencies

```text
Profiling attribution
    └──requires──> Canonical benchmark proof

Dirty-subtree retention
    └──requires──> Profiling attribution

Damage-scoped paint execution
    └──requires──> Dirty-subtree retention
    └──requires──> Viewport/visibility pruning

Raster cache hardening
    └──enhances──> Damage-scoped paint execution

Smoothness proof
    └──requires──> All prior optimization phases

GPU backend prototype
    └──conflicts──> CPU-only milestone scope
```

### Dependency Notes

- **Dirty-subtree retention requires profiling attribution:** The renderer needs baseline measurements so it can target the biggest tree-walk and rebuild costs first.
- **Damage-scoped paint execution requires retained command ownership:** Without stable command ranges or subtree ownership, filtered execution risks incorrect ordering or clip behavior.
- **Raster cache hardening enhances all paint phases:** Once command traversal narrows, raster-cache misses become easier to spot and more worthwhile to eliminate.
- **GPU backend prototype conflicts with current scope:** The user explicitly wants CPU rendering logic improvements first.

## MVP Definition

### Launch With (v1.5)

- [ ] CPU hotspot attribution on canonical benchmark scenarios
- [ ] Viewport/visibility pruning on shipped proof surfaces
- [ ] Dirty-subtree retained paint-command updates
- [ ] Damage-scoped command execution
- [ ] Retained raster caches for unchanged text/icon/image work
- [ ] Visible smoothness improvement on shipped shell surfaces

### Add After Validation (v1.x)

- [ ] Visual overlays for culled regions, overdraw, and command-filter hits — add once the CPU pipeline is stable
- [ ] More advanced spatial indexing if simple node-range indexing is not enough

### Future Consideration (v2+)

- [ ] GPU backend implementation — only after the CPU path is convincingly smooth
- [ ] Parallel paint/layout — only after retained ownership boundaries are mature

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| CPU hotspot attribution | HIGH | MEDIUM | P1 |
| Viewport/visibility pruning | HIGH | MEDIUM | P1 |
| Dirty-subtree retained paint updates | HIGH | HIGH | P1 |
| Damage-scoped paint execution | HIGH | HIGH | P1 |
| Raster cache hardening | HIGH | MEDIUM | P1 |
| Visual debug overlays | MEDIUM | MEDIUM | P2 |
| GPU backend groundwork beyond guardrails | LOW for this milestone | HIGH | P3 |

## Sources

- Qt Quick Scene Graph Default Renderer
- Qt Quick Performance Considerations
- Existing MESH profiling/benchmark research from `.planning/research/v1.3-performance-instrumentation-and-responsiveness.md`
- Current renderer code in `mesh-core-render` and `mesh-core-shell`

---
*Feature research for: CPU-side retained renderer optimization*
*Researched: 2026-05-10*
