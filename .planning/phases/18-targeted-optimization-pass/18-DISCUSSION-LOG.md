# Phase 18: Targeted Optimization Pass - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md - this log preserves the alternatives considered.

**Date:** 2026-05-09
**Phase:** 18-targeted-optimization-pass
**Areas discussed:** Hotspot Selection, Proof Standard, Regression Guardrails

---

## Hotspot Selection

| Option | Description | Selected |
|--------|-------------|----------|
| Worst measured | Use the profiler to pick the slowest canonical benchmark or stage once Phase 18 starts. | ✓ |
| Safest fix | Prefer a high-confidence, low-risk improvement even if it is not the absolute worst hotspot. | |
| Specific path | Prioritize a named interaction first, then use profiling to validate it. | |

**User's choice:** Worst measured.
**Notes:** If two hotspots are close, prefer the one with the largest absolute latency.

---

## Proof Standard

| Option | Description | Selected |
|--------|-------------|----------|
| Focused before/after | Show one optimized hotspot with clear before/after benchmark numbers, plus focused regression tests. | ✓ |
| Full suite comparison | Capture before/after numbers for all five canonical benchmark scenarios, even if only one is optimized. | |
| Strict no-regression suite | Require the optimized scenario to improve and all other benchmark scenarios to stay neutral or better. | |

**User's choice:** Focused before/after.
**Notes:** Capture a fresh baseline at the start of Phase 18. The selected metric must improve by at least 10%.

---

## Regression Guardrails

| Option | Description | Selected |
|--------|-------------|----------|
| All guardrails | Profiling-off behavior, visual output, benchmark contracts, and backend/service semantics must remain unchanged. | ✓ |
| Runtime only | Preserve profiling-off behavior and benchmark contracts; allow small visual/UI changes if justified. | |
| Visual only | Preserve visible behavior and benchmark contracts; allow runtime internals to change more freely. | |

**User's choice:** All guardrails.
**Notes:** Phase 18 should avoid UI changes, benchmark-contract changes, profiling-off behavior changes, and backend/service semantic changes.

---

## the agent's Discretion

- Choose the exact benchmark command sequence and measurement format.
- Choose the concrete optimization after inspecting fresh profiler output.
- Choose focused regression test selectors that prove the optimized path and protect the benchmark/profiling contract.

## Deferred Ideas

- Full before/after comparison for all five canonical benchmark scenarios.
- Strict no-regression performance gate across every benchmark scenario.
- Persistent benchmark history, trace export, or replayable profiling sessions.
- Broad visual/UI redesign while optimizing.
- Package/module manifest restructuring and module management work.
