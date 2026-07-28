# Phase 29: Damage-Indexed Paint Execution and Repaint Policy - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-11
**Phase:** 29-damage-indexed-paint-execution-and-repaint-policy
**Areas discussed:** Damage index shape, Overlay and chrome inclusion, Repaint policy, Correctness and fallback safety

---

## Damage Index Shape

| Option | Description | Selected |
|--------|-------------|----------|
| Subtree-first index | Use retained subtree or node ownership as the primary damage lookup, then paint commands owned by intersecting subtrees. | |
| Command-range-first index | Track explicit command spans or buckets as the primary lookup target. | |
| Hybrid | Start from subtree ownership, then retain lightweight command-span metadata inside each subtree for more precise filtered execution. | ✓ |
| Agent decides | Select the shape that best matches the retained renderer architecture. | |

**User's choice:** Agent decided based on the Qt retained-rendering model.
**Notes:** The user first selected this area explicitly, then asked for the whole discussion to be decided "based on the Qt rendering." The chosen direction preserves Phase 28 subtree ownership and adds compact span metadata inside each subtree instead of introducing a brand-new global command index.

---

## Overlay and Chrome Inclusion

| Option | Description | Selected |
|--------|-------------|----------|
| Always include overlays | Any overlay or chrome work repaints on every damage event to avoid correctness risk. | |
| Owner-root inclusion | Repaint subtree-owned chrome when the owning viewport or subtree participates in damage; keep external overlays separate. | ✓ |
| Pure geometry intersection | Repaint overlays only if their exact geometry intersects damage. | |
| Agent decides | Select the policy that best matches the retained renderer architecture. | |

**User's choice:** Agent decided based on the Qt retained-rendering model.
**Notes:** Scrollbars should behave as retained subtree-owned chrome, while tooltip-style overlay work already sits outside traversal timing and should remain a separate repaint concern. This keeps overlay behavior conservative without turning every overlay into global always-paint work.

---

## Repaint Policy

| Option | Description | Selected |
|--------|-------------|----------|
| Always minimal damage | Always repaint the smallest known dirty region. | |
| Cost-aware policy | Switch between minimal damage, bounding-rect repaint, and full-surface repaint based on measured cost and correctness conditions. | ✓ |
| Mostly full repaint | Favor broad repaint unless the damage case is trivial. | |
| Agent decides | Select the policy that best matches the retained renderer architecture. | |

**User's choice:** Agent decided based on the Qt retained-rendering model.
**Notes:** Qt guidance already locked this phase away from "smallest rect at all costs." The selected policy keeps minimal-damage repaint only when filtering is cheap, escalates to bounding-rect repaint when many small hits cluster, and falls back to full-surface repaint when retained ancestry or correctness becomes ambiguous.

---

## Correctness and Fallback Safety

| Option | Description | Selected |
|--------|-------------|----------|
| Aggressive filtering | Try hard to keep filtered execution even through ambiguous ordering or clip cases. | |
| Conservative fallback | Preserve current ordering and batch-barrier semantics, and broaden repaint quickly when filtered execution is unclear. | ✓ |
| Full precision indexing | Add deeper per-command clip/state machinery to avoid falling back. | |
| Agent decides | Select the policy that best matches the retained renderer architecture. | |

**User's choice:** Agent decided based on the Qt retained-rendering model.
**Notes:** The retained display-list order remains authoritative. The discussion intentionally rejected deeper clip-heavy precision machinery in this phase because Qt guidance and the existing milestone research both treat clipping and state proliferation as a common way to lose batching and CPU wins.

---

## the agent's Discretion

- Exact subtree span representation, including whether spans are stored as command ranges, block IDs, or another compact local descriptor
- Exact repaint-policy thresholds for minimal-damage, bounding-rect, and full-surface escalation
- Exact naming and placement of filtered-execution and repaint-policy counters in the existing profiling/debug payload

## Deferred Ideas

- More advanced spatial indexing if subtree-local span filtering is not sufficient
- Raster cache hardening remains Phase 30 work
- GPU backend and parallel paint or layout remain out of scope for this CPU retained-rendering milestone
