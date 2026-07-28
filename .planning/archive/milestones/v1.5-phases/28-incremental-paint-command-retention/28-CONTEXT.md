# Phase 28: Incremental Paint Command Retention - Context

**Gathered:** 2026-05-11
**Status:** Ready for planning

<domain>
## Phase Boundary

Stop local retained-tree changes from forcing whole-surface paint-command recollection on the CPU software-render path. This phase clarifies how retained paint-command ownership should become subtree-local, how transform/scroll/reorder-only mutations may preserve unrelated descendant paint data, and when the renderer must still fall back to the current full-surface rebuild path for correctness. It does not add damage-indexed command execution, new raster caches, GPU work, or a new diagnostics system.

</domain>

<decisions>
## Implementation Decisions

### Ownership Granularity
- **D-01:** Move retained paint-command ownership from the current surface-wide flat rebuild model toward dirty-subtree ownership keyed by stable retained node identity.
- **D-02:** Keep the ownership seam inside `mesh-core-render`, centered on retained render-object and retained display-list state, rather than introducing shell- or runtime-level paint retention bookkeeping.
- **D-03:** Treat full per-command global diffing as out of scope for this phase; subtree-local retention is the target granularity.

### Dirty-Change Handling
- **D-04:** Transform-, scroll-, and reorder-only changes should preserve unrelated descendant paint data whenever the affected subtree can be updated safely from retained metadata.
- **D-05:** Geometry, material, text, clip, opacity, insertion, and removal changes may still invalidate the affected subtree locally, but they should not automatically force unrelated branches to rebuild.
- **D-06:** Ancestor-path metadata updates are allowed when needed for correctness, but unchanged sibling branches should keep their retained paint data and signatures.

### Ordering and Fallback Safety
- **D-07:** Reduce z-order and command-signature churn for unchanged branches; subtree edits should not renumber or re-signature unrelated retained paint data unless ordering correctness requires it.
- **D-08:** Preserve a conservative full-surface fallback whenever dirty summaries are too broad, retained ancestry is ambiguous, or a local reuse path cannot prove correctness cheaply.
- **D-09:** Phase 28 should favor compatibility-preserving local reuse layered onto the current rebuild path, not a rewrite of the software painter or presentation flow.

### Debug and Proof
- **D-10:** Reuse the existing Phase 26 profiling and benchmark proof path plus Phase 27 aggregate-metric style; do not add a second trace or benchmark system.
- **D-11:** Debug output should expose aggregate retained-command reuse, subtree rebuild, and fallback behavior clearly enough to catch false-positive reuse without requiring per-command trace dumps.

### the agent's Discretion
- Planner and researcher may choose the exact retained subtree cache representation and splice/update algorithm, provided ownership stays local to `mesh-core-render` and preserves existing visual output.
- Planner and researcher may choose exact metric names and payload placement inside the existing profiling/debug snapshot path, provided subtree reuse, rebuild, and fallback behavior remain observable.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase Scope and Milestone Constraints
- `.planning/ROADMAP.md` — Phase 28 goal, dependencies, and planned work for incremental retained paint-command ownership.
- `.planning/REQUIREMENTS.md` — active requirements `PIPE-01` and `PIPE-02`, plus adjacent milestone boundaries that keep damage-indexed execution and caching in later phases.
- `.planning/PROJECT.md` — milestone-level priorities: CPU smoothness before GPU work, Qt-inspired retained rendering direction, and benchmark-visible responsiveness over synthetic wins.
- `.planning/STATE.md` — current milestone/session position and existing retained-rendering decisions carried forward into v1.5.

### Prior Phase Decisions and Proof
- `.planning/phases/27-viewport-culling-and-visibility-elision/27-CONTEXT.md` — Phase 27 decision to keep pruning scoped to omission/elision rather than pulling retained-command ownership changes forward early.
- `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md` — baseline proof showing paint traversal and full rebuild paths as the dominant shipped-surface hotspots.
- `.planning/milestones/v1.4-phases/25-display-list-batching-and-gpu-readiness-guardrails/25-CONTEXT.md` — retained display-list ownership, batching barrier constraints, and the rule that the software painter remains authoritative.
- `.planning/milestones/v1.4-phases/23-text-shaping-and-glyph-cache/23-CONTEXT.md` — precedent for keeping optimization work inside `mesh-core-render` and proving it through focused tests plus existing debug metrics.

