# Pitfalls Research

**Domain:** CPU-side retained renderer optimization for shell UI
**Researched:** 2026-05-10
**Confidence:** HIGH

## Critical Pitfalls

### Pitfall 1: Whole-tree retention in name only

**What goes wrong:**  
The renderer keeps “retained” structures, but every local change still recollects display entries and paint commands across the tree.

**Why it happens:**  
Dirty summaries stop at the render-object layer while retained paint-command ownership stays surface-wide.

**How to avoid:**  
Add subtree-level retained command ownership keyed by stable node IDs and only rebuild affected branches.

**Warning signs:**  
Small hover or slider updates still show large display-list rebuild time or command churn.

**Phase to address:**  
Phase 28

---

### Pitfall 2: Partial damage that still scans the full command list

**What goes wrong:**  
Damage rects are correct, but painting still loops over every command and clips late, so CPU cost stays high.

**Why it happens:**  
Damage is tracked as geometry but not indexed back to retained commands.

**How to avoid:**  
Build a node-range or spatial lookup so partial paints visit only intersecting commands plus required overlays.

**Warning signs:**  
Tiny damage areas still produce paint times that scale with total node count instead of changed-region size.

**Phase to address:**  
Phase 29

---

### Pitfall 3: Raster-cache gaps in icon and image paths

**What goes wrong:**  
SVG icons get reparsed/rerasterized and bitmap images get resized repeatedly during steady-state paints.

**Why it happens:**  
Source assets may be cached, but rasterized outputs are not retained by visual inputs such as size and tint.

**How to avoid:**  
Cache rasterized SVG/icon/bitmap variants and retain opaque/translucent metadata with the cache entry.

**Warning signs:**  
Icon-heavy surfaces stay expensive even when layout/style work is low; cache-hit metrics are missing or near zero.

**Phase to address:**  
Phase 30

---

### Pitfall 4: Treating clipping as a free optimization

**What goes wrong:**  
Developers add more per-item clipping to constrain paint, but overall CPU work and batching opportunities get worse.

**Why it happens:**  
Clipping looks like an obvious way to reduce drawing, but Qt explicitly warns it adds render-state complexity and can break batching.

**How to avoid:**  
Prefer viewport-aware omission, layout/elision, and coarser clip boundaries instead of defaulting to per-item clip.

**Warning signs:**  
Small delegates or labels start carrying clip state and paint metrics regress even when visible output is unchanged.

**Phase to address:**  
Phase 27

---

### Pitfall 5: Winning the benchmark and losing the product

**What goes wrong:**  
Numbers improve on isolated counters, but real shell surfaces still feel laggy to the user.

**Why it happens:**  
Optimization gets anchored to synthetic or internal metrics without visible proof on shipped surfaces.

**How to avoid:**  
Keep canonical benchmark results tied to navigation-bar, audio-popover, and other shipped proof surfaces, with explicit visible-smoothness checks.

**Warning signs:**  
Stage timings move, but hover/open-close/slider interactions do not feel noticeably better.

**Phase to address:**  
Phase 31

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Full-surface repaint fallback for every ambiguous change | Simple correctness | Masks where retained work is still too broad | Only as a temporary fallback with explicit metrics |
| Caching source files but not raster outputs | Easy first cache | Repeats parse/resize cost on every paint | Only for rarely used assets |
| Adding a new benchmark harness instead of using the existing one | Freedom to prototype | Splits proof paths and weakens regression history | Never for this milestone |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Full command-list scans under partial damage | Paint time tracks total command count | Add damage-indexed command filtering | As surfaces become moderately complex |
| Repeated SVG parsing and bitmap resizing | Icon-heavy paints remain expensive | Add retained raster caches | Immediately noticeable on recurring paints |
| Per-item clip proliferation | More state churn and less merging | Push clip decisions outward and prune earlier | On delegate-heavy or text-heavy surfaces |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| Benchmark-only improvements | UI still feels laggy despite “faster” reports | Require visible-smoothness proof on shipped surfaces |
| Aggressive repaint pruning with stale visuals | Users see incorrect or delayed updates | Keep full-repaint fallbacks and verify correctness per phase |
| Cache-heavy optimization with no eviction policy | Smoothness regresses over time or memory balloons | Add bounded caches and profiler-visible hit/miss metrics |

## "Looks Done But Isn't" Checklist

- [ ] **Dirty-subtree retention:** Verify local updates do not still recollect the entire surface command set.
- [ ] **Partial damage:** Verify small damage regions do not still scan every command before clipping.
- [ ] **Raster caches:** Verify unchanged icons/images/text hit caches on repeated paints.
- [ ] **Smoothness proof:** Verify shipped surfaces feel smoother, not just benchmark counters.

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Whole-tree retention in name only | Phase 28 | Display-list rebuild work stays local on scoped updates |
| Partial damage still scans everything | Phase 29 | Paint traversal cost scales with changed region, not total surface size |
| Raster-cache gaps | Phase 30 | Repeated paints show sustained cache hits and lower raster cost |
| Clipping as a free optimization | Phase 27 | Clip-heavy surfaces do not regress and unnecessary per-item clips are avoided |
| Benchmark win but no product win | Phase 31 | Canonical proof surfaces look visibly smoother to the user |

## Sources

- Qt Quick Scene Graph Default Renderer
- Qt Quick Performance Considerations
- Local renderer and shell orchestration code in `mesh-core-render` and `mesh-core-shell`

---
*Pitfalls research for: CPU-side retained renderer optimization*
*Researched: 2026-05-10*
