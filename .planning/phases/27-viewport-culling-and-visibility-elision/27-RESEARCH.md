# Phase 27: Viewport Culling and Visibility Elision - Research

**Researched:** 2026-05-11
**Domain:** MESH retained software rendering, explicit viewport pruning, and debug-proofed paint omission
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Follow Qt Quick style conservative visibility semantics for this phase.
- Pruning authority comes from explicit clip and scroll viewport boundaries, not from a global smart visibility heuristic.
- Allow cheap root-surface omission only when a subtree is trivially and provably outside the root surface bounds.
- Keep Phase 27 scoped to paint-time omission first; do not pull retained display-list ownership or render-object synchronization pruning forward from later phases.
- Use whole-subtree omission inside explicit clip or scroll viewports, plus localized rough pre-clipping for viewport-aware content.
- Do not introduce broad command-level smart filtering across partially visible subtrees in this phase.
- Reuse the existing profiling and debug pipeline, and expose aggregate counters only.

### the agent's Discretion
- Choose the exact code seam for viewport-aware pruning as long as it stays inside the current retained render-object, retained display-list, and filtered paint-node architecture.
- Choose the exact aggregate counter names and where they live in the existing debug payload.

### Deferred Ideas (OUT OF SCOPE)
- Global CPU-side occlusion or viewport-culling pass.
- Generic opacity-based hidden heuristics.
- Broad command-level filtering across partially visible subtrees.
- A second diagnostics or trace system.
</user_constraints>

<research_summary>
## Summary

Phase 27 should attach to the existing retained display-list and paint-execution boundary, not invent a new renderer layer. The current code already has three useful seams:

1. `display_list.rs` builds retained paint commands while propagating clip rectangles and scroll offsets.
2. `surface/painter/tree.rs` already supports clipped rendering and filtered paint-node traversal.
3. `shell_component.rs` already derives retained paint/debug snapshots from `DisplayListMetrics`.

The key architectural wrinkle is that MESH currently conflates `visibility:hidden` with plain `opacity = 0` in `style/resolve.rs`. That conflicts with the Qt-like rule locked in Phase 27, because the user explicitly rejected a generic “opacity zero means hidden” heuristic. Planning should therefore separate **explicit hidden semantics** from **generic opacity semantics** before pruning logic is added.

**Primary recommendation:** plan Phase 27 as one execution plan with three tightly-sequenced tasks:
- add explicit hidden/viewport pruning signals and aggregate counters,
- apply viewport-aware subtree omission at the retained display-list paint-command layer,
- wire aggregate proof into the existing debug payload with focused render and shell tests.
</research_summary>

<existing_implementation_facts>
## Existing Implementation Facts

### Retained Render and Paint Seams

- `crates/core/frontend/render/src/render_object.rs` already tracks retained dirty summaries for `clip` and `opacity`, but does not decide visibility pruning.
- `crates/core/frontend/render/src/display_list.rs` already:
  - skips `Display::None`,
  - propagates scroll offsets,
  - narrows child clip rectangles when `overflow_x` or `overflow_y` clips contents,
  - computes aggregate `DisplayListMetrics`,
  - stores compact `DisplayPaintCommand` payloads for the retained paint path.
- `crates/core/frontend/render/src/surface/painter/tree.rs` already:
  - skips filtered-out `paint_nodes`,
  - intersects command clips against a paint clip,
  - performs node-level clipped recursion for tree painting.
- `crates/core/shell/src/shell/component/shell_component.rs` already:
  - updates the retained display list before paint,
  - selects effective paint damage,
  - paints through `paint_display_list_for_module_with_profiling_metrics`,
  - translates `DisplayListMetrics` into `mesh_core_debug::RetainedPaintSnapshot`.

### Current Hidden-State Behavior

- `crates/core/ui/interaction/src/lib.rs::node_is_hidden()` already treats three things as hidden for interaction logic:
  - `Display::None`,
  - zero-sized layout,
  - truthy `hidden` attribute.
