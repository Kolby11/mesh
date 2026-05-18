# Phase 42: Renderer Architecture Decision Matrix - Context

**Gathered:** 2026-05-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 42 produces a source-backed renderer architecture decision matrix. It compares Blitz direct adoption, Blitz-inspired architecture borrowing, and a MESH-owned focused-crate renderer path before any prototype or production integration work commits to a direction.

This phase delivers the decision framework and crate outcomes only. It does not build prototypes, wire production renderer paths, or replace any existing renderer components.

</domain>

<decisions>
## Implementation Decisions

### Blitz Posture

- **D-01:** Treat Blitz as the reference architecture by default. Study and prototype against Blitz, but default to preserving MESH-owned renderer boundaries unless direct adoption is clearly cleaner.
- **D-02:** The direct-adoption bar is experimental enough to allow internal MESH redesign if the comparison justifies it. The redesign must be explicit, measurable, and justified by the matrix.
- **D-03:** Wayland shell model mismatch is a hard blocker for direct Blitz adoption.
- **D-04:** Browser-engine-level performance overhead is a hard blocker for direct Blitz adoption. MESH must not feel like it is embedding a full browser engine.
- **D-05:** If Blitz fails direct-adoption gates, Phase 42 should still recommend borrowing architecture and crates where useful, especially boundaries and pieces such as Taffy, Parley, and AnyRender.

### Scorecard Gates

- **D-06:** Direct Blitz adoption has two hard blockers: Wayland shell model fit and no browser-engine-level performance overhead.
- **D-07:** After those blockers pass, capability gain matters most. The matrix should favor the path that gives MESH the most meaningful CSS/layout/text/rendering capability improvement.
- **D-08:** Performance overhead includes interaction latency, render/layout/paint architecture cost, startup time, compile/build cost, binary size, memory, resource usage, and native dependency burden.
- **D-09:** Observability may temporarily regress during prototype work if the final migration plan restores MESH equivalents for invalidation, damage, profiling, diagnostics, and debug payloads.
- **D-10:** Build/dependency cost is acceptable when it unlocks significant renderer capability, but it must be measured and included in the matrix.

### Crate Outcomes

- **D-11:** Skia/rust-skia is a fallback path, valuable if Blitz/AnyRender cannot meet MESH needs.
- **D-12:** Taffy and Parley are likely accepts as strong standalone candidates for layout and text, even if Blitz itself is not adopted.
- **D-13:** Stylo is a direct candidate for MESH style resolution if it brings enough CSS capability and passes shell/performance tradeoffs.
- **D-14:** AnyRender / Vello-style abstraction is a likely accept and should be evaluated as the preferred rendering abstraction path before falling back to Skia.
- **D-15:** Winit and AccessKit are likely accept candidates.
- **D-16:** Muda, html5ever, and xml5ever are deferred unless a concrete need appears.

### Prototype Boundary Handoff

- **D-17:** Phase 43 should compare both the navigation bar and audio popover as prototype targets.
- **D-18:** Phase 43 prototypes should be throwaway harnesses, not production-wired paths.
- **D-19:** Phase 43 prototypes should compare visual output plus interaction shape: hover, click, slider, and open-close behavior. They do not need real backend runtime, diagnostics, or profiling.
- **D-20:** Phase 42 should hand off a decision matrix only. Phase 43 planners infer prototype details from scores and constraints.
- **D-21:** If the two-surface prototype scope feels expensive, do not reduce it. Both navigation bar and audio popover are required for useful comparison.

### Folded Todos

- **Evaluate Blitz crate dependencies** (`.planning/todos/pending/2026-05-17-evaluate-blitz-crate-dependencies.md`) is folded into Phase 42. It asks the matrix to map MESH's current rendering/layout/input/accessibility modules against Blitz's crate boundary model and decide which ecosystem dependencies fit.

### the agent's Discretion

The agent may choose the exact matrix format, scoring scale, source list, and evidence presentation, provided the decisions above are represented explicitly and the hard blockers are separated from weighted tradeoffs.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Current Milestone Scope

