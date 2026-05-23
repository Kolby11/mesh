# Phase 56: Animation And Transition Paint Integration - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-23T09:22:33+02:00
**Phase:** 56-animation-and-transition-paint-integration
**Areas discussed:** Paint-only buckets, Animated bounds, Surface transitions

---

## Paint-only buckets

| Option | Description | Selected |
|--------|-------------|----------|
| Conservative classification | Start from current `AnimatedVisualStyle` / `TransitionProperties`, classify paint-only vs layout-affecting, and keep unknowns on relayout or diagnostics. | ✓ |
| Aggressive paint-only routing | Treat most visual properties as repaint-only immediately, widening tests after implementation. | |
| Defer classification | Leave current relayout behavior mostly intact and only add compatibility tests. | |

**User's choice:** Smart fallback selected conservative classification because interactive choice was unavailable.
**Notes:** This aligns with prior phases: MESH owns style/layout/animation/damage and unsupported behavior should diagnose instead of silently pretending support.

---

## Animated bounds

| Option | Description | Selected |
|--------|-------------|----------|
| Current-style bounds | Compute visual bounds from the current animated style and damage previous plus current bounds. | ✓ |
| Target-style bounds only | Use the final style target for bounds and rely on full repaint fallback for motion. | |
| Phase 57 deferral | Defer most animated bounds behavior to the broader stacking/damage phase. | |

**User's choice:** Smart fallback selected current-style bounds.
**Notes:** Phase 56 owns ANIM-03, so animated visual bounds and damage need focused proof even though broad stacking/damage policy remains Phase 57.

---

## Surface transitions

| Option | Description | Selected |
|--------|-------------|----------|
| Fold audio popover polish | Include Phase 31 audio popover transition-delay debt as bounded Phase 56 proof. | ✓ |
| Keep separate | Leave the todo pending for a later polish phase. | |
| Redesign transition lifecycle | Turn this phase into a broader shell surface transition system redesign. | |

**User's choice:** Smart fallback selected folding the audio popover polish as bounded proof.
**Notes:** Scope is limited to accepted behavior and existing `hide_transition_ms` / `closing_until` lifecycle hooks; this is not a broad transition-system redesign.

---

## the agent's Discretion

- Exact bucket names and helper APIs.
- Whether keyframe animations can be narrowed from conservative relayout on a per-property basis in Phase 56.
- Exact verification command grouping and fixture names.

## Deferred Ideas

- Module install requirement resolution remains outside Phase 56.
- Broad stacking, clipping, z-index, and repaint-policy tuning remains Phase 57.
- Backend selection/rollback observability remains Phase 58.
