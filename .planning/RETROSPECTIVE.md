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

## Milestone: v1.8 - Rendering Engine Architecture

**Shipped:** 2026-05-18
**Phases:** 4 | **Plans:** 14 | **Sessions:** multiple planning, research, implementation, and verification sessions

### What Was Built

- Source-backed Blitz adopt-vs-build decision matrix with explicit candidate crate outcomes.
- Comparable renderer prototype evidence for Blitz reference and MESH-owned focused-crate paths.
- Retained MESH-shaped focused evidence using Taffy, Parley, AnyRender-style paint, and AccessKit-compatible boundaries.
- Production-adjacent focused proof integration behind current renderer and shell ownership.
- Phased renderer migration roadmap, ownership classification, and author-facing `.mesh` renderer contract.

### What Worked

- Running decision, prototype, integration, and migration-contract phases in sequence kept the renderer choice grounded in evidence.
- The shared navigation/audio slice made Blitz and focused-crate prototype results comparable instead of abstract.
- Treating focused proof snapshots as adapter-owned evidence avoided accidentally exposing migration internals as author API.

### What Was Inefficient

- The milestone close still surfaced old Phase 31 debug/todo artifacts, so cross-milestone deferred work needs periodic cleanup.
- The codebase drift gate hit a non-blocking SDK Node `EPERM`, which reduced confidence in that advisory signal.
- The archive SDK created useful archive files but still needed manual ROADMAP, PROJECT, and milestone-entry cleanup.

### Patterns Established

- Renderer candidates must preserve or replace retained `NodeId`, typed invalidation, damage, profiling, diagnostics, theme-owned selection, and AccessKit-compatible evidence before promotion.
- Broad renderer migration needs feature flags or local bypasses, rollback paths, Linux/Nix impact notes, binary/build risk notes, and exact CI gates.
- `.mesh` remains a bounded shell UI authoring surface, not browser DOM/HTML/CSS compatibility.

### Key Lessons

1. Architecture choices should move from source-backed matrix to comparable prototype to constrained production proof before migration planning.
2. A concrete blocker is acceptable prototype evidence when it is reproducible and compared under the same criteria.
3. Migration contracts should name non-goals as explicitly as guarantees, especially when browser-engine crates are involved.

### Cost Observations

- Model mix: not tracked.
- Sessions: multiple.
- Notable: the broadest command cost was the full workspace regression at Phase 45 closeout; it stayed green after docs-only migration planning.

---

## Milestone: v1.7 - Rethink Modularity and Extensibility Concepts

**Shipped:** 2026-05-18
**Phases:** 5 | **Plans:** 17 | **Sessions:** multiple planning, implementation, review, and verification sessions

### What Was Built

- Canonical module vocabulary and `module.json` manifest normalization.
- Typed installed-graph contribution records for frontend, resources, keybinds, interfaces, providers, settings, and libraries.
- Interface/provider/resource validation that keeps frontend requirements, backend provider identity, and host capabilities separate.
- Author-facing migration diagnostics and docs for legacy manifest names and canonical module workflows.
- Shipped navigation/audio proof that exercises canonical manifests and installed graph behavior without service-specific Rust branches.

### What Worked

- The vocabulary-first phase reduced ambiguity before manifest, graph, diagnostic, and proof work.
- Keeping old public names as replacement debt prevented compatibility paths from becoming renewed public API.
- The real navigation/audio proof forced docs, tests, and runtime behavior to converge on one workflow.

### What Was Inefficient

- v1.7 closed without a milestone audit artifact, so the archive records accepted audit debt.
- Several open artifacts remained from earlier milestones and had to be explicitly deferred at close.
- MILESTONES.md still contained stale planned v1.7 framing that needed cleanup after the SDK archive.

### Patterns Established

- Treat installed-graph records as the inspectable boundary for extension behavior.
- Preserve manifest-owned declarations as canonical data; settings should override effective behavior, not become declaration sources.
- Migration diagnostics should be author-facing and concrete, while temporary loaders remain internal implementation details.

### Key Lessons

1. Cross-cutting terminology work should be completed before adding more extension features.
2. Real shipped proof modules are better acceptance tests than isolated manifest fixtures alone.
3. Future renderer work should include an explicit adopt-vs-build decision before implementation phases.

### Cost Observations

- Model mix: not tracked.
- Sessions: multiple.
- Notable: the main cost was architectural reconciliation across previous milestone decisions.

---

## Cross-Milestone Trends

### Process Evolution

| Milestone | Sessions | Phases | Key Change |
|-----------|----------|--------|------------|
| v1.8 | multiple | 4 | Renderer architecture acceptance moved through decision matrix, comparable prototypes, focused production proof, and migration contract. |
| v1.7 | multiple | 5 | Module extensibility acceptance moved through vocabulary, manifest, graph, diagnostics, and shipped proof in one milestone. |
| v1.5 | multiple | 6 | Performance acceptance moved from counters-only proof to benchmark plus live-UAT proof. |

### Cumulative Quality

| Milestone | Tests | Coverage | Zero-Dep Additions |
|-----------|-------|----------|-------------------|
| v1.8 | Prototype cargo checks, focused renderer proof tests, shell navigation/audio regressions, workspace test, and docs grep verification | Requirements 13/13 | None identified |
| v1.7 | Focused Rust manifest, graph, shell, diagnostic, and docs proof tests | Requirements 13/13 | None identified |
| v1.5 | Focused Rust and `.mesh` regression tests plus live UAT | Requirements 17/17 | None identified |

### Top Lessons (Verified Across Milestones)

1. Keep service-specific behavior out of Rust core while still testing shipped proof surfaces end to end.
2. Retained renderer work needs stable debug payloads so optimizations remain observable after each phase.
3. Canonical vocabulary and manifest contracts should be locked before expanding plugin-facing runtime behavior.
4. Renderer migration work should keep author contracts, rollback gates, and ownership boundaries explicit before broad adoption.
