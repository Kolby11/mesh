# Phase 17: Canonical Benchmark Scenarios and Proof Flows - Context

**Gathered:** 2026-05-09
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase turns the Phase 16 benchmark inspector scaffold into repeatable benchmark/proof flows for real shipped shell interactions. It defines canonical scenarios for hover, surface open/close, pointer-driven updates, keyboard traversal, and backend-driven state updates, then wires those scenarios to the live debug-only profiling data so responsiveness claims are repeatable and comparable.

This phase covers:
- Adding explicit benchmark launch/proof flows inside the existing debug inspector benchmark view.
- Anchoring benchmark scenarios on shipped surfaces such as `@mesh/navigation-bar`, `@mesh/audio-popover`, and `@mesh/debug-inspector` rather than isolated synthetic widgets only.
- Producing stable scenario identities and result summaries that correlate shell-wide, per-surface, and backend-stage profiling data.
- Proving `BACK-03` and `BENCH-01` through `BENCH-05` with automated shell/component tests where possible.
- Keeping benchmark results live/session-scoped and debug-only.

This phase does not add:
- Trace capture, replay, export, or long-lived benchmark history.
- The Phase 18 optimization pass or before/after improvement claims.
- A compositor-level E2E test harness.
- A broad redesign of the debug inspector, navigation bar, audio popover, backend runtime, or renderer architecture.
- Package/module manifest restructuring; that remains a separate deferred planning item.

</domain>

<decisions>
## Implementation Decisions

### Benchmark Launch Model
- **D-01:** Benchmarks should be explicit debug-inspector actions in the benchmark/interaction view, not automatic runs triggered merely by opening the inspector or enabling profiling.
- **D-02:** Each canonical scenario should have a stable scenario id, label, target surface/module, and run status so results remain comparable across sessions and later Phase 18 optimization work.
- **D-03:** The benchmark view should evolve from scaffold-only cards into runnable scenario rows or controls, while staying inside the existing debug inspector surface and existing debug-only profiling path.
- **D-04:** Profiling must remain independently toggled through the debug path; benchmark controls may guide or require profiling to be active, but they must not create a new profiling entrypoint.

### Canonical Scenario Anchors
- **D-05:** Use shipped surfaces as the primary proof anchors: `@mesh/navigation-bar` for hover and keyboard traversal, `@mesh/audio-popover` or the navigation-bar audio affordance for pointer/slider-driven updates, and the debug inspector itself for benchmark presentation.
- **D-06:** Surface open/close should use an existing shell surface transition such as `shell.toggle-surface` / `shell.hide-surface` rather than inventing a benchmark-only surface lifecycle path.
- **D-07:** Backend-driven update should correlate the existing `mesh.audio` provider stages with the frontend surface cost caused by the resulting service update.
- **D-08:** Synthetic helper data is acceptable inside automated tests only when needed for determinism, but the user-facing benchmark definitions should remain framed around real shipped shell interactions.

### Result Contract
- **D-09:** Benchmark results should summarize the same live rolling profiling buckets already exposed by `mesh.debug`: input handling, runtime update handling, style/restyle, layout, paint, present/commit, redraw count, total surface render, and backend poll/update, command handling, and state publish/delivery where applicable.
- **D-10:** A benchmark result should include enough identity to explain what was measured: scenario id, target surface/module, trigger kind, latest run status, and relevant shell/surface/backend timing summaries.
- **D-11:** Phase 17 should not persist benchmark history. It may keep current-session/latest-run results in debug state or inspector-local state as needed for live display.
- **D-12:** Empty, skipped, or unavailable scenario states should be visible and non-fatal, matching the Phase 16 sparse-state inspector rule.

### Proof Strategy
- **D-13:** Automated proof should favor existing Rust shell/component tests over a new E2E harness.
- **D-14:** Tests should prove that each scenario is discoverable, has a stable id/category, and maps to the expected profiling buckets or backend attribution fields.
- **D-15:** Real-surface component tests should cover the debug inspector benchmark view after it becomes runnable, including empty/unavailable states and at least one populated result state.
- **D-16:** Backend-driven proof should assert both backend-stage attribution and resulting frontend/surface timing correlation, satisfying `BACK-03` without adding service-specific Rust business logic.

