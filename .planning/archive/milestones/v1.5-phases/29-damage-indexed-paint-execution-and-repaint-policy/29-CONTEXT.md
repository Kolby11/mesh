# Phase 29: Damage-Indexed Paint Execution and Repaint Policy - Context

**Gathered:** 2026-05-11
**Status:** Ready for planning

<domain>
## Phase Boundary

Make partial-damage paints on the CPU retained-rendering path scale with the changed region instead of total surface complexity. This phase adds damage-to-command execution filtering and a measured repaint-policy switch on top of Phase 28's subtree-local command retention. It does not introduce GPU work, new raster caches, a second diagnostics system, or a browser-engine-style global invalidation architecture.

</domain>

<decisions>
## Implementation Decisions

### Damage Index Shape
- **D-01:** Follow the Qt retained-rendering model by keeping damage lookup aligned with retained subtree ownership keyed by stable node identity; do not introduce a brand-new global flat command index.
- **D-02:** Each retained subtree should carry compact command-span metadata plus aggregate bounds so partial paints can filter within a subtree without abandoning Phase 28's ownership model.
- **D-03:** Span lookup should use both retained ownership and geometry: intersect damage against subtree or span bounds first, then preserve retained subtree identity as the authority for ordering, fallback, and debug attribution.

### Overlay and Chrome Inclusion
- **D-04:** Display-list-owned chrome such as scrollbars should repaint when their owning subtree or viewport participates in damage, or when their own retained geometry changes; they are not global always-paint overlays.
- **D-05:** Tooltip-style overlay work that already lives outside display-list traversal should remain a separate overlay pass and should only force repaint when tooltip state or geometry changes, not on every retained display-list damage event.
- **D-06:** Stay Qt-like and conservative about clip/state complexity: prefer owner-root or viewport-root overlay inclusion over introducing extra per-primitive clipping just to make tiny overlay damage more precise.

### Repaint Policy
- **D-07:** Dirty-region selection is a cost-aware policy choice, not "always compute the smallest region." Minimal-damage repaint is the default only when span filtering is cheap and the dirty set is sparse.
- **D-08:** Escalate to a coarser bounding-rect repaint when many small damage hits cluster within the same retained subtree or viewport, or when region bookkeeping becomes more expensive than repainting the combined area.
- **D-09:** Escalate to full-surface repaint when dirty summaries are broad, retained ancestry is ambiguous, root-level clip or opacity state changed, or filtered execution cannot cheaply prove correctness.

### Correctness and Fallback Safety
- **D-10:** Filtered execution must preserve existing display-list ordering and batching-barrier semantics; damage filtering skips unrelated spans but must never reorder the surviving commands.
- **D-11:** When clip, z-order, opacity, transform ancestry, or mixed subtree ownership makes filtered execution unclear, prefer the broader repaint policy immediately rather than adding deeper per-command clip/state machinery.
- **D-12:** Debug proof should extend the existing profiling and debug metrics with repaint-policy selection, filtered-span hit counts, and fallback counters; do not create a second trace or benchmark path.

### the agent's Discretion
- Planner and researcher may choose the exact subtree span representation, such as stored command ranges, block IDs, or another compact span descriptor, provided it stays local to `mesh-core-render` and keeps retained subtree ownership as the primary seam.
- Planner and researcher may choose the exact repaint-policy thresholds and heuristics, provided the policy remains benchmark-driven, conservative around correctness, and observable through the existing debug payload.
- Planner and researcher may choose the exact metric names and payload placement for filtered execution and repaint-policy counters, provided they extend the current profiling and invalidation/debug surfaces instead of introducing a new diagnostics channel.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase Scope and Milestone Constraints
- `.planning/ROADMAP.md` — Phase 29 goal, dependencies, planned work, and milestone exclusions.
- `.planning/REQUIREMENTS.md` — active requirements `PIPE-03`, `PIPE-04`, and `CULL-03`, plus out-of-scope constraints that keep GPU and cache work in later phases.
- `.planning/PROJECT.md` — milestone-level priorities: CPU smoothness before GPU work, Qt-guided retained rendering, and visible smoothness over synthetic wins.
- `.planning/STATE.md` — current milestone position and accumulated retained-rendering decisions carried into Phase 29.

### Qt Rendering Guidance
- `.planning/research/SUMMARY.md` — milestone research summary that explicitly sequences Phase 29 after subtree-local retention and locks damage filtering plus cost-aware repaint policy as the next step.
- `.planning/research/v1.4-major-performance-fixes-qt-retained-rendering.md` — Qt Quick and QBackingStore lessons that lock this phase to retained-node ownership, partial-update policy switching, and conservative fallback behavior.

