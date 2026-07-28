# Phase 28: Incremental Paint Command Retention - Research

**Researched:** 2026-05-11
**Domain:** MESH retained display-list ownership, dirty-subtree command reuse, and retained paint fallback safety
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Move retained paint-command ownership from the current surface-wide rebuild model toward dirty-subtree ownership keyed by stable retained node identity.
- Keep the ownership seam inside `mesh-core-render`, centered on retained render-object and retained display-list state.
- Treat full per-command global diffing as out of scope; subtree-local retention is the target granularity.
- Transform-, scroll-, and reorder-only changes should preserve unrelated descendant paint data whenever retained metadata can prove local reuse safely.
- Geometry, material, text, clip, opacity, insertion, and removal changes may still invalidate the affected subtree locally, but they should not force unrelated branches to rebuild.
- Reduce z-order and command-signature churn for unchanged branches.
- Preserve a conservative full-surface fallback whenever dirty summaries are too broad, retained ancestry is ambiguous, or local reuse cannot prove correctness cheaply.
- Reuse the existing Phase 26 profiling path and the aggregate retained paint metrics pattern already extended in Phase 27.

### the agent's Discretion
- Choose the exact retained subtree cache representation and splice/update algorithm as long as it remains local to `mesh-core-render`.
- Choose the exact aggregate metric names and where they live inside the existing retained paint/debug payload, provided reuse, subtree rebuild, and fallback behavior remain observable.

### Deferred Ideas (OUT OF SCOPE)
- Damage-indexed command execution and repaint policy selection.
- Global per-command diffing or command-bucket execution filtering.
- Raster/icon/image/text cache work.
- Rewriting the software painter or adding a second debug or benchmark system.
</user_constraints>

<research_summary>
## Summary

Phase 28 should attach to the existing `render_object -> display_list -> surface paint` pipeline by changing **how retained paint commands are owned and refreshed**, not by changing how the software painter executes them. The current retained display list already reuses display entries by key, but it still rebuilds the whole paint-command vector on every retained-tree generation change. That is the bottleneck this phase should remove.

The strongest planning shape is a single execution plan with three tasks:

1. add dirty-subtree command ownership metadata and fallback accounting at the display-list seam,
2. implement subtree-local command refresh for transform/scroll/reorder paths while preserving unchanged descendant payloads,
3. expose aggregate reuse/fallback proof in the existing profiling/debug path and lock behavior with focused render and shell tests.

This keeps Phase 28 squarely inside `mesh-core-render`, preserves the existing paint entrypoints, and sets up Phase 29’s damage-indexed execution without pulling that scope forward.
</research_summary>

<existing_implementation_facts>
## Existing Implementation Facts

### Retained Render Ownership Today

- `crates/core/frontend/render/src/render_object.rs` already computes a `RenderObjectDirtySummary` plus `dirty_node_ids()` keyed by stable `NodeId`.
- The dirty summary already distinguishes `reordered`, `transform`, `clip`, `opacity`, `geometry`, `material`, `text`, and insertion/removal changes.
- `crates/core/shell/src/shell/component/rendering.rs` already treats retained tree reuse as the fast path boundary and calls out future dirty-subtree work directly in comments.

### Retained Display List Behavior Today

- `crates/core/frontend/render/src/display_list.rs` retains `entries: HashMap<DisplayListKey, DisplayListEntry>` and tracks reuse/rebuild/remove counts by key.
- `collect_display_entries(...)` already builds a stable retained-entry map for diffing.
- `collect_paint_commands(...)` still recollects a fresh flat `Vec<DisplayPaintCommand>` for the full surface on each retained generation change.
- `update_metrics_without_rebuild(...)` skips recollection only when the retained tree generation and surface size are unchanged, which is too coarse for local subtree updates.

### Paint and Debug Integration Seams

- `crates/core/frontend/render/src/surface/mod.rs` consumes a flat `&[DisplayPaintCommand]` via `paint_display_list_for_module_with_profiling_metrics(...)`.
- `crates/core/foundation/debug/src/lib.rs` and `crates/core/shell/src/shell/component.rs` already expose aggregate retained paint metrics.
- Phase 27 already added aggregate omission counters to the debug payload, so Phase 28 should extend that same snapshot style for subtree reuse, subtree rebuild, and fallback counts rather than inventing a new proof channel.

### Existing Test Anchors

- `crates/core/frontend/render/src/display_list.rs` already contains tests for command reuse, batching, clipping, and omission metrics.
- `crates/core/shell/src/shell/tests.rs` already locks the `invalidation.paint` JSON shape used by `mesh.debug`.
- `crates/core/frontend/render/src/render_object.rs` already proves dirty-node tracking and dirty summary classification.
</existing_implementation_facts>

<recommended_technical_shape>
## Recommended Technical Shape

### 1. Introduce explicit retained command ownership by subtree

Recommended shape:
- Keep the flat `paint_commands` export for the painter, but derive it from retained subtree-owned segments instead of a full recollection pass.
- Store per-node or per-subtree command slices keyed by stable `NodeId`, together with enough retained ancestry/order metadata to splice them back into the surface-wide flat command order.
- Reuse `render_object.rs` dirty-node IDs as the primary signal for which subtree-owned command segments need regeneration.

Why:
- The stable node identity already exists.
- The painter still wants a flat ordered slice.
- Subtree-local segments allow local replacement while preserving unrelated branches.

### 2. Split local reuse paths from conservative fallback paths

Recommended behavior:
- Transform-only, scroll-only, and reorder-only changes should take targeted subtree refresh paths.
- Insert/remove, ambiguous ancestry, root replacement, or very broad dirty sets should trigger a conservative full rebuild fallback.
- Geometry/material/text/clip/opacity changes should rebuild only the affected subtree segment and ancestors needed to preserve ordering/clip correctness.

