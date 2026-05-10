# Project Research Summary

**Project:** MESH
**Domain:** CPU-side retained renderer optimization for a Wayland shell framework
**Researched:** 2026-05-10
**Confidence:** HIGH

## Executive Summary

Qt Quick’s renderer confirms that MESH is on the right architectural track: retain UI-facing state separately from paint-facing render data, keep work scoped to what changed, and treat clipping, visibility, and batching as policy decisions instead of blindly recomputing everything. The most relevant Qt lessons for MESH are transform-root retention, explicit visibility pruning, careful use of clipping, and aggressive reuse of retained geometry or raster data across frames.

Local code inspection shows that MESH still pays too much whole-surface CPU cost after the `v1.4` retained-rendering milestone. `RetainedDisplayList::update_inner()` recollects display entries and paint commands across the tree whenever retained generation changes, and `render_display_list_for_module()` still scans every command even when only a small damage rect changed. The icon path also caches source images but still resizes bitmaps and reparses SVGs on repeated paints. Those are likely why “everything still feels laggy” even though the retained foundations exist.

The recommended approach is a CPU-only milestone that first improves hotspot attribution, then tightens visibility/culling, retained paint-command ownership, damage-indexed execution, and raster caching. GPU backend work should remain out of scope until the software renderer is visibly smoother on shipped surfaces.

## Key Findings

### Recommended Stack

MESH should keep the current Rust software-renderer stack and evolve it rather than replacing it. The retained widget tree, render-object layer, text/glyph stack, and debug benchmark harness are already the correct foundations; what is missing is narrower command ownership, better damage filtering, and broader raster reuse.

**Core technologies:**
- `mesh-core-render`: software renderer and retained paint pipeline — the main optimization target
- `mesh-core-elements` retained tree: stable identity and dirty summaries — the main source of local-change scoping
- Qt Quick renderer guidance: retained scene-graph patterns — the primary architectural reference

### Expected Features

**Must have (table stakes):**
- Dirty-subtree retained paint-command updates
- Damage-scoped paint execution
- Viewport and visibility pruning
- Retained raster caches for icons/images/text
- Benchmark-backed visible-smoothness proof

**Should have (competitive):**
- Repaint-policy switching based on measured CPU cost
- Transform/scroll-root retention that keeps repeated motion cheap
- Debug counters for cull skips and raster-cache misses

**Defer (v2+):**
- GPU backend implementation
- Parallel paint/layout
- Trace persistence/export

### Architecture Approach

The architecture should stay layered: runtime invalidation -> retained widget tree -> render-object diff -> retained paint-command cache -> damage/visibility/raster execution -> pixel buffer -> present. The milestone should make retained command ownership more local and make damage mapping cheap enough that partial repaints no longer behave like full-surface traversals.

**Major components:**
1. CPU render profiling and proof attribution — explain where time is spent now
2. Visibility/culling planner — omit offscreen or hidden work early
3. Retained paint-command cache plus damage index — keep local changes local
4. Raster cache hardening — stop reparsing/resizing unchanged visual assets

### Critical Pitfalls

1. **Whole-tree retention in name only** — avoid by moving retained command ownership to dirty subtrees
2. **Partial damage that still scans everything** — avoid by indexing commands back from damage regions
3. **Raster-cache gaps in SVG/icon/image paths** — avoid by caching raster outputs, not just sources
4. **Treating clipping as a free optimization** — avoid by preferring viewport-aware omission and coarser clip boundaries
5. **Winning benchmarks without improving felt smoothness** — avoid by validating on shipped surfaces, not only counters

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 26: CPU Render Profiling and Baseline Proof
**Rationale:** The next implementation phases should attack measured hotspots, not assumptions.  
**Delivers:** Finer-grained CPU render attribution on canonical scenarios and proof surfaces.  
**Addresses:** Profiling and proof requirements.  
**Avoids:** Benchmark-only guesswork.

