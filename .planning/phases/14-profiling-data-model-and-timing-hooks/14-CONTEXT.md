# Phase 14: Profiling Data Model and Timing Hooks - Context

**Gathered:** 2026-05-08
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase adds the first profiling runtime model and low-overhead timing hooks for MESH shell internals. It defines how the shell records shell-wide and per-surface timing data for the required top-level stages, while keeping profiling disabled by default, reachable only through the existing debug path, and bounded enough to remain usable during live interaction.

This phase covers:
- Extending the existing debug snapshot model with profiling-specific state and snapshot types.
- Recording shell-wide timing buckets for input handling, runtime update handling, tree build, style/restyle, layout, paint, present/commit, redraw count, and total surface render time.
- Recording per-surface timing data for every rendered shell surface, with surface id as the canonical unit and optional module/component identity when known.
- Defining a live rolling retention model with aggregates plus bounded recent samples.
- Wiring profiling collection so it is independently toggled inside the existing debug path and can continue collecting even while the overlay is hidden.

This phase does not add:
- Deep nested trace trees or capture/replay behavior.
- Per-backend-provider or per-service-stage attribution beyond lightweight optional tags needed to support later phases.
- The profiling inspector UI itself, new debug tabs/views, or benchmark scenario UX; those belong to later phases.

</domain>

<decisions>
## Implementation Decisions

### Metrics Shape
- **D-01:** Phase 14 stores both rolling aggregates and a bounded recent-sample ring; it is not aggregate-only and not event-trace-first.
- **D-02:** Recent samples use a fixed sample count per metric bucket rather than a time-window retention policy.
- **D-03:** Recent samples use a split model: compact timing records by default, with a small set of optional context fields when available.
- **D-04:** The optional context fields in Phase 14 stay practical and lightweight: stage, order/timestamp, duration, surface id, redraw count, stable surface/module identity when known, and a small trigger-kind tag such as `input`, `service_update`, or `rebuild`.

### Stage Boundaries
- **D-05:** Phase 14 uses strict top-level shell profiling stages only: input handling, runtime update handling, tree build, style/restyle, layout, paint, present/commit, redraw count, and total surface render time.
- **D-06:** `total surface render time` is a first-class explicit span, not a derived-only number reconstructed later from sub-stages.
- **D-07:** Shell-wide stages and per-surface stages are both recorded; shell-global work is not forced into a surface bucket.
- **D-08:** The `runtime update` boundary wraps shell-side processing of state/input/update work before per-surface render stages.
- **D-09:** Redraw count is a first-class profiled metric in the snapshot model rather than sample metadata only.

### Surface Accounting
- **D-10:** Every shell surface the runtime presents counts as a first-class profiled surface unit, including transient or popover-style surfaces when they actually render.
- **D-11:** Hidden surfaces do not appear in active profiling output unless they perform real update/render work.
- **D-12:** Surface id is the canonical profiling key, with stable module/component identity attached as optional context when known.
- **D-13:** Recent sample rings keep repeated surface render samples raw until the fixed count fills; Phase 14 does not coalesce same-cycle render bursts.
- **D-14:** Shell-wide work that does not lead to a visible surface render is still kept in profiling data rather than discarded.

### Collection Trigger
- **D-15:** When profiling is enabled, collection runs continuously until profiling is disabled; it is not limited to moments when a profiling tab is actively visible.
- **D-16:** Profiling is a separate explicit toggle inside the existing debug path; turning on the general debug overlay alone does not automatically start profiling.
- **D-17:** Enabling profiling resets prior profiling session data so each session starts from a clean state.
- **D-18:** Profiling collection continues even if the debug overlay is later hidden; overlay visibility and profiling collection state are independent.

### the agent's Discretion
- Planner/researcher may choose the exact Rust type layout for aggregate buckets, recent-sample rings, and compact-vs-optional sample context as long as the fixed-count rolling model stays intact.
- Planner/researcher may choose the exact stable identifier fields and enum/string representation for trigger-kind tags, as long as they stay small and practical rather than becoming deep trace metadata.
- Planner/researcher may choose the exact instrumentation helper boundaries in the shell/render loop, as long as the locked top-level stage model and explicit total-surface span are preserved.
- Planner/researcher may choose the exact reset semantics implementation path for profiling enable/disable, as long as a newly enabled session begins clean and profiling state remains independent from overlay visibility.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone and Phase Scope
- `.planning/PROJECT.md` - `v1.3` milestone framing, including the debug-only profiling rule and live/rolling profiler boundary.
- `.planning/REQUIREMENTS.md` - `PROF-02`, `PROF-03`, `TIME-01`, and `TIME-03`, plus the out-of-scope boundary against full tracing and architecture rewrites.
- `.planning/ROADMAP.md` - Phase 14 goal, required stage list, and dependency chain into later profiling attribution and inspector phases.
- `.planning/STATE.md` - carried-forward project decisions, including the locked `v1.3` profiling constraints.

### Existing Debug Runtime and Entry Path
- `crates/core/foundation/debug/src/lib.rs` - current `DebugSnapshot`, `DebugOverlayState`, and `DebugTab` model that Phase 14 must extend rather than replace.
- `crates/core/shell/src/shell/runtime/debug.rs` - `Shell::build_debug_snapshot()` and the current point-in-time debug snapshot assembly path.
- `crates/core/shell/src/shell/runtime/request.rs` - debug overlay request handling and the current toggle/cycle-tab state flow.
- `crates/core/shell/src/shell/types.rs` - `CoreRequest` and shell input/runtime request boundaries.
- `crates/core/shell/src/shell/ipc.rs` - existing IPC commands for debug overlay control; later profiling toggles should stay aligned with this debug path.
- `crates/tools/cli/src/main.rs` - current CLI entrypoint for toggling the running shell debug overlay.