### the agent's Discretion
- Planner/researcher may choose the exact Rust type names and request/API shape for benchmark scenario definitions and latest-run results.
- Planner/researcher may choose whether benchmark execution is modeled as shell-owned requests, debug-service state, inspector-local handlers, or a small combination, as long as the behavior stays debug-only and uses the existing `mesh.debug` path.
- Planner/researcher may choose the exact UI layout for scenario rows/controls in the benchmark view, as long as it remains compact, stable with sparse data, and consistent with the current debug inspector.
- Planner/researcher may choose deterministic test fixture mechanics for backend-driven and pointer-driven scenarios, as long as user-facing benchmark semantics remain grounded in shipped interactions.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone and Phase Scope
- `.planning/PROJECT.md` - v1.3 milestone framing, including debug-only profiling, live inspector, fixed benchmark suite, and bounded optimization sequence.
- `.planning/REQUIREMENTS.md` - `BACK-03`, `BENCH-01`, `BENCH-02`, `BENCH-03`, `BENCH-04`, and `BENCH-05`, plus Phase 18 optimization boundaries.
- `.planning/ROADMAP.md` - Phase 17 goal, dependency on Phase 16, shipped-surface anchors, and planned benchmark/proof-flow work.
- `.planning/STATE.md` - carried-forward decisions and the pending package/module manifest todo that remains deferred.

### Prior Phase Context That Must Carry Forward
- `.planning/phases/14-profiling-data-model-and-timing-hooks/14-CONTEXT.md` - locked profiling data model, debug-only activation, fixed-count rolling samples, stage list, and live/session-scoped retention.
- `.planning/phases/15-backend-and-surface-attribution/15-VERIFICATION.md` - proof that backend provider/stage attribution and comparable per-surface timing snapshots exist.
- `.planning/phases/16-debug-only-profiling-mode-and-live-inspector/16-CONTEXT.md` - locked inspector host model, benchmark scaffold boundary, and requirement that Phase 17 owns runnable benchmark flows.
- `.planning/phases/13-navigation-bar-rendering-proof/13-CONTEXT.md` - real-surface proof philosophy and navigation-bar constraints relevant to benchmark anchors.

### Debug, Profiling, and Inspector Code
- `crates/core/foundation/debug/src/lib.rs` - `DebugSnapshot`, `DebugInspectorView::Benchmark`, `ProfilingSnapshot`, shell/surface/backend profiling summary types, and profiling stage labels.
- `crates/core/shell/src/shell/runtime/debug.rs` - debug service payload assembly and JSON shape consumed by the `.mesh` inspector.
- `crates/core/shell/src/shell/runtime/profiling.rs` - bounded shell, surface, and backend profiling collector.
- `crates/core/shell/src/shell/ipc.rs` - existing debug IPC commands, including `shell:debug_overlay`, `shell:debug_profiling`, and `shell:debug_cycle_tab`.
- `crates/core/shell/src/shell/types.rs` - `CoreRequest` boundary for shell/debug/runtime requests.
- `crates/core/shell/src/shell/service.rs` - frontend event to `CoreRequest` conversion, including `shell.toggle-debug-profiling`.

### Shipped Surface Anchors
- `modules/frontend/debug-inspector/src/main.mesh` - current debug inspector shell surface and profiling/benchmark view integration.
- `modules/frontend/debug-inspector/src/components/benchmark-view.mesh` - Phase 16 scaffold that Phase 17 should turn into repeatable benchmark flows.
- `modules/frontend/debug-inspector/module.json` - core-shipped inspector surface capabilities, sizing, and debug-only placement.
- `modules/frontend/navigation-bar/src/main.mesh` - primary shipped shell surface for hover, keyboard traversal, and surface responsiveness proof.
- `modules/frontend/audio-popover/src/main.mesh` - shipped popover/control surface candidate for surface open/close and pointer-driven update scenarios.

