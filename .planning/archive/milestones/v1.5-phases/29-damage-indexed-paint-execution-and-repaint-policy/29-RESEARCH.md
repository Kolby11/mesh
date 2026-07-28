# Phase 29: Damage-Indexed Paint Execution and Repaint Policy - Research

**Researched:** 2026-05-11
**Domain:** MESH retained software rendering, damage-indexed display-list execution, and repaint-policy selection
**Confidence:** HIGH

<user_constraints>
## User Constraints from CONTEXT.md

### Locked Decisions
- Damage lookup stays aligned with retained subtree ownership keyed by stable node identity.
- Retained subtrees should carry compact command-span metadata plus aggregate bounds; do not introduce a separate global flat command index.
- Damage filtering should intersect damage against subtree or span bounds first, then preserve retained subtree identity for ordering, fallback, and debug attribution.
- Display-list-owned chrome such as scrollbars should repaint with the owning subtree or viewport, not as global always-paint overlays.
- Tooltip work remains a separate overlay pass and only participates when tooltip state or geometry changes.
- Repaint policy is cost-aware: minimal damage is valid for cheap sparse cases, bounding rect for clustered cases, and full surface for broad or ambiguous cases.
- Filtered execution must preserve display-list ordering, clipping, and batching-barrier semantics.
- Debug proof extends the existing profiling/debug payload. No second trace or benchmark path.

### Deferred Ideas
- Advanced spatial indexing beyond subtree-local span metadata.
- Raster cache hardening for icons, images, text, and glyphs. That remains Phase 30.
- GPU backend work and parallel paint/layout. Those remain later milestones.
</user_constraints>

<research_summary>
## Summary

Phase 29 should be implemented as a focused extension of `mesh-core-render`'s retained display-list seam. Phase 28 already added subtree-owned retained paint commands and aggregate reuse/fallback counters in `crates/core/frontend/render/src/display_list.rs`. The current shell paint path still passes the full `paint_commands()` slice into `paint_display_list_for_module_with_profiling_metrics(...)`, and the software painter still loops every command, using only clip intersection and an optional node-id filter.

The right planning shape is therefore not a new renderer subsystem. It is:

1. Add subtree-local command-span metadata and repaint-policy fields to `RetainedDisplayList`.
2. Use the metadata to produce an ordered filtered command view for the selected damage region, preserving the existing painter order.
3. Extend debug snapshots and shell tests with policy selection, filtered command counts, fallback counts, and before/after benchmark proof hooks.

This should be one execution plan with three tasks. The tasks are sequential because the filtered paint path depends on span metadata and the final debug proof depends on both.
</research_summary>

<existing_implementation_facts>
## Existing Implementation Facts

### Retained Display-List Ownership

- `crates/core/frontend/render/src/display_list.rs` defines `RetainedDisplayList`, `DisplayListMetrics`, `DamageRect`, and `RetainedPaintSubtree`.
- Phase 28 already stores retained paint subtrees keyed by `NodeId` in `RetainedDisplayList::subtrees`.
- `build_paint_subtree(...)` rebuilds dirty subtrees and reuses previous sibling subtrees when retained dirty-node metadata proves that reuse is safe.
- The public consumer boundary is still `RetainedDisplayList::paint_commands() -> &[DisplayPaintCommand]`.

### Existing Damage and Policy Logic

- `DisplayListMetrics` already carries one `damage_rect`, `damage_rect_count`, `damage_area`, `surface_area`, `full_surface_damage`, `partial_present_supported`, and `skipped_paint_pixels`.
- `crates/core/shell/src/shell/component/shell_component.rs` has `select_effective_damage(...)` and `select_damage_policy(...)`.
- The existing policy returns `Minimal`, `BoundingRect`, or `FullRepaint`, but the selected policy is not currently surfaced in debug metrics.
- Effective damage currently selects a clip rectangle, but the painter still receives the complete retained command slice.

### Existing Paint Traversal Seam

- `crates/core/frontend/render/src/surface/mod.rs` exposes `paint_display_list_for_module_with_profiling_metrics(...)`.
- `crates/core/frontend/render/src/surface/painter/tree.rs::render_display_list_for_module(...)` loops all commands, intersects command clip with paint clip, optionally skips commands by `paint_nodes`, and preserves input order.
- This is the natural seam for a filtered command slice: keep the painter simple and ordered, but pass fewer commands into it.

