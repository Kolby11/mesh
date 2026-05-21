# Phase 47: Taffy Layout Adapter Integration - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-18T21:23:26+02:00
**Phase:** 47-Taffy Layout Adapter Integration
**Areas discussed:** Taffy rollout boundary, Unsupported layout fallback, Todo scope

---

## Taffy Rollout Boundary

| Option | Description | Selected |
|--------|-------------|----------|
| Parity/proof-only | Keep Taffy as proof evidence or a sidecar comparison path. | |
| Opt-in runtime path | Add a Taffy path behind an explicit runtime switch while keeping the old layout path authoritative. | |
| Strict replacement | Replace the relevant current layout code with Taffy for in-scope layout behavior. | ✓ |

**User's choice:** Strict replacement.
**Notes:** User stated that Phase 47 should not be concerned with backward compatibility and should replace the relevant code using Taffy.

---

## Unsupported Layout Fallback

| Option | Description | Selected |
|--------|-------------|----------|
| Strict replacement, diagnostics for gaps | Use Taffy as the only production layout path for in-scope cases; unsupported cases are implementation gaps to fix, not runtime fallbacks. | ✓ |
| Replace supported cases, fallback only for unsupported cases | Taffy becomes authoritative for supported layout behavior; old layout remains as a non-default emergency path for unsupported cases. | |
| Replace shipped surfaces first | Taffy becomes authoritative only for shipped navigation/audio surfaces in Phase 47; broader layout replacement follows after parity is proven. | |

**User's choice:** Strict replacement, diagnostics for gaps.
**Notes:** This reconciles LAYT-03 by making unsupported cases visible through diagnostics, failed parity coverage, or blocker records rather than silently preserving old-engine output.

---

## Todo Scope

| Option | Description | Selected |
|--------|-------------|----------|
| Do not fold | Keep audio popover transition timing deferred to the v1.10 animation/motion milestone. | ✓ |
| Fold in | Treat popover transition delay as layout-adjacent Phase 47 scope. | |
| Review later | Mention it as reviewed but leave the final choice to planning. | |

**User's choice:** Inferred from existing milestone scope and Phase 47 decision.
**Notes:** The todo matched loosely on render/audio, but its content is surface transition polish, not Taffy layout replacement. It is recorded as reviewed but not folded into Phase 47.

---

## the agent's Discretion

- Exact Taffy module placement, dependency-feature movement, adapter type names, and test file organization remain planner discretion.
- Planner must preserve the core user decision: Taffy replaces relevant in-scope layout code; unsupported cases become diagnostics or blockers rather than hidden old-engine fallbacks.

## Deferred Ideas

- Audio Popover Transition Delay Polish — keep for v1.10 animation and motion-fidelity work.