### Render Loop and Surface Lifecycle Integration
- `crates/core/shell/src/shell/runtime/render.rs` - current render/present loop and where debug snapshots are built and painted.
- `crates/core/shell/src/shell/mod.rs` - shell orchestration layer, event flow, and the current `DebugOverlay` ownership path.
- `crates/core/shell/src/shell/component.rs` - `FrontendSurfaceComponent` as the durable surface runtime unit likely to anchor per-surface timing and redraw tracking.
- `crates/core/shell/src/shell/types.rs` - `ShellComponent` trait surface lifecycle boundaries relevant to stage timing.

### Debug Overlay Rendering and Existing Proof Patterns
- `crates/core/ui/render/src/surface/debug_overlay.rs` - current overlay renderer, tab model, and panel paint path that later phases will extend from the profiling data model defined here.
- `crates/core/shell/src/shell/tests.rs` - shell-level tests around debug IPC/debug snapshot behavior and a likely home for profiling-disabled/profiling-enabled runtime tests.
- `docs/llm-context.md` - crate map and current shell/debug entry points.

### Prior Phase Context That Must Carry Forward
- `.planning/phases/11-keyboard-navigation-and-shortcuts/11-CONTEXT.md` - stable shell-owned interaction state and shortcut/debug-path conventions that should not be bypassed.
- `.planning/phases/12-theme-animation-tokens-and-css-animations/12-CONTEXT.md` - active-animation/dirtiness model and the project’s existing stance against unnecessary redraw churn.
- `.planning/phases/13-navigation-bar-rendering-proof/13-CONTEXT.md` - proof-surface context for later benchmark phases and the rule that shipped real surfaces, not throwaway fixtures, should anchor milestone proof.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `mesh_core_debug::DebugSnapshot` and `DebugOverlayState` already provide the crate boundary for debug-only runtime state; Phase 14 should extend these types with profiling data instead of creating a parallel diagnostics subsystem.
- `Shell::build_debug_snapshot()` already centralizes debug snapshot construction, making it the natural aggregation point for profiling rollups once timing data has been recorded elsewhere in the runtime.
- `runtime/render.rs` already gates debug snapshot building and overlay painting behind `self.debug.enabled`, which is the cleanest existing entry path for a profiling toggle that stays debug-only.
- `DebugOverlay` already paints a right-side panel from a snapshot model, so later phases can build on a known renderer contract once Phase 14 defines the profiling data shape.
- `ShellComponent`/`FrontendSurfaceComponent` provide stable per-surface runtime units and surface ids, which are the right granularity for per-surface timing and explicit total render spans.

### Established Patterns
- The shell prefers typed, shell-owned runtime state over transient IDs or ad hoc frontend-defined profiling logic; Phase 14 should keep timing ownership in Rust shell/runtime code.
- The current debug path is request-driven through `CoreRequest`, keyboard shortcuts, IPC, and overlay state, so profiling control should follow that same request/state model rather than inventing a separate config/settings path.
- The project already distinguishes shell-wide orchestration work from per-surface work, which supports the locked decision to record both global and surface-local timing rather than forcing everything into one bucket.
- Existing milestone decisions emphasize bounded, practical observability rather than browser-engine or trace-system scope, so the profiling model should stay compact and explicit.
- Real shell surfaces already have visible/hidden lifecycle state, making “active only when actual work occurs” a natural rule for profiling snapshots.

### Integration Points
- Extend `crates/core/foundation/debug/src/lib.rs` with profiling snapshot/data structures, profiling enable state, and any new debug-tab or toggle primitives needed later.
- Add timing storage and aggregation ownership in the shell runtime near `crates/core/shell/src/shell/mod.rs`, `crates/core/shell/src/shell/runtime/debug.rs`, and `crates/core/shell/src/shell/runtime/render.rs`.
- Add explicit instrumentation hooks around shell update handling, per-surface build/layout/paint/present work, and redraw accounting in the runtime/render path.
- Add profiling-control requests next to existing debug requests in `crates/core/shell/src/shell/types.rs`, `crates/core/shell/src/shell/runtime/request.rs`, and `crates/core/shell/src/shell/ipc.rs`.
- Add regression tests for profiling-disabled silence, enabled snapshot rollup shape, and debug-path toggle behavior in `crates/core/shell/src/shell/tests.rs`.

</code_context>

<specifics>
## Specific Ideas

- The first profiler should feel like a live rolling shell instrument panel, not a trace recorder.
- Fixed-count recent rings are preferred over time-window retention because deterministic bounded memory is more important than wall-clock semantics in the first release.
- Practical trigger-kind tags such as `input`, `service_update`, and `rebuild` are useful, but deep request/provider attribution should wait for later phases.
- Surface id remains the canonical runtime unit even when later benchmark work focuses on shipped surfaces such as `navigation-bar` or `audio-popover`.

</specifics>

<deferred>
## Deferred Ideas

- Deep nested trace trees, capture/replay workflows, and long-lived profiling persistence are deferred beyond Phase 14.
- Rich attribution such as backend provider/stage breakdowns beyond lightweight optional tags is deferred to Phase 15 and later milestone phases.
- Profiling inspector UI and benchmark-facing interaction views are deferred to Phases 16 and 17.

### Reviewed Todos (not folded)
- `Create unified package and module manifest phase` - reviewed during Phase 14 cross-reference, but not folded because it is a separate planning/domain effort and only matched this phase weakly by the generic word `phase`.

</deferred>

---

*Phase: 14-Profiling Data Model and Timing Hooks*
*Context gathered: 2026-05-08*