### Existing Overlay and Chrome Behavior

- Scrollbars are represented as `DisplayPaintCommandKind::Scrollbars`, emitted after each node's descendants inside the retained display-list order.
- Tooltip rendering happens after display-list traversal inside `paint_display_list_for_module_with_profiling_metrics(...)`.
- Phase 26 already protects tooltip traversal accounting with `tooltip_overlay_does_not_dominate_paint_traversal_metric`.

### Existing Debug Contract

- `crates/core/foundation/debug/src/lib.rs::RetainedPaintSnapshot` owns aggregate retained paint debug fields.
- `crates/core/shell/src/shell/component.rs::retained_paint_snapshot(...)` maps `DisplayListMetrics` into the debug snapshot.
- `crates/core/shell/src/shell/runtime/debug.rs::profiling_invalidation_json(...)` serializes `invalidation.paint`.
- `crates/core/shell/src/shell/tests.rs` asserts the invalidation/debug JSON shape.
</existing_implementation_facts>

<recommended_technical_shape>
## Recommended Technical Shape

### 1. Keep Span Metadata Local to Retained Subtrees

Extend `RetainedPaintSubtree` or a sibling internal structure with compact span data:

- owner `NodeId`,
- command range within the final flat command vector,
- aggregate bounds for commands in that span,
- command count,
- flags for scrollbar/chrome participation,
- optional fallback marker when span bounds are not trustworthy.

This should remain private to `display_list.rs` unless a public read-only view is needed for tests.

### 2. Add an Explicit Repaint Policy Result

Introduce a render-owned policy enum such as `RepaintPolicy::{MinimalDamage, BoundingRect, FullSurface}` and metrics fields such as:

- `repaint_policy`,
- `filtered_command_count`,
- `filtered_span_count`,
- `filtered_commands_skipped`,
- `filtered_full_fallback_count`,
- `bounding_rect_policy_count`,
- `minimal_damage_policy_count`,
- `full_surface_policy_count`.

The exact names can differ, but the plan should require debug-visible policy selection and filtered traversal proof.

### 3. Feed the Painter a Narrower Ordered Command Slice

Add a method on `RetainedDisplayList` that returns a paint input for a `DamageRect` and policy:

- full surface returns the full command slice,
- bounding rect returns all command spans intersecting the selected bounding rect,
- minimal damage returns only commands/spans intersecting sparse damage when bookkeeping is cheap,
- ambiguous state falls back to full surface.

The selected command list must preserve original display-list order. It can be a borrowed full slice for full repaint and an owned `Vec<DisplayPaintCommand>` or indexed iterator for filtered repaint.

### 4. Keep Shell Code as Orchestration, Not Ownership

Shell component code can merge tooltip damage, call policy selection, pass selected damage to the render crate, and publish debug snapshots. It should not own command-span metadata or filtered-command selection.

### 5. Test Correctness Before Micro-Optimization

The highest-risk failures are missing overlays, stale scrollbars, and ordering regressions. Tests should prove:

- filtered execution skips unrelated commands for sparse damage,
- command order is unchanged among surviving commands,
- scrollbars paint when their owning subtree/viewport participates,
- tooltip damage still triggers overlay repaint without counting as display-list traversal,
- broad dirty summaries and large damage select full-surface fallback.
</recommended_technical_shape>

<candidate_plan_breakdown>
## Candidate Plan Breakdown

### Plan 29-01: Damage-indexed retained paint execution and repaint-policy proof

**Task 29-01-01:** Add subtree span metadata and explicit repaint-policy accounting at the retained display-list seam.

Likely files:
- `crates/core/frontend/render/src/display_list.rs`
- `crates/core/foundation/debug/src/lib.rs`
- `crates/core/shell/src/shell/component.rs`
- `crates/core/shell/src/shell/runtime/debug.rs`

**Task 29-01-02:** Route paint execution through the filtered retained command view while preserving order, clipping, scrollbars, and tooltip overlay behavior.

