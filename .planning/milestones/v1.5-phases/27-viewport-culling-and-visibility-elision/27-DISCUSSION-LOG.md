# Phase 27: Viewport Culling and Visibility Elision - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-11
**Phase:** 27-viewport-culling-and-visibility-elision
**Areas discussed:** Culling boundary, Hidden-state semantics, Elision granularity, Debug visibility

---

## Culling Boundary

| Option | Description | Selected |
|--------|-------------|----------|
| Clipped and scrollable containers only | Safest first pass; reuse explicit clip rectangles and scroll offsets only. | |
| Any subtree fully outside the surface viewport | Broader win potential, but pushes toward global visibility reasoning. | |
| Hybrid | Start from explicit clip/scroll regions, but also allow trivially off-viewport root-surface omission. | |
| You decide | Let the agent choose the boundary from existing code paths and milestone goals. | ✓ |

**User's choice:** `4`
**Notes:** The decision was resolved as a hybrid boundary: explicit clip/scroll regions remain primary, with cheap root-surface omission allowed only when trivially provable.

---

## Culling Boundary Follow-up

| Option | Description | Selected |
|--------|-------------|----------|
| Skip paint execution only | Preserve retained display-list generation and only omit final paint work. | ✓ |
| Skip paint-command generation and paint execution | Cut work earlier by omitting both command generation and final paint. | |
| Skip render-object sync, paint-command generation, and paint execution | Pull earlier retained-tree ownership work into this phase. | |
| You decide | Let the agent choose the cut point. | |

**User's choice:** `1`
**Notes:** The user kept Phase 27 scoped to paint-time omission first instead of pulling retained-command rebuild ownership into this phase.

---

## Hidden-State Semantics

| Option | Description | Selected |
|--------|-------------|----------|
| Only existing hard-hidden cases | Restrict omission to explicit hidden/display-none/surface-hidden style cases. | |
| Include fully transparent branches when they cannot visually contribute | Practical CPU win without low-opacity heuristics. | |
| Include partially transparent branches too when opacity is very low | Heuristic expansion beyond clear correctness boundaries. | |
| You decide | Let the agent choose the hidden-state rule. | |

**User's choice:** Free-text: “i want thsi to be done as it is done in qt painte”
**Notes:** The discussion pivoted to aligning the phase with Qt Quick’s renderer model. That replaced the earlier opacity discussion with a stricter Qt-like conservative visibility baseline.

---

## Elision Granularity

| Option | Description | Selected |
|--------|-------------|----------|
| Whole-subtree omission only | Omit fully invisible subtrees inside explicit viewport/clip regions. | |
| Whole-subtree omission plus per-command filtering | Broader optimization, overlapping later retained-command work. | |
| Root-level subtree omission plus clipped text/list-style child pruning where viewport knowledge already exists | Closest Qt-style rough pre-clipping interpretation. | ✓ |
| You decide | Let the agent choose from current retained/display-list seams. | |

**User's choice:** `lock 3`
**Notes:** The user asked how Qt does it first, then accepted option 3 as the closest Qt-like interpretation: explicit viewport authority, whole-subtree omission where fully outside, plus localized rough pre-clipping for viewport-aware content.

---

## Debug Visibility

| Option | Description | Selected |
|--------|-------------|----------|
| Aggregate counters only | Lightweight proof with stable metrics and low overhead. | ✓ |
| Aggregate counters plus reason categories | More diagnosis detail with modest extra scope. | |
| Per-node or per-subtree detailed records | High-detail diagnostics feature. | |
| You decide | Let the agent choose the proof detail level. | |

**User's choice:** `1`
**Notes:** The user preferred aggregate-only proof in the existing debug/profiling path, not a detailed pruning trace system.

---

## the agent's Discretion

- Exact API seam for viewport-aware pruning within the existing retained display-list and filtered paint-node path.
- Exact aggregate counter names and placement in the existing profiling/debug payload.

## Deferred Ideas

- Do not build a global smart visibility or occlusion system in Phase 27.
- Do not treat opacity as a general hidden-state heuristic in this phase.
- Defer broad command-level filtering across partially visible subtrees to later retained paint-command / damage-indexed phases.
