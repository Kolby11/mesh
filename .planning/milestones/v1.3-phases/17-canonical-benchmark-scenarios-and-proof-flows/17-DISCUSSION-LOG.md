# Phase 17: Canonical Benchmark Scenarios and Proof Flows - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md - this log preserves the alternatives considered.

**Date:** 2026-05-09
**Phase:** 17-canonical-benchmark-scenarios-and-proof-flows
**Areas discussed:** Benchmark launch model, canonical scenario anchors, result contract, proof strategy, pending todo handling

---

## Benchmark Launch Model

| Option | Description | Selected |
|--------|-------------|----------|
| Explicit debug-inspector actions | Add runnable controls or rows inside the existing benchmark inspector view. Keeps benchmark work debug-only and visible. | yes |
| Automatic collection | Run benchmark flows automatically when profiling or the inspector opens. Risks surprising behavior and noisy data. | |
| External CLI-only harness | Keep UI scaffold-only and expose scenarios only through command-line tests. Weaker match for the Phase 16 inspector scaffold. | |

**User's choice:** Fallback default selected because interactive input was unavailable.
**Notes:** The selected path preserves the Phase 16 boundary: the inspector already has a benchmark view, and Phase 17 should make it runnable without creating a separate profiling entrypoint.

---

## Canonical Scenario Anchors

| Option | Description | Selected |
|--------|-------------|----------|
| Shipped surfaces as primary anchors | Use navigation bar, audio popover, and debug inspector as the visible proof targets. | yes |
| Synthetic benchmark fixture first | Build isolated benchmark-only widgets for determinism. Useful in tests but too synthetic for the user-facing suite. | |
| Broad shell-wide sampling only | Report aggregate live profiler data without scenario-specific anchors. Fails the repeatability goal. | |

**User's choice:** Fallback default selected because interactive input was unavailable.
**Notes:** This carries forward the Phase 13 real-surface proof pattern and the Phase 17 roadmap text naming `navigation-bar` and `audio-popover`.

---

## Result Contract

| Option | Description | Selected |
|--------|-------------|----------|
| Live latest-run summaries | Store session-scoped scenario results with stable ids and shell/surface/backend timing summaries. | yes |
| Persistent benchmark history | Save long-lived run history for later comparison. Out of scope for the first live/rolling profiler. | |
| Text-only scaffold | Keep category descriptions without structured result data. Insufficient for repeatable proof flows. | |

**User's choice:** Fallback default selected because interactive input was unavailable.
**Notes:** Phase 17 should prepare comparable scenario results for Phase 18, but persistence and trace/replay remain deferred.

---

## Proof Strategy

| Option | Description | Selected |
|--------|-------------|----------|
| Existing shell/component tests | Extend Rust shell profiling tests and real-surface `.mesh` component tests. | yes |
| New compositor E2E harness | Add Wayland/compositor-level automation. Higher scope and not present in current test architecture. | |
| Manual verification only | Rely on human benchmark checks. Too weak for canonical repeatable scenarios. | |

**User's choice:** Fallback default selected because interactive input was unavailable.
**Notes:** The codebase already has focused profiling tests in `crates/core/shell/src/shell/tests.rs` and debug-inspector real-surface tests in `crates/core/shell/src/shell/component/tests.rs`.

---

## Pending Todo Handling

| Option | Description | Selected |
|--------|-------------|----------|
| Do not fold | Keep package/module manifest work deferred as a separate future phase. | yes |
| Fold it | Include manifest restructuring in Phase 17 benchmark planning. | |

**User's choice:** Fallback default selected because interactive input was unavailable.
**Notes:** Prior Phase 14 context also reviewed this todo and did not fold it because it is a separate planning/domain effort.

---

## the agent's Discretion

- Exact Rust type names and request/API shape for scenario definitions and latest-run results.
- Exact compact UI layout for runnable benchmark rows/controls.
- Deterministic fixture mechanics for tests, as long as the user-facing benchmark semantics remain shipped-surface based.

## Deferred Ideas

- Persist benchmark history, trace files, exports, or replayable sessions.
- Run the Phase 18 targeted optimization pass or claim before/after improvements.
- Add a compositor-level E2E harness.
- Redesign shipped surfaces beyond what is needed to exercise benchmark scenarios.
- Fold package/module manifest restructuring into this phase.