### Testing and Proof
- `crates/core/shell/src/shell/tests.rs` - shell-level profiling tests for debug toggles, profiling-disabled silence, per-surface rollups, and backend stage attribution.
- `crates/core/shell/src/shell/component/tests.rs` - real-surface component tests, including current debug-inspector benchmark scaffold proof.
- `.planning/codebase/TESTING.md` - local test conventions, Nix-based cargo commands, and preference for focused behavior tests.
- `.planning/codebase/ARCHITECTURE.md` - shell/module/debug architecture and anti-patterns around service-specific Rust logic.
- `.planning/codebase/INTEGRATIONS.md` - local IPC, Wayland, audio provider, and capability integration context.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `DebugInspectorView::Benchmark` already exists in `crates/core/foundation/debug/src/lib.rs`, so Phase 17 can extend the existing view rather than adding a new debug tab model.
- `modules/frontend/debug-inspector/src/components/benchmark-view.mesh` already lists the five canonical categories required by Phase 17.
- `modules/frontend/debug-inspector/src/main.mesh` already consumes `mesh.debug` profiling payloads and switches among overview, surfaces, backend services, and benchmark views.
- `ProfilingSnapshot` already contains shell, surface, and backend summaries, including recent samples, stage totals, redraw counts, and backend provider/stage data.
- `crates/core/shell/src/shell/component/tests.rs` already has real-surface tests for the debug inspector and benchmark scaffold.

### Established Patterns
- Profiling is debug-only, explicitly toggled, and live/rolling rather than persisted.
- The shell owns profiling data and exposes it through `mesh.debug`; `.mesh` UI consumes that data like a normal frontend module.
- Real shell proof work should use shipped surfaces and focused shell/component tests rather than one-off native diagnostics UI.
- Backend-specific behavior stays in Luau providers; Rust core should only record generic provider/stage timing and route service state.

### Integration Points
- Benchmark state and requests will likely connect through `crates/core/foundation/debug/src/lib.rs`, `crates/core/shell/src/shell/runtime/debug.rs`, `crates/core/shell/src/shell/runtime/profiling.rs`, and `crates/core/shell/src/shell/types.rs`.
- Inspector UI changes will touch `modules/frontend/debug-inspector/src/main.mesh` and `modules/frontend/debug-inspector/src/components/benchmark-view.mesh`.
- Shipped interaction anchors will involve `modules/frontend/navigation-bar/src/main.mesh` and `modules/frontend/audio-popover/src/main.mesh`, with tests in `crates/core/shell/src/shell/component/tests.rs`.
- Backend-driven correlation should reuse the existing backend attribution seams proven in Phase 15 instead of adding service-specific shell branches.

</code_context>

<specifics>
## Specific Ideas

- Treat the benchmark view as a compact control/result panel inside the right-side debug inspector.
- Keep every benchmark scenario explicit and named so Phase 18 can compare optimization results against the same suite.
- Let live UI results remain session-scoped; Phase 18 can use the same scenario ids for before/after proof without Phase 17 adding persistence.
- Prefer deterministic automated proof for the scenario contract, while keeping user-facing descriptions anchored to real shell interactions.

</specifics>

<deferred>
## Deferred Ideas

- Persist benchmark history, trace files, exports, or replayable sessions.
- Run the Phase 18 targeted optimization pass or claim before/after improvements.
- Add a compositor-level E2E harness.
- Redesign shipped surfaces beyond what is needed to exercise benchmark scenarios.

### Reviewed Todos (not folded)
- `Create unified package and module manifest phase` - reviewed during Phase 17 cross-reference, but not folded because package/module manifest structure, module management, icon pack installation, and interface declarations are a separate future phase outside benchmark/proof-flow scope.

</deferred>

---

*Phase: 17-Canonical Benchmark Scenarios and Proof Flows*
*Context gathered: 2026-05-09*
