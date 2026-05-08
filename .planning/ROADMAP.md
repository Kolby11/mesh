# Roadmap: MESH v1.3 Performance Instrumentation and Responsiveness

**Status:** Active milestone planning
**Phases:** 14-18
**Total Phases:** 5

## Overview

`v1.3` focuses on measurable shell responsiveness. The milestone adds a debug-only profiling mode with live rolling metrics, exposes those metrics through a `.mesh`-rendered inspector, defines canonical benchmark scenarios on real shipped interactions, and uses the measurements to drive a bounded optimization pass in the same milestone.

## Phases

### Phase 14: Profiling Data Model and Timing Hooks

**Goal:** Add a profiling runtime model and low-overhead timing hooks that measure real shell stages without changing normal user-facing behavior when profiling is off.
**Depends on:** Phase 13
**Requirements:** `PROF-02`, `PROF-03`, `TIME-01`, `TIME-03`

Planned work:

- Define profiling runtime/debug types that extend the existing debug snapshot model.
- Add timing hooks for input handling, script/runtime updates, tree build, style/restyle, layout, paint, present/commit, redraw counts, and total surface render time.
- Keep instrumentation disabled unless profiling mode is active, with bounded overhead when enabled.

### Phase 15: Backend and Surface Attribution

**Goal:** Attribute profiling data by surface/module and by backend provider/service stage so hotspots are actionable rather than aggregate-only.
**Depends on:** Phase 14
**Requirements:** `TIME-02`, `BACK-01`, `BACK-02`

Planned work:

- Add per-surface timing breakdowns that still roll up into shell-wide totals.
- Attribute backend timings by provider/service.
- Separate backend poll/update, command handling, and state publish/delivery stages.

### Phase 16: Debug-Only Profiling Mode and Live Inspector

**Goal:** Extend the existing debug overlay/debug path with profiling mode controls and a live `.mesh`-rendered inspector.
**Depends on:** Phase 14, Phase 15
**Requirements:** `PROF-01`, `INSP-01`, `INSP-02`, `INSP-03`

Planned work:

- Reuse existing debug toggle and state flow instead of creating a separate end-user mode system.
- Add profiling inspector views for overview, surfaces, backend services, and benchmark interactions.
- Build the inspector UI with normal frontend `.mesh` components so it exercises the standard UI stack.

### Phase 17: Canonical Benchmark Scenarios and Proof Flows

**Goal:** Define fixed benchmark scenarios on real shipped shell interactions so responsiveness claims are repeatable and comparable.
**Depends on:** Phase 16
**Requirements:** `BACK-03`, `BENCH-01`, `BENCH-02`, `BENCH-03`, `BENCH-04`, `BENCH-05`

Planned work:

- Provide repeatable benchmark/proof flows for hover latency, surface open/close, slider drag, keyboard traversal, and backend-driven state updates.
- Use shipped surfaces such as `navigation-bar` and `audio-popover` as concrete anchors while keeping profiling shell-wide.
- Ensure the benchmark views show both backend attribution and visible frontend cost where applicable.

### Phase 18: Targeted Optimization Pass

**Goal:** Use the profiler and benchmark suite to land a bounded set of high-impact responsiveness fixes and prove improvement.
**Depends on:** Phase 17
**Requirements:** `OPT-01`, `OPT-02`, `OPT-03`

Planned work:

- Identify the worst measured hotspots rather than optimizing speculative paths.
- Prioritize render invalidation, restyle/layout churn, redraw volume, and input-to-visible-response latency.
- Optimize backend paths only when the profiler shows material visible impact.
- Demonstrate at least one measurable before/after improvement on the canonical benchmark suite.

## Milestone Boundaries

### Included

- Debug-only profiling runtime and inspector behavior
- Shell-wide, per-surface, and per-backend-provider/stage timing visibility
- Benchmark proof flows on real interaction paths
- Bounded optimization work driven by the new measurements

### Excluded

- GPU renderer rewrite or renderer model replacement
- Backend architecture redesign not justified by benchmark results
- Full profiling trace persistence, replay, or external tracing integration
- Broad UI redesign beyond the profiling inspector and proof-surface adjustments

## Archived Milestones

- `v1.2` Rendering System Upgrade — shipped 2026-05-08. Archive: `.planning/milestones/v1.2-ROADMAP.md`
- `v1.1` Backend Plugin MVP — shipped 2026-05-05. Archive: `.planning/milestones/v1.1-ROADMAP.md`

## Backlog and Carryover

- Deferred v1.1 validation metadata cleanup remains backlog work outside `v1.3`.
- Phase 11 human/UAT verification debt remains visible but separate from the `v1.3` product goal.
- The pending unified package/module manifest phase idea remains future work unless reprioritized independently.

---
*Roadmap updated: 2026-05-08 after starting milestone v1.3*