Likely files:
- `crates/core/frontend/render/src/display_list.rs`
- `crates/core/frontend/render/src/surface/mod.rs`
- `crates/core/frontend/render/src/surface/painter/tree.rs`
- `crates/core/shell/src/shell/component/shell_component.rs`

**Task 29-01-03:** Lock debug proof, policy selection, and benchmark evidence with focused tests.

Likely files:
- `crates/core/frontend/render/src/display_list.rs`
- `crates/core/frontend/render/src/surface/mod.rs`
- `crates/core/frontend/render/src/surface/painter/tree.rs`
- `crates/core/shell/src/shell/tests.rs`
- `crates/core/shell/src/shell/component/tests.rs`
- `.planning/phases/29-damage-indexed-paint-execution-and-repaint-policy/29-01-BENCHMARK.md`
</candidate_plan_breakdown>

<validation_architecture>
## Validation Architecture

| Validation Target | Command | Purpose |
|-------------------|---------|---------|
| Formatting | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check` | Keep Rust formatting stable. |
| Retained display-list span and filtering logic | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list` | Proves span metadata, policy selection, command filtering, ordering, scrollbar inclusion, and fallback behavior. |
| Paint traversal and tooltip regression | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render painter_` and `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render tooltip_overlay` | Protects clipped display-list traversal and tooltip accounting. |
| Shell profiling/debug payload | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling` | Protects `mesh.debug.profiling.surfaces[].invalidation.paint` policy and filtered-execution fields. |
| Phase benchmark proof | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase26_real_surface_baseline_emits_canonical_proof_measurements -- --nocapture` | Reuses canonical shipped-surface scenario IDs and records Phase 29 before/after evidence. |

Required proof properties:

- sparse damage visits fewer retained commands than the full command list,
- filtered command order matches original display-list order,
- scrollbar commands participate when their owner subtree or viewport intersects damage,
- tooltip overlay damage is merged separately and does not inflate display-list traversal metrics,
- minimal, bounding-rect, and full-surface policies are all reachable in focused tests,
- debug JSON exposes policy and filtered execution counters through the existing invalidation payload.
</validation_architecture>

<common_pitfalls>
## Common Pitfalls

### Pitfall 1: Partial damage that still scans all commands
If the implementation only clips every command against damage, it will not satisfy Phase 29. The plan must require a narrower command input to paint traversal.

### Pitfall 2: A global spatial index too early
A new global index would duplicate retained subtree ownership and increase invalidation complexity. Keep span metadata local to retained subtrees unless simple spans prove insufficient in a later phase.

### Pitfall 3: Missing scrollbar or overlay work
Scrollbars are display-list commands owned by their node/subtree. Tooltip is outside display-list traversal. Treating both as the same global overlay would either overpaint or skip visible chrome.

### Pitfall 4: Minimal damage at any cost
Qt guidance says dirty-region selection is policy. Tiny regions can cost more to manage than a bounding rect, and ambiguous retained state should fall back immediately.

### Pitfall 5: Debug payload churn
Use the existing `invalidation.paint` payload. Do not add a separate trace channel or per-command diagnostics stream in Phase 29.
</common_pitfalls>

<sources>
## Sources

### Primary
- `.planning/phases/29-damage-indexed-paint-execution-and-repaint-policy/29-CONTEXT.md`
- `.planning/ROADMAP.md`
- `.planning/REQUIREMENTS.md`
- `.planning/research/SUMMARY.md`
- `.planning/research/v1.4-major-performance-fixes-qt-retained-rendering.md`
- `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md`
- `.planning/phases/27-viewport-culling-and-visibility-elision/27-01-SUMMARY.md`
- `.planning/phases/28-incremental-paint-command-retention/28-01-SUMMARY.md`
- `crates/core/frontend/render/README.md`
- `crates/core/frontend/render/src/display_list.rs`
- `crates/core/frontend/render/src/render_object.rs`
- `crates/core/frontend/render/src/surface/mod.rs`
- `crates/core/frontend/render/src/surface/painter/tree.rs`
- `crates/core/shell/src/shell/component.rs`
- `crates/core/shell/src/shell/component/shell_component.rs`
- `crates/core/shell/src/shell/runtime/debug.rs`
- `crates/core/shell/src/shell/tests.rs`
</sources>

---

*Research complete: 2026-05-11*
