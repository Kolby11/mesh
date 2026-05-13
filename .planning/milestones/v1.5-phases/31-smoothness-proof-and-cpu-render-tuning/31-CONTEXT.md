# Phase 31: Smoothness Proof and CPU Render Tuning - Context

**Gathered:** 2026-05-12
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 31 tunes the completed CPU retained-rendering pipeline and proves that shipped shell surfaces are visibly smoother. It covers measured threshold tuning, cache/repaint/clear/background heuristics, canonical benchmark comparison, focused manual UAT, and final documentation of what remains future GPU or parallel work. It does not introduce a new benchmark system, renderer architecture rewrite, GPU backend, parallel paint/layout, or new shell UI features.

</domain>

<decisions>
## Implementation Decisions

### Smoothness Acceptance Proof
- **D-01:** Phase 31 acceptance uses mixed proof: canonical benchmark evidence plus focused manual UAT notes on shipped surfaces.
- **D-02:** Benchmark data is necessary but not sufficient; this phase exists to prove user-visible smoothness, not just improved internal counters.
- **D-03:** Manual UAT should stay focused on the canonical shipped-surface scenarios: `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, and `backend_update`.

### Tuning Posture
- **D-04:** Use a conservative tuning posture. Tune measured thresholds and heuristics only where existing proof shows a win.
- **D-05:** Avoid structural rewrites in Phase 31. The retained rendering architecture, damage filtering, and raster cache ownership established in Phases 27-30 are inputs to tune, not systems to redesign.
- **D-06:** Changes should be small enough to explain with before/after benchmark evidence and targeted tests.

### Correctness Guardrails
- **D-07:** Use strict correctness guardrails. Every tuning change needs tests or UAT notes showing visuals and interactions remain unchanged apart from smoother rendering.
- **D-08:** If a tuning change cannot prove visual and interaction correctness cheaply, prefer leaving it conservative and documenting it for future work.
- **D-09:** Repaint/cache threshold changes must preserve existing display-list ordering, clipping, scrollbar inclusion, overlay behavior, cache freshness, and opacity/translucency conservatism.

### the agent's Discretion
- Planner may choose the exact threshold values, benchmark comparison format, and UAT note structure, provided they honor D-01 through D-09.
- Planner may choose whether to implement one combined tuning plan or split tuning and proof into separate plans, provided all Phase 31 requirements are covered.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase Scope and Requirements
- `.planning/ROADMAP.md` — Phase 31 goal, dependencies, planned work, and milestone boundaries.
- `.planning/REQUIREMENTS.md` — active requirements `PERF-03`, `SMTH-01`, `SMTH-02`, and `SMTH-03`.
- `.planning/PROJECT.md` — milestone-level priority: CPU smoothness before GPU or parallel work.
- `.planning/STATE.md` — current phase position and completed Phase 30 transition.

### Prior Phase Decisions and Proof
- `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md` — original canonical benchmark baseline and scenario identities.
- `.planning/phases/27-viewport-culling-and-visibility-elision/27-CONTEXT.md` — visibility/culling constraints that remain correctness guardrails.
- `.planning/phases/28-incremental-paint-command-retention/28-CONTEXT.md` — retained paint-command ownership and fallback constraints.
- `.planning/phases/29-damage-indexed-paint-execution-and-repaint-policy/29-CONTEXT.md` — repaint-policy and damage-filtering decisions to tune conservatively.
- `.planning/phases/30-raster-cache-hardening-for-icons-images-and-text/30-CONTEXT.md` — cache ownership, cache proof, and Phase 31 boundary.
- `.planning/phases/30-raster-cache-hardening-for-icons-images-and-text/30-01-BENCHMARK.md` — latest deterministic cache and canonical scenario evidence.
- `.planning/phases/30-raster-cache-hardening-for-icons-images-and-text/30-VERIFICATION.md` — Phase 30 verification status and residual risk.

### Renderer and Profiling Code
- `crates/core/frontend/render/src/display_list.rs` — retained display-list metrics, repaint policy, damage filtering, batching barriers, and cached icon opacity decisions.
- `crates/core/frontend/render/src/surface/mod.rs` — display-list paint entrypoint and paint profiling metrics.
- `crates/core/frontend/render/src/surface/profiling.rs` — raster cache and raster timing counters.
- `crates/core/frontend/render/src/surface/icon.rs` — raster cache behavior, freshness/bypass logic, and cached resource opacity.
- `crates/core/frontend/render/src/surface/text.rs` — text layout cache metrics and reuse behavior.
- `crates/core/shell/src/shell/component/tests/invalidation/profiling.rs` — canonical shipped-surface profiling proof test.
- `crates/core/shell/src/shell/runtime/debug.rs` — debug/profiling payload serialization.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `RetainedDisplayList` and `DisplayListMetrics`: existing place to tune repaint-policy thresholds and prove fallback behavior.
- `PaintProfilingMetrics`: existing path for text/raster/traversal proof; extend only if existing fields are insufficient.
- `phase26_real_surface_baseline_emits_canonical_proof_measurements`: existing shipped-surface benchmark proof path for the five canonical scenarios.
- `30-01-BENCHMARK.md`: latest Phase 30 evidence to compare against when Phase 31 tuning changes behavior.

### Established Patterns
- Render-path optimizations stay in `mesh-core-render`; shell code should expose proof and feed state, not own renderer policy.
- Prefer compatibility-preserving fast paths layered onto conservative fallbacks.
- Existing profiling/debug payloads are the acceptance path; do not create another diagnostics surface.
- Use focused Rust tests and benchmark artifacts rather than broad manual inspection.

### Integration Points
- Repaint threshold tuning likely connects through `display_list.rs` policy and barrier metrics.
- Cache behavior tuning likely connects through `surface/icon.rs`, `surface/text.rs`, and `surface/profiling.rs`.
- Smoothness proof connects through shell profiling tests, benchmark artifacts, and a Phase 31 UAT record.

</code_context>

<specifics>
## Specific Ideas

- Mixed proof is the accepted shape: benchmark evidence plus manual UAT notes.
- Conservative tuning is preferred over a late milestone rewrite.
- Strict visual and interaction correctness proof is required for every tuning change.

</specifics>

<deferred>
## Deferred Ideas

### Reviewed Todos (not folded)
- `2026-05-08-create-unified-package-and-module-manifest-phase.md` — matched by generic planning terms but unrelated to CPU smoothness tuning. Keep as separate backlog/future planning work.

</deferred>

---

*Phase: 31-Smoothness Proof and CPU Render Tuning*
*Context gathered: 2026-05-12*