### Prior Phase Decisions and Proof
- `.planning/phases/28-incremental-paint-command-retention/28-CONTEXT.md` — Phase 28 decision to keep retained paint-command ownership subtree-local inside `mesh-core-render`.
- `.planning/phases/27-viewport-culling-and-visibility-elision/27-CONTEXT.md` — clipping and viewport guidance that Phase 29 should preserve instead of proliferating per-item clip complexity.
- `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md` — baseline proof showing that paint traversal and whole-surface retained work remain dominant hotspots on shipped surfaces.

### Renderer Ownership and Integration Seams
- `crates/core/frontend/render/README.md` — renderer ownership boundary; Phase 29 must stay inside `mesh-core-render`.
- `crates/core/frontend/render/src/render_object.rs` — dirty summaries and stable dirty node IDs that anchor damage routing to retained identities.
- `crates/core/frontend/render/src/display_list.rs` — retained subtree ownership, current damage rect metrics, command collection, and the natural seam for subtree-local span metadata and repaint-policy logic.
- `crates/core/frontend/render/src/surface/mod.rs` — display-list paint entrypoint and profiling boundary, including the separate tooltip overlay pass.
- `crates/core/frontend/render/src/surface/painter/tree.rs` — current display-list traversal path that still scans commands and filters by node ID, which Phase 29 should narrow using subtree-local span filtering.
- `crates/core/shell/src/shell/component/rendering.rs` — shell-to-render boundary and tooltip overlay sizing behavior that informs overlay repaint inclusion.

### External Specs
- No new external spec or ADR was introduced during this discussion; downstream guidance is fully captured by the repository references above and the locked Qt research artifacts.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/frontend/render/src/display_list.rs`: already owns retained subtrees, whole-surface damage accounting, fallback metrics, and command vectors, so it is the correct place to add subtree-local span metadata and repaint-policy selection.
- `crates/core/frontend/render/src/render_object.rs`: already exposes stable dirty node IDs and dirty summaries that can anchor damage filtering to retained ownership rather than a new global indexing system.
- `crates/core/frontend/render/src/surface/painter/tree.rs`: already respects command order, clip intersection, and scrollbar rendering, so Phase 29 can narrow traversal inputs without rewriting the software painter.
- `crates/core/frontend/render/src/surface/mod.rs`: already separates display-list traversal timing from tooltip overlay rendering, which is the right seam for Phase 29's overlay inclusion policy.

### Established Patterns
- Render-path optimizations stay inside `mesh-core-render`; shell/runtime code should expose state and profiling hooks, not own retained repaint policy.
- Qt guidance is applied as retained-tree and policy inspiration, not as a literal port of Qt internals or a browser-engine-style invalidation system.
- Fast paths are layered onto compatibility-preserving fallbacks with explicit debug counters so false precision does not silently regress correctness.

### Integration Points
- Phase 29 should attach at the retained display-list boundary, where Phase 28 already established subtree-local command ownership and where damage metadata is already computed.
- The execution filter should feed the existing display-list painter with a narrower command subset or subtree span selection while preserving current ordering and batch-barrier behavior.
- Repaint-policy decisions should surface through the same profiling and debug payload chain already used for retained generation, damage area, omission, and traversal proof.

</code_context>

<specifics>
## Specific Ideas

- The decisions in this context are intentionally derived from the Qt retained-rendering model already adopted for MESH: stable retained ownership first, then narrower execution inside that retained structure, with repaint region selection treated as a measured policy.
- For this phase, "Qt-like" means subtree-local retained ownership with compact span metadata, conservative clipping/state behavior, and fast escalation from minimal damage to bounding-rect or full-surface repaint when correctness or bookkeeping cost becomes unclear.

</specifics>

<deferred>
## Deferred Ideas

- More advanced spatial indexing beyond subtree-local span metadata remains future work only if simple retained subtree plus span lookup proves insufficient on real surfaces.
- Raster cache hardening for icons, images, text, and glyphs remains Phase 30 work.
- GPU backend work and any parallel paint or layout experimentation remain later milestones after the CPU retained path is visibly smooth.

### Reviewed Todos (not folded)
- `2026-05-08-create-unified-package-and-module-manifest-phase.md` — reviewed because phase matching saw the word "phase", but it is unrelated to retained CPU repaint execution and remains separate backlog planning work.

</deferred>

---

*Phase: 29-Damage-Indexed Paint Execution and Repaint Policy*
*Context gathered: 2026-05-11*
