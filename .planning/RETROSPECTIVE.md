# Project Retrospective

*A living document updated after each milestone. Lessons feed forward into future planning.*

## Milestone: v1.5 - CPU Rendering Performance Improvement

**Shipped:** 2026-05-13
**Phases:** 6 | **Plans:** 10 | **Sessions:** multiple live implementation and UAT sessions

### What Was Built

- CPU render profiling attribution for canonical shipped-surface scenarios.
- Viewport, visibility, and clip-aware retained paint pruning.
- Incremental retained paint-command updates and damage-indexed paint execution.
- Raster cache hardening for SVG, bitmap, icon, text, and glyph paths.
- Repaint-policy tuning and shipped-surface smoothness proof, including live audio popover UAT fixes.

### What Worked

- Canonical benchmark scenarios kept performance claims tied to reproducible shell interactions.
- Retained rendering boundaries from v1.4 gave later phases clean ownership for culling, command retention, and damage filtering.
- Live UAT exposed interaction regressions that benchmark counters alone would not have caught.

### What Was Inefficient

- Some live audio-surface behavior required several retest/fix loops after the initial smoothness proof.
- Phase 26 and Phase 30 passed verification but did not leave `VALIDATION.md` artifacts, creating archive-time metadata debt.

### Patterns Established

- Treat visible smoothness as a joint benchmark plus live-UAT acceptance condition.
- Keep repaint-policy thresholds conservative unless shipped-surface proof shows a clear reason to widen them.
- Record deferred polish as explicit pending todo files before milestone archive.

### Key Lessons

1. Performance wins are not complete until shipped controls still feel correct under immediate pointer and backend updates.
2. Stateful popovers need a single source of truth for hover, focus, command, and backend reconciliation paths.
3. Future renderer migrations should consume the retained display-list pipeline rather than bypass it.

### Cost Observations

- Model mix: not tracked.
- Sessions: multiple.
- Notable: the most expensive work was not raw implementation, but live interaction convergence around the audio popover.

---

## Cross-Milestone Trends

### Process Evolution

| Milestone | Sessions | Phases | Key Change |
|-----------|----------|--------|------------|
| v1.5 | multiple | 6 | Performance acceptance moved from counters-only proof to benchmark plus live-UAT proof. |

### Cumulative Quality

| Milestone | Tests | Coverage | Zero-Dep Additions |
|-----------|-------|----------|-------------------|
| v1.5 | Focused Rust and `.mesh` regression tests plus live UAT | Requirements 17/17 | None identified |

### Top Lessons (Verified Across Milestones)

1. Keep service-specific behavior out of Rust core while still testing shipped proof surfaces end to end.
2. Retained renderer work needs stable debug payloads so optimizations remain observable after each phase.