- `.planning/PROJECT.md` — Project core value, validated renderer history, and v1.8 milestone target.
- `.planning/REQUIREMENTS.md` — Phase 42 requirements REND-01 through REND-03 and crate outcome scope.
- `.planning/ROADMAP.md` — Phase 42 goal, dependencies, and success criteria.
- `.planning/research/SUMMARY.md` — v1.8 renderer research summary and recommended stack evaluation.

### Folded Todo and Prior Proof

- `.planning/todos/pending/2026-05-17-evaluate-blitz-crate-dependencies.md` — Original user-captured Blitz crate boundary and dependency evaluation task.
- `.planning/spikes/MANIFEST.md` — Existing Skia retained-display-list painter spike summary; Skia CPU-raster feasibility was validated in isolation.

### Codebase Context Maps

- `.planning/codebase/STACK.md` — Current Rust/Luau/.mesh stack, rendering dependencies, Wayland runtime, and native dependency context.
- `.planning/codebase/ARCHITECTURE.md` — Current shell/frontend/render/presentation architecture and anti-patterns.
- `.planning/codebase/INTEGRATIONS.md` — Wayland/compositor, IPC, system command, observability, and environment integration context.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- `crates/core/frontend/render/src/display_list.rs` — Existing retained display-list path that any renderer comparison must map or explicitly replace.
- `crates/core/frontend/render/src/render_object.rs` — Retained render-object tree with dirty slots for transform, clip, opacity, geometry, material, text, and accessibility.
- `crates/core/frontend/render/src/surface/painter.rs` and `crates/core/frontend/render/src/surface/painter/*` — Current software painter boundary for widgets, geometry, text, and tree painting.
- `crates/core/frontend/render/src/surface/profiling.rs` — Current render profiling concepts that can temporarily regress in prototypes but must be restored by migration.
- `crates/core/presentation/src/lib.rs` and `crates/core/presentation/src/wayland_surface/*` — Current presentation and Wayland damage path; direct Blitz adoption must fit this shell model.
- `modules/frontend/navigation-bar/src/main.mesh` and `modules/frontend/audio-popover/src/main.mesh` — Required real surfaces for Phase 43 comparison.

### Established Patterns

- MESH already owns retained widget identity, typed invalidation, retained render objects, retained display data, damage tracking, text caching, selector indexing, profiling snapshots, and shipped-surface benchmarks.
- Rust core must stay generic across services and shell surfaces; renderer architecture must not introduce service-specific branches.
- Frontend authors use `.mesh` components with template/script/style blocks, not arbitrary browser pages.
- Presentation is Wayland-native shell software, not a conventional app-window renderer.

### Integration Points

- Renderer decision must account for `mesh-core-render`, `mesh-core-presentation`, `mesh-core-frontend`, `mesh-core-component`, and shell component runtime paths.
- Any future Winit evaluation must be judged against MESH's Wayland/layer-shell lifecycle rather than generic desktop app needs.
- Any future AccessKit evaluation should use retained node identity as the update boundary.

</code_context>

<specifics>
## Specific Ideas

- Blitz is the reference architecture by default, but Phase 42 may still recommend deeper adoption if the scorecard justifies internal MESH redesign.
- Direct Blitz adoption must not create browser-engine-level overhead.
- Taffy, Parley, AnyRender/Vello-style rendering, Winit, and AccessKit have a positive default stance.
- Stylo is worth direct evaluation, not only evaluation through Blitz.
- Skia is fallback even though the prior isolated Skia spike validated CPU-raster feasibility.
- Phase 43 should compare both navigation bar and audio popover with throwaway harnesses.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

### Reviewed Todos (not folded)

- Audio popover transition delay polish (`.planning/todos/pending/2026-05-13-phase31-audio-popover-transition-delay.md`) was matched by keyword noise but not folded. It remains accepted polish debt unless it naturally falls out of the Phase 43 audio-popover prototype comparison.

</deferred>

---

*Phase: 42-Renderer Architecture Decision Matrix*
*Context gathered: 2026-05-18*