- `crates/core/ui/elements/src/style/resolve.rs` currently resolves CSS `visibility:hidden|collapse` by setting `style.opacity = 0.0`.
- The render and display-list code currently treat `Display::None` as hidden, but they do not inspect the `hidden` attribute and they do not distinguish visibility-hidden from plain author-set opacity.

### Existing Proof and Debug Contract

- `crates/core/foundation/debug/src/lib.rs` defines `ProfilingInvalidationSnapshot` and `RetainedPaintSnapshot`, which already expose aggregate retained paint metrics.
- `crates/core/shell/src/shell/runtime/debug.rs` serializes `invalidation.paint` into `mesh.debug`.
- Phase 26 added retained profiling stages and baseline proof artifacts, and the shipped scenario baseline shows paint traversal is still the dominant cost.

### Existing Test Anchors

- `crates/core/frontend/render/src/display_list.rs` already has focused tests for clipping, batching, damage, and compact paint payload behavior.
- `crates/core/frontend/render/src/surface/painter/tests.rs` already has clipping tests such as `painter_clips_children_when_overflow_hidden`.
- `crates/core/shell/src/shell/tests.rs` already locks `ProfilingInvalidationSnapshot` JSON shape and retained paint metrics.
</existing_implementation_facts>

<recommended_technical_shape>
## Recommended Technical Shape

### 1. Separate Explicit Hidden Semantics from Generic Opacity

Introduce an explicit hidden/visibility signal in the computed-style or equivalent retained paint data path, rather than reusing plain `opacity = 0.0`.

Why:
- The user locked a Qt-like rule: explicit hidden semantics are valid pruning authority, but generic opacity is not.
- The current `visibility:hidden -> opacity = 0.0` mapping makes those two cases indistinguishable.

Recommended shape:
- Add a dedicated visibility/paint-hidden signal in `mesh-core-elements` style data.
- Keep plain `opacity = 0.0` paintable unless it is paired with explicit hidden semantics.
- Reuse existing `hidden` attribute semantics where appropriate instead of inventing a second attribute system.

### 2. Prune at Retained Paint-Command Collection Time

Phase 27 should attack paint traversal by omitting fully invisible subtrees while the retained display list builds its `DisplayPaintCommand`s.

Why this seam is the best fit:
- It is already clip- and scroll-aware.
- It affects the actual retained paint traversal hotspot without pulling Phase 28’s ownership/refactor work forward.
- It preserves the current `render_object -> display_list -> paint` architecture.

Recommended behavior:
- Root-surface omission: skip a subtree only when its bounds are trivially outside the root surface clip.
- Explicit viewport omission: when an ancestor establishes a clipping viewport via overflow, skip child subtree recursion when the child subtree bounds do not intersect that active viewport.
- Partial intersections remain paintable in Phase 27; no broad command filtering inside partially visible branches.

### 3. Keep Debug Proof Aggregate and Cheap

Expose only aggregate pruning counters through the existing invalidation/debug path.

Recommended categories:
- total omitted subtrees,
- total omitted nodes or commands,
- total localized pre-clipped descendants.

Avoid:
- per-node trace streams,
- human-readable reason categories,
- second benchmark/debug payloads.

### 4. Preserve Scrollbar and Overlay Correctness

Pruning must not silently drop visible scrollbar or overlay work. Scroll-root clipping should still allow visible scrollbar commands when the scroll container itself remains visible.
</recommended_technical_shape>

<candidate_plan_breakdown>
## Candidate Plan Breakdown

### Plan 27-01: Explicit Hidden Semantics, Viewport Omission, and Aggregate Proof

**Task 27-01-01:** Add an explicit hidden/visibility signal and aggregate pruning counters.
- Likely files:
  - `crates/core/ui/elements/src/style/types.rs`
  - `crates/core/ui/elements/src/style/resolve.rs`
  - `crates/core/frontend/render/src/display_list.rs`
  - `crates/core/foundation/debug/src/lib.rs`
  - `crates/core/shell/src/shell/component.rs`
  - `crates/core/shell/src/shell/runtime/debug.rs`

