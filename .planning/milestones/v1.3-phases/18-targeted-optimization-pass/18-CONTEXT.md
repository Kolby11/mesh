# Phase 18: Targeted Optimization Pass - Context

**Gathered:** 2026-05-09
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase uses the debug-only profiler and the fixed canonical benchmark suite from Phase 17 to identify the worst measured responsiveness hotspot, land a bounded high-impact optimization, and prove a measurable before/after improvement.

This phase covers:
- Capturing a fresh baseline at the start of Phase 18 using the canonical benchmark suite.
- Selecting the optimization target from profiler evidence, not speculation.
- Prioritizing render invalidation, restyle/layout churn, redraw volume, input-to-visible-response latency, and backend paths only when they show visible frontend impact.
- Proving at least one focused before/after improvement of 10% or more on the selected metric.
- Preserving profiling-off behavior, visual output, benchmark contracts, and backend/service semantics.

This phase does not add:
- New benchmark scenarios beyond the five canonical Phase 17 scenarios.
- Trace capture, replay, export, persistence, or benchmark history storage.
- Renderer architecture replacement, GPU renderer work, or broad shell/runtime redesign.
- Backend architecture redesign unless profiler evidence shows material visible impact.
- Package/module manifest restructuring; that remains a separate deferred planning item.

</domain>

<decisions>
## Implementation Decisions

### Hotspot Selection
- **D-01:** Phase 18 should choose the optimization target from fresh profiler measurements captured at the start of the phase.
- **D-02:** The default target is the worst measured canonical benchmark or stage, not the safest speculative improvement.
- **D-03:** If two hotspots are close, prefer the one with the largest absolute latency.
- **D-04:** Backend paths are eligible only when the profiler shows material visible frontend impact; otherwise prioritize render invalidation, restyle/layout churn, redraw volume, and input-to-visible-response latency.

### Proof Standard
- **D-05:** Phase 18 should prove one focused before/after improvement rather than requiring a full before/after comparison for all five benchmark scenarios.
- **D-06:** The selected metric must improve by at least 10% to count as measurable improvement for `OPT-02`.
- **D-07:** The proof should include fresh baseline data, post-change data, and a concise explanation of which benchmark/stage improved.
- **D-08:** Focused regression tests are required around the optimized path and the benchmark/profiling contract it relies on.

### Regression Guardrails
- **D-09:** Profiling-off behavior must remain unchanged.
- **D-10:** Visual output and shipped shell surface behavior must remain unchanged unless a future phase explicitly scopes a UI change.
- **D-11:** Benchmark scenario ids, payload shape, launch request semantics, and status behavior from Phase 17 must remain stable.
- **D-12:** Backend/service semantics must remain unchanged; Rust core must stay generic and must not gain service-specific audio or provider business logic.

### the agent's Discretion
- Planner/researcher may choose the exact benchmark command sequence and measurement format, as long as a fresh baseline and post-change comparison are captured.
- Planner/researcher may choose the concrete optimization technique after inspecting profiler output, as long as the change remains bounded and satisfies the guardrails above.
- Planner/researcher may choose the exact regression test selectors, but tests should be focused enough to run during phase execution without turning this into a full-suite performance project.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone and Phase Scope
- `.planning/PROJECT.md` - v1.3 milestone framing, current Phase 18 focus, active optimization requirements, and locked debug-only profiling boundaries.
- `.planning/REQUIREMENTS.md` - `OPT-01`, `OPT-02`, and `OPT-03`, plus traceability to Phase 18.
- `.planning/ROADMAP.md` - Phase 18 goal, dependency on Phase 17, planned optimization priorities, and milestone exclusions.
- `.planning/STATE.md` - carried-forward decisions, Phase 17 benchmark decisions, and deferred package/module manifest todo.

### Prior Phase Context That Must Carry Forward
- `.planning/phases/14-profiling-data-model-and-timing-hooks/14-CONTEXT.md` - locked profiling data model, debug-only activation, bounded rolling samples, and stage list.
- `.planning/phases/15-backend-and-surface-attribution/15-VERIFICATION.md` - proof that backend provider/stage attribution and comparable per-surface timing snapshots exist.
- `.planning/phases/16-debug-only-profiling-mode-and-live-inspector/16-CONTEXT.md` - locked inspector host model, debug-only profiling mode, sparse-state behavior, and benchmark scaffold boundary.
- `.planning/phases/17-canonical-benchmark-scenarios-and-proof-flows/17-CONTEXT.md` - canonical scenario identities, shipped-surface anchors, benchmark result contract, and proof strategy.
- `.planning/phases/17-canonical-benchmark-scenarios-and-proof-flows/17-VERIFICATION.md` - verification evidence for benchmark scenario coverage and backend/frontend correlation.
- `.planning/phases/17-canonical-benchmark-scenarios-and-proof-flows/17-04-SUMMARY.md` - final Phase 17 backend-to-frontend benchmark correlation behavior and focused proof commands.