### Renderer Ownership and Integration Seams
- `crates/core/frontend/render/README.md` — render crate ownership boundary; retained paint-command logic should stay in `mesh-core-render`.
- `crates/core/frontend/render/src/render_object.rs` — retained render-object generations, dirty summaries, and dirty node identities that can drive subtree-local command retention.
- `crates/core/frontend/render/src/display_list.rs` — current retained display entry diffing, paint-command collection, batch metrics, and whole-surface command ownership that Phase 28 must refine.
- `crates/core/frontend/render/src/surface/mod.rs` — current surface paint entrypoints and retained display-list consumption boundary.
- `crates/core/frontend/render/src/surface/painter/tree.rs` — current display-command traversal and paint execution path that should remain the consumer rather than being rewritten in this phase.
- `crates/core/shell/src/shell/component/rendering.rs` — retained tree finalize/annotation flow that supplies transform, scroll, and overflow state into the render pipeline.

### External Specs
- No additional external spec or ADR was introduced during this run; downstream guidance is fully captured by the repository references above.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/frontend/render/src/render_object.rs`: already tracks dirty node IDs plus transform/reorder/clip/opacity/geometry/material/text change buckets, which gives Phase 28 a stable dirty-root signal.
- `crates/core/frontend/render/src/display_list.rs`: already separates retained display-entry diffing from paint-command collection and keeps prior metrics/state across generations, which is the natural ownership seam for subtree retention.
- `crates/core/frontend/render/src/surface/painter/tree.rs`: already consumes retained display commands as a separate traversal step, so Phase 28 can change command retention without replacing paint execution.
- `crates/core/shell/src/shell/component/rendering.rs`: already annotates runtime tree overflow and scroll state before render-object/display-list work begins, so transform and scroll metadata already exists upstream.

### Established Patterns
- Render-path optimizations stay inside `mesh-core-render`; shell/runtime code should feed state and debug plumbing, not own renderer retention logic.
- Fast paths are added conservatively on top of compatibility-preserving fallbacks rather than replacing the old path all at once.
- Performance proof should extend the existing benchmark scenarios and aggregate debug metrics rather than inventing a new inspection channel.

### Integration Points
- Phase 28 likely lands between render-object dirty summaries and display-list command generation, using stable node IDs to decide which retained command segments can survive.
- Reorder, transform, and scroll handling must coordinate with current display-entry signatures and batch/barrier semantics so unchanged branches do not churn unnecessarily.
- Fallback and observability should thread through existing display-list metrics and retained profiling snapshots that the shell already serializes.

</code_context>

<specifics>
## Specific Ideas

- Favor retained paint-command ownership per dirty subtree or branch rather than a single flat command-vector rebuild for the whole surface.
- Keep ancestor metadata patching cheap for transform/scroll/reorder-only updates while preserving unchanged descendant command payloads where safe.
- Treat this phase as the ownership/stability precursor to Phase 29 rather than pulling damage-indexed command filtering or repaint-policy heuristics forward early.

</specifics>

<deferred>
## Deferred Ideas

- Damage-indexed command execution, filtered repaint policy, and minimal-damage vs full repaint policy selection remain Phase 29 work.
- SVG/bitmap/icon/text/glyph cache retention remains Phase 30 work.

### Reviewed Todos (not folded)
- `2026-05-08-create-unified-package-and-module-manifest-phase.md` — reviewed because the matcher found the word "phase", but it is unrelated to retained paint-command ownership and remains separate backlog work.

</deferred>

---

*Phase: 28-Incremental Paint Command Retention*
*Context gathered: 2026-05-11*