Why:
- This matches the locked decisions.
- It prevents correctness risk from over-aggressive local reuse.
- It preserves a clear safety escape hatch for complex dirty summaries.

### 3. Minimize signature churn for unchanged branches

Recommended behavior:
- Keep unchanged subtree command signatures stable when sibling edits occur.
- Recompute ordering metadata only for the edited subtree span and any directly affected ancestor spans.
- Avoid renumbering unrelated branches just because a nearby subtree changed.

Why:
- Requirement `PIPE-02` is not only about skipping geometry/style invalidation; it also requires retention stability for transform/scroll/reorder activity.

### 4. Keep proof aggregate and phase-local

Recommended categories:
- subtree segments reused,
- subtree segments rebuilt,
- subtree-local command count rebuilt,
- full fallback count,
- local reuse blocked count or broad-dirty fallback count.

Avoid:
- per-command trace logs,
- second profiling payloads,
- Phase 29 damage-region accounting work.
</recommended_technical_shape>

<candidate_plan_breakdown>
## Candidate Plan Breakdown

### Plan 28-01: Dirty-subtree retained command ownership and local reuse proof

**Task 28-01-01:** Add retained command ownership metadata and fallback metrics at the display-list seam.
- Likely files:
  - `crates/core/frontend/render/src/display_list.rs`
  - `crates/core/frontend/render/src/render_object.rs`
  - `crates/core/foundation/debug/src/lib.rs`
  - `crates/core/shell/src/shell/component.rs`
  - `crates/core/shell/src/shell/runtime/debug.rs`

**Task 28-01-02:** Implement subtree-local refresh for transform/scroll/reorder changes while preserving unrelated descendant command payloads.
- Likely files:
  - `crates/core/frontend/render/src/display_list.rs`
  - `crates/core/frontend/render/src/surface/mod.rs`
  - `crates/core/shell/src/shell/component/rendering.rs`

**Task 28-01-03:** Add focused render and shell proof for local reuse, subtree rebuild boundaries, and conservative fallback behavior.
- Likely files:
  - `crates/core/frontend/render/src/display_list.rs`
  - `crates/core/frontend/render/src/render_object.rs`
  - `crates/core/shell/src/shell/tests.rs`
</candidate_plan_breakdown>

<common_pitfalls>
## Common Pitfalls

### Pitfall 1: Rebuilding the flat command vector anyway
If subtree metadata exists but `paint_commands` is still always recollected from the whole tree, Phase 28 does not actually satisfy `PIPE-01`.

### Pitfall 2: Letting reorder fixes renumber unrelated branches
This would preserve correctness but violate the phase goal by causing churn in unchanged retained paint data.

### Pitfall 3: Pulling damage-indexed execution into this phase
If the implementation starts filtering execution by damage region, it overlaps Phase 29 and makes Phase 28 harder to prove in isolation.

### Pitfall 4: Removing the conservative fallback
Complex dirty ancestry or broad invalidation needs an explicit safe fallback; otherwise false-positive reuse bugs will become hard-to-debug visual corruption.
</common_pitfalls>

<validation_architecture>
## Validation Architecture

Phase 28 validation should stay render- and shell-focused, with aggregate proof after each task.

| Validation Target | Command | Purpose |
|-------------------|---------|---------|
| Formatting | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check` | Keep Rust formatting stable. |
| Retained display list behavior | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list` | Proves subtree-local command reuse, local rebuild, and fallback accounting at the retained display-list seam. |
| Render-object dirty tracking | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render render_object` | Protects the dirty-node and change-type signals that drive subtree ownership updates. |
| Shell profiling/debug snapshot | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling` | Protects `mesh.debug` invalidation payload shape and Phase 28 aggregate metrics. |

Required proof properties:
- transform-, scroll-, and reorder-only updates preserve unrelated descendant command payloads,
- subtree-local geometry/material/text edits rebuild only the affected subtree span plus required ancestry,
- broad or ambiguous dirty summaries take the conservative full fallback path,
- aggregate reuse/rebuild/fallback counters appear only in the existing invalidation/debug payload.
</validation_architecture>

<sources>
## Sources

### Primary (HIGH confidence)
- `.planning/phases/28-incremental-paint-command-retention/28-CONTEXT.md`
- `.planning/ROADMAP.md`
- `.planning/REQUIREMENTS.md`
- `.planning/STATE.md`
- `crates/core/frontend/render/src/render_object.rs`
- `crates/core/frontend/render/src/display_list.rs`
- `crates/core/frontend/render/src/surface/mod.rs`
- `crates/core/frontend/render/src/surface/painter/tree.rs`
- `crates/core/foundation/debug/src/lib.rs`
- `crates/core/shell/src/shell/component.rs`
- `crates/core/shell/src/shell/component/rendering.rs`
- `crates/core/shell/src/shell/runtime/debug.rs`

### Prior-phase references
- `.planning/phases/27-viewport-culling-and-visibility-elision/27-CONTEXT.md`
- `.planning/phases/27-viewport-culling-and-visibility-elision/27-RESEARCH.md`
- `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md`
</sources>

## Research Conclusion

Phase 28 should refactor retained paint-command ownership inside `mesh-core-render` so command data becomes subtree-local and selectively refreshable, while preserving the existing flat paint-consumer boundary and a conservative full-rebuild fallback. The code already has the right raw signals: stable node IDs, dirty summaries, retained entry reuse, and aggregate debug metrics. The missing piece is subtree-owned command retention plus proof that local changes no longer force whole-surface command recollection.

## RESEARCH COMPLETE