### Phase 27: Viewport Culling and Visibility Elision
**Rationale:** Qt emphasizes explicit visibility control and careful clipping. MESH should reduce work before rebuilding or painting commands.  
**Delivers:** Clip/viewport-aware pruning and hidden-surface elision.  
**Uses:** Existing retained tree and dirty summaries.  
**Implements:** Visibility/culling component.

### Phase 28: Incremental Paint Command Retention
**Rationale:** Local code still recollects full paint-command structures on retained-generation changes. This is likely a core source of lag.  
**Delivers:** Dirty-subtree command retention and narrower rebuild scope.  
**Addresses:** Retained pipeline requirements.  
**Avoids:** “Retained” work that still walks the entire surface.

### Phase 29: Damage-Indexed Paint Execution and Repaint Policy
**Rationale:** Once command ownership is local, the next step is to stop scanning unrelated commands under partial damage.  
**Delivers:** Damage-to-command lookup plus cost-aware repaint policy switching.  
**Implements:** Execution filtering and repaint heuristics.

### Phase 30: Raster Cache Hardening for Icons, Images, and Text
**Rationale:** After tree traversal and paint traversal narrow, repeated raster work becomes easier to see and more valuable to fix.  
**Delivers:** Retained SVG/icon/bitmap caches plus tighter text/glyph reuse metrics.  
**Uses:** Existing `cosmic_text`, `swash`, and `resvg` stack.

### Phase 31: Smoothness Proof and CPU Render Tuning
**Rationale:** The milestone succeeds only if real shell interactions look smoother.  
**Delivers:** Heuristic tuning, final benchmark runs, and visible-smoothness validation on shipped surfaces.  
**Avoids:** Shipping a technically “optimized” pipeline that still feels laggy.

### Phase Ordering Rationale

- Profiling comes first so later phases attack the biggest remaining CPU costs.
- Culling precedes filtered paint execution because early omission reduces the size of retained command work.
- Dirty-subtree retention must land before damage-indexed paint execution, otherwise filtered execution still lacks stable ownership boundaries.
- Raster cache hardening comes after command filtering so cache misses are easier to observe and attribute.
- Smoothness proof closes the milestone because the user’s actual complaint is visible lag, not just inefficient code.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 29:** Damage index design and repaint-policy heuristics may need focused tradeoff analysis during planning.
- **Phase 30:** Cache-key design and eviction policy should be validated against real surface behavior.

Phases with standard patterns (skip research-phase):
- **Phase 26:** Builds directly on existing profiling and benchmark infrastructure.
- **Phase 27:** Largely follows the Qt visibility and clipping guidance already researched.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Based on local code and official Qt docs |
| Features | HIGH | User goal is explicit and local bottlenecks are visible in code |
| Architecture | HIGH | Retained-layer boundaries already exist; the missing work is narrower ownership and execution |
| Pitfalls | HIGH | Backed by official Qt docs plus concrete local code paths |

**Overall confidence:** HIGH

### Gaps to Address

- Exact repaint-policy heuristic thresholds should be validated with real benchmark data during planning/execution.
- It is still possible that one or two benchmark scenarios are dominated by tree build or script work rather than paint; Phase 26 should confirm that before later phases are executed.

## Sources

### Primary (HIGH confidence)
- Qt Quick Scene Graph Default Renderer — https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph-renderer.html
- Qt Quick Performance Considerations — https://doc.qt.io/qt-6/qtquick-performance.html
- Qt Quick Scene Graph — https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html
- Local renderer code in `mesh-core-render` and orchestration code in `mesh-core-shell`

### Secondary (MEDIUM confidence)
- Existing MESH research and benchmark notes in `.planning/research/v1.3-performance-instrumentation-and-responsiveness.md`
- Existing MESH retained-rendering research in `.planning/research/v1.4-major-performance-fixes-qt-retained-rendering.md`

---
*Research completed: 2026-05-10*
*Ready for roadmap: yes*
