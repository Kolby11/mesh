# Roadmap: MESH

## Milestones

- [x] **v1.5 CPU Rendering Performance Improvement** - Phases 26-31 shipped 2026-05-13.
- [ ] **v1.6 Skia-Backed Rendering Performance Investigation** - planned next.

## Completed Phases

<details>
<summary>v1.5 CPU Rendering Performance Improvement (Phases 26-31) - SHIPPED 2026-05-13</summary>

- [x] Phase 26: CPU Render Profiling and Baseline Proof (1/1 plan) - completed 2026-05-10.
- [x] Phase 27: Viewport Culling and Visibility Elision (1/1 plan) - completed 2026-05-11.
- [x] Phase 28: Incremental Paint Command Retention (1/1 plan) - completed 2026-05-11.
- [x] Phase 29: Damage-Indexed Paint Execution and Repaint Policy (2/2 plans) - completed 2026-05-12.
- [x] Phase 30: Raster Cache Hardening for Icons, Images, and Text (1/1 plan) - completed 2026-05-12.
- [x] Phase 31: Smoothness Proof and CPU Render Tuning (4/4 plans) - completed 2026-05-13.

Archive artifacts:

- `.planning/milestones/v1.5-ROADMAP.md`
- `.planning/milestones/v1.5-REQUIREMENTS.md`
- `.planning/milestones/v1.5-MILESTONE-AUDIT.md`

Accepted tech debt:

- Slight audio popover transition delay remains as deferred polish by user request.
- Phase 26 and Phase 30 are missing retroactive `VALIDATION.md` artifacts; verification passed.

</details>

## Planned Phases

### v1.6 Skia-Backed Rendering Performance Investigation

**Goal:** Determine whether a Skia-backed renderer materially improves MESH rendering performance and, if it does, migrate the low-level paint backend behind the existing retained-rendering architecture.

Planned scope:

- Research Rust Skia integration options, build constraints, CPU/GPU backend support, Wayland presentation fit, and long-term maintenance cost.
- Build a benchmarkable Skia-backed painter spike for the existing retained display-list command stream.
- Compare Skia CPU and available GPU paths against the current custom/tiny-skia/resvg/cosmic-text/swash software stack on canonical scenarios.
- Decide whether to migrate `mesh-core-render` primitives to Skia wholesale, use Skia selectively for expensive primitives, or keep the current renderer.
- If the spike wins, plan the migration behind the existing retained widget tree, render-object tree, damage policy, profiling, and shell presentation boundaries.

Out of scope for the spike:

- Replacing the `.mesh` compiler, layout engine, retained tree, module system, input handling, or shell service architecture.
- Removing v1.5 retained-pipeline work; Skia should consume the improved retained command stream rather than replace the architecture around it.
- Shipping a partial migration without benchmark proof and visual-correctness coverage.

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 26. CPU Render Profiling and Baseline Proof | v1.5 | 1/1 | Complete | 2026-05-10 |
| 27. Viewport Culling and Visibility Elision | v1.5 | 1/1 | Complete | 2026-05-11 |
| 28. Incremental Paint Command Retention | v1.5 | 1/1 | Complete | 2026-05-11 |
| 29. Damage-Indexed Paint Execution and Repaint Policy | v1.5 | 2/2 | Complete | 2026-05-12 |
| 30. Raster Cache Hardening for Icons, Images, and Text | v1.5 | 1/1 | Complete | 2026-05-12 |
| 31. Smoothness Proof and CPU Render Tuning | v1.5 | 4/4 | Complete | 2026-05-13 |

## Archived Milestones

- `v1.5` CPU Rendering Performance Improvement - shipped 2026-05-13.
- `v1.4` Major Performance Fixes - shipped 2026-05-09.
- `v1.3` Performance Instrumentation and Responsiveness - shipped 2026-05-09.
- `v1.2` Rendering System Upgrade - shipped 2026-05-08.
- `v1.1` Backend Plugin MVP - shipped 2026-05-05.

## Backlog and Carryover

- Deferred validation/UAT cleanup from older milestones remains backlog work outside `v1.6`.
- The pending unified package/module manifest phase idea remains future planning work and is not part of renderer optimization.
- The slight audio popover transition delay from Phase 31 remains deferred polish: `.planning/todos/pending/2026-05-13-phase31-audio-popover-transition-delay.md`.
- Parallel paint/layout remains sequenced after the retained CPU pipeline and any Skia investigation clarify the next rendering boundary.

---
*Roadmap updated: 2026-05-13 after archiving v1.5*
