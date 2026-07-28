# Phase 28: Incremental Paint Command Retention - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-11
**Phase:** 28-incremental-paint-command-retention
**Areas discussed:** Ownership granularity, Dirty-change handling, Ordering and fallback safety, Debug and proof

---

## Ownership Granularity

| Option | Description | Selected |
|--------|-------------|----------|
| Keep surface-wide command rebuilds | Preserve the current single flat rebuild step even for local retained changes. | |
| Dirty-subtree retained ownership | Track command ownership per dirty subtree keyed by stable retained node identity so unrelated branches can survive local changes. | ✓ |
| Full per-command global diff | Diff and patch commands individually across the whole surface. | |

**User's choice:** Execute-mode fallback selected `Dirty-subtree retained ownership`.
**Notes:** The skill's interactive question path was not available in this execution mode, so the fallback default chose the narrowest option that satisfies `PIPE-01` without pulling Phase 29 scope forward.

---

## Dirty-Change Handling

| Option | Description | Selected |
|--------|-------------|----------|
| Rebuild the whole surface on transform/scroll/reorder changes | Keep current broad invalidation behavior for these mutations. | |
| Preserve unrelated descendant paint data | Allow transform-, scroll-, and reorder-only paths to patch retained ancestry/metadata while keeping unaffected descendants intact. | ✓ |
| Add damage-indexed command filtering now | Combine ownership retention with filtered execution in this phase. | |

**User's choice:** Execute-mode fallback selected `Preserve unrelated descendant paint data`.
**Notes:** This aligns directly with `PIPE-02` and with the current render-object/display-list split, while leaving damage-region execution to Phase 29.

---

## Ordering and Fallback Safety

| Option | Description | Selected |
|--------|-------------|----------|
| Always prefer local reuse | Force subtree retention even when dirty summaries are broad or ambiguous. | |
| Conservative local reuse with full-surface fallback | Reuse locally when ownership is provable, otherwise fall back to the existing whole-surface path. | ✓ |
| Disable local reuse for reorder/scroll paths | Keep only the simplest transform cases incremental. | |

**User's choice:** Execute-mode fallback selected `Conservative local reuse with full-surface fallback`.
**Notes:** This matches prior retained-rendering phases, preserves correctness, and keeps Phase 28 focused on local ownership rather than risky aggressive heuristics.

---

## Debug and Proof

| Option | Description | Selected |
|--------|-------------|----------|
| No new observability | Trust visual output and existing totals without reuse/fallback attribution. | |
| Aggregate reuse/fallback metrics in existing profiling path | Extend current debug/profiling output with subtree reuse, rebuild, and fallback counters. | ✓ |
| Per-command trace logging | Add deep command trace output for every retained reuse decision. | |

**User's choice:** Execute-mode fallback selected `Aggregate reuse/fallback metrics in existing profiling path`.
**Notes:** This stays consistent with Phase 26 and Phase 27 proof style and gives planners enough observability without introducing a second diagnostics system.

---

## the agent's Discretion

- Exact subtree cache structure and splice algorithm inside `mesh-core-render`
- Exact metric names and debug payload placement for reuse/rebuild/fallback reporting

## Deferred Ideas

- Damage-indexed command execution and repaint policy selection belong in Phase 29.
- Raster/resource caching work belongs in Phase 30.
- Unified package/module manifest planning remains unrelated backlog work.