**Task 27-01-02:** Implement viewport-aware subtree omission in retained paint-command collection.
- Likely files:
  - `crates/core/frontend/render/src/display_list.rs`
  - `crates/core/frontend/render/src/surface/painter/tree.rs`
  - `crates/core/shell/src/shell/component/shell_component.rs`

**Task 27-01-03:** Add focused render and shell proof for clipping, hidden semantics, and debug payload counters.
- Likely files:
  - `crates/core/frontend/render/src/display_list.rs`
  - `crates/core/frontend/render/src/surface/painter/tests.rs`
  - `crates/core/shell/src/shell/tests.rs`
</candidate_plan_breakdown>

<common_pitfalls>
## Common Pitfalls

### Pitfall 1: Treating all opacity-zero nodes as hidden
This would violate the user’s Qt-like constraint and could silently drop legitimate translucent/animated content.

### Pitfall 2: Moving ownership work into Phase 27
If the implementation rewrites retained display-list ownership or broad command filtering, it will overlap Phase 28/29 and blur milestone boundaries.

### Pitfall 3: Pruning partial intersections too aggressively
Dropping partially visible branches would turn a conservative omission phase into a correctness-risky filtering phase.

### Pitfall 4: Adding verbose diagnostics instead of cheap counters
Per-node or reason-rich pruning traces would expand scope into a diagnostics feature rather than a focused optimization phase.
</common_pitfalls>

<validation_architecture>
## Validation Architecture

Phase 27 validation should stay render- and shell-focused, with fast aggregate checks after each task.

| Validation Target | Command | Purpose |
|-------------------|---------|---------|
| Formatting | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check` | Keep Rust formatting stable. |
| Retained display list pruning | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list` | Proves subtree omission, clipping, and aggregate pruning metrics at the render-crate seam. |
| Painter clipping regressions | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render painter_` | Protects visible clipping behavior and prevents over-pruning regressions in the software painter. |
| Shell profiling/debug snapshot | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling` | Protects `mesh.debug` invalidation payload shape and aggregate retained paint counters. |

Required proof properties:
- explicit hidden semantics are distinct from generic opacity semantics,
- fully out-of-viewport descendants under explicit scroll/clip roots are omitted,
- partially intersecting descendants still paint,
- aggregate pruning counters appear in the existing invalidation/debug payload only.
</validation_architecture>

<sources>
## Sources

### Primary (HIGH confidence)
- `.planning/phases/27-viewport-culling-and-visibility-elision/27-CONTEXT.md`
- `.planning/ROADMAP.md`
- `.planning/REQUIREMENTS.md`
- `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md`
- `crates/core/ui/elements/src/style/resolve.rs`
- `crates/core/ui/elements/src/style/types.rs`
- `crates/core/ui/interaction/src/lib.rs`
- `crates/core/frontend/render/src/display_list.rs`
- `crates/core/frontend/render/src/surface/painter/tree.rs`
- `crates/core/frontend/render/src/surface/painter/tests.rs`
- `crates/core/shell/src/shell/component.rs`
- `crates/core/shell/src/shell/component/shell_component.rs`
- `crates/core/shell/src/shell/runtime/debug.rs`
- `crates/core/shell/src/shell/tests.rs`

### External Design Reference
- `https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph-renderer.html`
- `https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html`
</sources>

## Research Conclusion

Phase 27 is best planned as a conservative, render-local optimization pass: separate explicit hidden semantics from generic opacity, omit fully invisible subtrees only where explicit viewport authority exists, and prove the win through aggregate counters in the existing debug payload. The retained display-list collection path is the right seam because it directly targets the current paint-traversal hotspot without dragging future retained-command ownership work into this phase.

## RESEARCH COMPLETE
