# Phase 25: Display-List Batching and GPU Readiness Guardrails - Context

**Gathered:** 2026-05-09
**Status:** Ready for planning

<domain>
## Phase Boundary

Batch compatible retained display primitives by material/state while preserving ordering, clipping, opacity, transform, text, and translucent-overlap correctness. Leave explicit guardrails and handoff criteria for a later GPU backend without implementing GPU rendering or replacing the software painter.

</domain>

<decisions>
## Implementation Decisions

### Batching Model
- Batch compatible retained display primitives with the same material/state and no ordering barrier.
- Keep batching in the `mesh-core-render` retained display-list layer from Phase 22.
- Preserve correctness through explicit barriers for clip, opacity, transform, text, and translucent overlap.
- Produce batch summaries and barrier reasons rather than GPU commands.

### GPU Readiness
- GPU readiness means data and ordering boundaries are explicit enough for a later backend, without implementing GPU rendering.
- The software painter remains authoritative and unchanged.
- Represent barriers with typed enums plus debug-readable reason strings.
- Document handoff criteria for GPU and parallel paint/layout milestones.

### Milestone Proof
- Final proof covers retained dirty data, render objects, display-list damage, text cache, selector index, and batching metrics through tests or debug visibility.
- Use existing profiling/debug snapshot paths and focused unit tests rather than a new benchmark harness.
- Report opportunities, actual skipped work, barriers, and unsupported backend limits honestly.
- After Phase 25 passes, autonomous lifecycle should proceed to milestone audit/complete/cleanup.

### the agent's Discretion
The agent may choose exact batch metric names and barrier grouping as long as batching stays observational/metadata-only and does not change paint output.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/frontend/render/src/display_list.rs` owns retained display entries, damage, and paint metrics.
- `RetainedPaintSnapshot` already exposes display-list metrics through debug profiling snapshots.
- Prior phases added retained render objects, text cache metrics, and selector index tests.

### Established Patterns
- Render optimizations are proven with focused unit tests and debug profiling counters.
- Current presentation/painter code remains full-buffer software rendering unless a backend capability says otherwise.
- GSD compatibility CLI may need direct roadmap/requirements patching after phase completion.

### Integration Points
- Extend retained display-list metrics with batch count, merged primitive count, barrier count, and readable barrier reasons.
- Thread these metrics into `RetainedPaintSnapshot` and debug JSON.
- Add a short GPU readiness proof artifact for milestone handoff criteria.

</code_context>

<specifics>
## Specific Ideas

Batch adjacent compatible primitives only when material signatures match and no barrier slot/reason applies. Treat text, icon, opacity, clipping, and translucent content as barriers for now.

</specifics>

<deferred>
## Deferred Ideas

GPU backend implementation, WGPU abstractions, parallel paint/layout, and replacing the software painter remain deferred.

</deferred>