### Debug, Profiling, Benchmark, and Runtime Code
- `crates/core/foundation/debug/src/lib.rs` - `DebugSnapshot`, benchmark snapshot types, profiling snapshot types, and benchmark status labels.
- `crates/core/shell/src/shell/runtime/debug.rs` - debug service payload assembly, benchmark row derivation, and backend/frontend correlation logic.
- `crates/core/shell/src/shell/runtime/profiling.rs` - bounded shell, surface, and backend profiling collector.
- `crates/core/shell/src/shell/runtime/request.rs` - debug benchmark request handling and shell/runtime request routing.
- `crates/core/shell/src/shell/service.rs` - frontend event to `CoreRequest` conversion, including benchmark and debug/profiling actions.
- `crates/core/shell/src/shell/ipc.rs` - debug IPC commands and benchmark launch routing.

### Shipped Surface and Inspector Anchors
- `modules/frontend/debug-inspector/src/main.mesh` - debug inspector parent, benchmark payload normalization, and view switching.
- `modules/frontend/debug-inspector/src/components/benchmark-view.mesh` - compact benchmark rows and fixed canonical run controls.
- `modules/frontend/navigation-bar/src/main.mesh` - primary shipped shell surface for hover, keyboard traversal, and responsiveness proof.
- `modules/frontend/audio-popover/src/main.mesh` - shipped popover/control surface for surface open/close and pointer-driven update scenarios.

### Testing and Proof
- `crates/core/shell/src/shell/tests.rs` - shell-level profiling, benchmark, backend-correlation, and debug request tests.
- `crates/core/shell/src/shell/component/tests.rs` - real-surface component tests for debug inspector and shipped `.mesh` surfaces.
- `.planning/codebase/TESTING.md` - local test conventions, Nix-based cargo commands, and preference for focused behavior tests.
- `.planning/codebase/ARCHITECTURE.md` - shell/module/debug architecture and anti-patterns around service-specific Rust logic.
- `.planning/codebase/CONVENTIONS.md` - Rust, Luau, `.mesh`, and test naming/style conventions.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `DebugBenchmarkSnapshot` and related benchmark scenario types already provide stable scenario ids, labels, statuses, targets, metrics, and hints for Phase 18 proof.
- `Shell::build_debug_snapshot()` already derives benchmark rows from live rolling profiling snapshots, so baseline and post-change proof should use that contract instead of adding a new measurement store.
- `ProfilingSnapshot` already includes shell, surface, and backend summaries with stage-level timings that can identify the worst measured hotspot.
- `modules/frontend/debug-inspector/src/components/benchmark-view.mesh` already exposes canonical benchmark rows and run controls for the five fixed scenarios.
- Existing shell/component benchmark tests in `crates/core/shell/src/shell/tests.rs` and `crates/core/shell/src/shell/component/tests.rs` provide the regression harness for preserving the benchmark contract.

### Established Patterns
- Profiling is debug-only, explicitly toggled, and live/rolling rather than persisted.
- Benchmark rows are session-local and derived from `mesh.debug`; Phase 18 should not add trace persistence or benchmark history.
- Shipped-surface proof is preferred over synthetic-only proof, with deterministic fixtures acceptable in automated tests.
- Backend-specific behavior stays in Luau providers; Rust core records generic provider/stage timing and routes generic service state.
- New Rust functions should be targeted helpers near related runtime/debug/profiling code rather than large additions to shell orchestration.

### Integration Points
- Baseline and post-change measurements will connect through `crates/core/foundation/debug/src/lib.rs`, `crates/core/shell/src/shell/runtime/debug.rs`, and `crates/core/shell/src/shell/runtime/profiling.rs`.
- Optimization work may touch render invalidation, style/restyle, layout, paint, present/commit, redraw, shell input, or backend attribution paths depending on profiler output.
- Benchmark contract regression tests should remain in `crates/core/shell/src/shell/tests.rs`; real-surface UI proof should remain in `crates/core/shell/src/shell/component/tests.rs`.
- Inspector UI changes are not expected for this phase unless needed to preserve or expose the focused proof; broad inspector redesign is out of scope.

</code_context>

<specifics>
## Specific Ideas

- Start the phase by capturing a fresh baseline rather than relying on Phase 17 output that may be stale or absent.
- Use the largest absolute latency as the tie-breaker when measured hotspots are close.
- Treat a 10% or greater improvement on the selected metric as the threshold for Phase 18 success.
- Prefer a focused before/after proof for one optimized hotspot plus regression coverage, rather than making the phase a full benchmark-reporting project.

</specifics>

<deferred>
## Deferred Ideas

- Full before/after comparison for all five canonical scenarios.
- Strict no-regression performance gate across every benchmark scenario.
- Persistent benchmark history, trace export, or replayable profiling sessions.
- Broad visual/UI redesign while optimizing.
- Package/module manifest restructuring and module management work.

### Reviewed Todos (not folded)
- `Create unified package and module manifest phase` - visible in project state but not folded because package/module manifest structure, module management, icon pack installation, and interface declarations are outside Phase 18's targeted optimization scope.

</deferred>

---

*Phase: 18-Targeted Optimization Pass*
*Context gathered: 2026-05-09*
