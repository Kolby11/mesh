# Phase 31: Smoothness Proof and CPU Render Tuning - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-12
**Phase:** 31-smoothness-proof-and-cpu-render-tuning
**Areas discussed:** Smoothness acceptance proof, Tuning posture, Correctness guardrails

---

## Smoothness Acceptance Proof

| Option | Description | Selected |
|--------|-------------|----------|
| Mixed proof | Require canonical benchmark evidence plus focused manual UAT notes for shipped surfaces. | ✓ |
| Benchmark-led | Accept if canonical profiling metrics improve, with visual checks only as smoke tests. | |
| Manual UAT-led | Accept only after hands-on visual confirmation, using metrics as supporting evidence. | |

**User's choice:** Mixed proof.
**Notes:** Phase 31 should prove visible smoothness, not just improved internal counters.

---

## Tuning Posture

| Option | Description | Selected |
|--------|-------------|----------|
| Conservative | Tune thresholds and clear/background/cache heuristics only where existing proof shows a win; avoid structural rewrites. | ✓ |
| Moderate | Allow small renderer behavior changes if they improve shipped-surface smoothness and have focused regression tests. | |
| Aggressive | Pursue larger heuristic changes during Phase 31, accepting more implementation risk to chase smoothness. | |

**User's choice:** Conservative.
**Notes:** The planner should treat Phases 27-30 as the architecture to tune rather than redesigning it.

---

## Correctness Guardrails

| Option | Description | Selected |
|--------|-------------|----------|
| Strict | Every tuning change needs focused tests or UAT notes showing visuals/interactions remain unchanged apart from smoother rendering. | ✓ |
| Targeted | Require strict proof only on the five canonical scenarios; other areas get smoke coverage. | |
| Pragmatic | Allow minor visual differences if smoothness clearly improves and the differences are documented. | |

**User's choice:** Strict.
**Notes:** Conservative fallback is preferred when correctness cannot be proven cheaply.

---

## the agent's Discretion

- Exact threshold values, benchmark comparison layout, and UAT record format are left to planning and execution.
- Plan decomposition is left to the planner as long as all Phase 31 requirements are covered.

## Deferred Ideas

- `2026-05-08-create-unified-package-and-module-manifest-phase.md` remains deferred as unrelated backlog work.
