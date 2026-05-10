# Stack Research

**Domain:** CPU-side retained renderer optimization for a Wayland shell UI framework
**Researched:** 2026-05-10
**Confidence:** HIGH

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `mesh-core-render` | workspace | Software pixel painter, display-list retention, text/icon/image raster work | Existing renderer crate already owns the CPU path; the milestone should improve this path rather than replace it. |
| `mesh-core-elements` + retained widget tree | workspace | Retained style/layout/widget model and dirty summaries | The existing retained tree is the correct source of truth for dirty-subtree updates and culling decisions. |
| Qt Quick scene-graph guidance | Qt 6.11 docs | Reference architecture for retained nodes, batch roots, clipping, and visibility rules | Official Qt docs describe the retained-rendering patterns most relevant to MESH without forcing a browser-engine model. |
| Existing debug benchmark harness | workspace | Canonical hover/open-close/pointer/keyboard/backend-update proof flows | The user wants visibly smoother rendering on real surfaces, so improvements must stay tied to shipped benchmark scenarios. |

### Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `cosmic_text` | workspace | Text layout and shaping reuse | Keep using it for text layout caching; optimize invalidation and reuse rather than replacing it. |
| `swash` | workspace | Glyph rasterization for icon/font paths | Continue using it, but ensure cache hits survive common hover/animation/scroll updates. |
| `resvg` / `usvg` / `tiny-skia` | workspace | SVG parsing and rasterization | Keep for file-backed SVG icons, but add retained raster caches so unchanged assets are not reparsed every paint. |
| Simple in-repo spatial index / command ranges | new local structure | Damage-to-command lookup | Prefer a local index keyed by retained node ranges before introducing a new external dependency. |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| Debug inspector profiling snapshots | Surface current stage timings and invalidation counts | Extend to attribute display-list rebuild, command traversal, and raster-cache misses. |
| Canonical benchmark scenarios | Repeatable proof on shipped surfaces | Keep hover, open/close, pointer update, keyboard traversal, and backend update as acceptance paths. |
| Visual debug counters | Explain why rendering stayed expensive | Add counters for cull skips, filtered commands, cache hits/misses, and repaint-policy switches. |

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| Improve the existing software renderer | Jump directly to a GPU backend | Only after the CPU path is smooth enough to expose the true GPU-specific bottlenecks. |
| Dirty-subtree retention inside existing display-list pipeline | Whole-tree rebuilds with faster raw loops | Only if profiling proves rebuild bookkeeping is more expensive than recollection, which is unlikely on larger surfaces. |
| Retained raster caches for SVG/icons/images | Re-decode or re-raster assets on demand | Only acceptable for truly one-shot assets that never repeat. |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| GPU-backend work in `v1.5` | Hides CPU pipeline waste and broadens scope too early | Tighten retained CPU rendering first |
| Per-item clipping as a default optimization | Qt docs explicitly warn that clipping can break batching and add state cost | Prefer viewport-aware elision, layout constraints, and opaque cover strategies |
| Benchmark-only tuning with no shipped-surface validation | The user reports real lag everywhere, not just synthetic regressions | Pair benchmark evidence with visible-smoothness checks on real shell surfaces |

## Stack Patterns by Variant

**If the bottleneck is display-list churn:**
- Keep the current crates
- Add subtree retention and command-range indexing inside `mesh-core-render`

**If the bottleneck is raster work:**
- Keep `cosmic_text`, `swash`, and `resvg`
- Add retained caches and opaque metadata around them instead of changing render libraries

## Sources

- Qt Quick Scene Graph Default Renderer — batching, transform batch roots, clipping, texture atlas, and render-timing guidance
- Qt Quick Performance Considerations — clipping, overdraw, opacity, and text performance guidance
- Local code:
  - `crates/core/shell/src/shell/component/rendering.rs`
  - `crates/core/frontend/render/src/render_object.rs`
  - `crates/core/frontend/render/src/display_list.rs`
  - `crates/core/frontend/render/src/surface/painter/tree.rs`
  - `crates/core/frontend/render/src/surface/icon.rs`

---
*Stack research for: CPU-side retained renderer optimization*
*Researched: 2026-05-10*
