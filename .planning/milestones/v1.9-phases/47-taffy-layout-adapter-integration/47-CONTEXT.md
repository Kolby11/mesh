# Phase 47: Taffy Layout Adapter Integration - Context

**Gathered:** 2026-05-18T21:23:26+02:00
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 47 replaces the relevant current MESH layout computation with Taffy for the in-scope production layout surface: retained MESH widget nodes, shipped navigation/audio surfaces, and required parity cases for rows, columns, stacks, fixed sizes, gaps, padding, absolute positioning, and container-width behavior.

This is not a long-lived compatibility adapter phase. Taffy should become the authoritative layout path for supported MESH layout semantics in this phase. Existing MESH node identity, runtime keys, dirty categories, retained render-object synchronization, profiling, diagnostics, and shipped surface behavior remain preservation gates around the replacement.

</domain>

<decisions>
## Implementation Decisions

### Taffy Replacement Boundary

- **D-01:** Replace the relevant current layout engine code with Taffy for in-scope layout behavior. Do not preserve a parallel backward-compatible production layout path for cases Taffy can handle.
- **D-02:** Treat Phase 47 as a production replacement for layout, not proof-only scaffolding and not an opt-in runtime experiment. The planner should design around Taffy as the authoritative layout computation path for supported rows, columns, stacks, fixed sizes, gaps, padding, absolute positioning, and container-width cases.
- **D-03:** Phase 46 rollback decisions remain relevant for the broader renderer-library scaffold and for non-layout libraries, but Phase 47 intentionally overrides the default-authority posture for layout: Taffy is expected to take over the relevant layout code rather than remain adapter-owned evidence only.

### Unsupported Cases And Diagnostics

- **D-04:** Unsupported Taffy cases are implementation gaps to diagnose and close, not silent runtime fallbacks to the old layout engine.
- **D-05:** Reconcile LAYT-03 by making unsupported cases visible through non-fatal diagnostics, failing parity coverage, or explicit unsupported-case records. The planner should not use LAYT-03 to keep the old layout engine as a hidden compatibility path.
- **D-06:** If the implementation discovers a current MESH layout feature that Taffy cannot represent without semantic loss, the phase should either map it explicitly, narrow the supported semantics with documentation and tests, or record a blocker. It should not silently mask the gap with old-engine output.

### Preservation Gates

- **D-07:** Stable `NodeId`, runtime keys, retained dirty categories, and retained render-object geometry synchronization are hard gates. Taffy may own geometry computation, but MESH identity and invalidation semantics remain authoritative.
- **D-08:** Existing shipped navigation/audio behavior is the acceptance target. The replacement must preserve visible geometry and interaction hit regions for shipped surfaces, not only satisfy synthetic layout unit tests.
- **D-09:** Layout diagnostics should be explicit enough for future renderer phases to distinguish Taffy mapping gaps from paint, text, presentation, or shell lifecycle problems.

### Parity And Test Scope

- **D-10:** Parity tests must cover the roadmap-required layout cases: rows, columns, stacks, fixed sizes, gaps, padding, absolute positioning, and container-width cases.
- **D-11:** Shipped navigation and audio fixtures should be canonical parity cases, because Phase 47 is intended to replace production-relevant layout behavior rather than merely prove an isolated adapter.
- **D-12:** Tests should compare resulting `LayoutRect` geometry and retained identity effects where possible. Exact implementation strategy is planner discretion, but failures should point at concrete semantic differences rather than broad snapshot drift.

### Todo Scope

- **D-13:** Do not fold the audio popover transition-delay todo into Phase 47. It remains animation and motion-fidelity polish for v1.10, not Taffy layout replacement scope.

### the agent's Discretion

The user directly selected strict replacement over fallback compatibility. The planner has discretion over exact module placement, adapter type names, and migration mechanics, but must preserve the decision that Taffy replaces the relevant in-scope layout code and that unsupported cases become diagnostics or blockers rather than silent old-engine fallbacks.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone Scope

- `.planning/PROJECT.md` — Current v1.9 renderer-library integration goal and the v1.10 animation deferral.
- `.planning/REQUIREMENTS.md` — LAYT-01 through LAYT-03 requirements and out-of-scope boundaries.
- `.planning/ROADMAP.md` — Phase 47 goal and success criteria.
- `.planning/STATE.md` — Carried-forward renderer migration decisions and current milestone state.

### Prior Phase Context

- `.planning/phases/46-renderer-library-dependency-and-adapter-foundation/46-CONTEXT.md` — Feature scaffold, renderer-library dependency decisions, and broader rollback posture that Phase 47 narrows for layout.
- `.planning/phases/46-renderer-library-dependency-and-adapter-foundation/46-SUMMARY.md` — Completed dependency and adapter foundation evidence.
- `.planning/phases/46-renderer-library-dependency-and-adapter-foundation/46-VERIFICATION.md` — Verified Phase 46 build/test gates.

### Renderer Migration Contracts

- `docs/renderer-migration.md` — Renderer migration principles, promotion gates, dependency record, and Phase 46 dependency choices.
- `docs/renderer-ownership.md` — Authoritative, adapter-owned, and replacement-candidate boundaries. Phase 47 promotes Taffy layout from replacement candidate toward authoritative layout behavior for in-scope semantics.
- `docs/frontend/renderer-contract.md` — Public `.mesh` renderer contract and author-facing stability boundary.
- `crates/core/frontend/render/README.md` — Render crate ownership boundary.

### Current Layout Code

- `crates/core/ui/elements/src/layout.rs` — Current custom `LayoutEngine`, intrinsic layout cache, text measurer integration, flex-like row/column layout, padding/gap handling, absolute positioning, and layout unit tests.
- `crates/core/ui/elements/src/tree.rs` — `WidgetNode`, retained node identity, computed style, and layout storage.
- `crates/core/shell/src/shell/component/rendering.rs` — Shell component layout invocation, restyle/layout invalidation behavior, intrinsic cache use, and profiling-stage integration.
- `crates/core/shell/src/shell/component/runtime_tree.rs` — Retained runtime-tree identity and dirty tracking that Taffy layout must not invalidate.
- `crates/core/frontend/render/src/render_object.rs` — Retained render-object geometry slots and dirty summaries driven by layout changes.
- `crates/core/frontend/render/src/library_adapters.rs` — Phase 46 renderer-library feature/status scaffold, including `renderer-taffy`.
- `crates/core/frontend/render/src/proof.rs` — Focused proof evidence containing prior `taffy_layout` snapshots.

### Shipped Surface Fixtures

- `modules/frontend/navigation-bar/src/main.mesh` — Shipped navigation surface layout source.
- `modules/frontend/navigation-bar/src/components/volume-button.mesh` — Audio entry/control component participating in shipped navigation/audio geometry.
- `crates/core/shell/src/shell/component/tests` — Existing shipped-surface, restyle, invalidation, and Phase 44 navigation/audio tests to reuse or extend.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- `LayoutEngine::compute_with_intrinsic_cache_and_measurer` in `crates/core/ui/elements/src/layout.rs`: current layout entrypoint used by shell rendering. This is the likely replacement boundary or compatibility shim to convert retained `WidgetNode` trees into Taffy layout input and write computed `LayoutRect`s back.
- `IntrinsicLayoutCache` and `TextMeasurer`: current layout supports text/intrinsic measurement. Taffy integration must preserve equivalent measurement hooks or explicitly gate any semantic differences.
- `crates/core/shell/src/shell/component/rendering.rs`: already wraps layout in profiling and dirty-category logic, so the Taffy path should integrate there without weakening profiling or retained restyle behavior.
- `RenderObjectTree` in `crates/core/frontend/render/src/render_object.rs`: existing geometry dirty detection can verify that Taffy-written layout rectangles still synchronize into retained render objects.
- `renderer-taffy` feature scaffold in `crates/core/frontend/render`: confirms Phase 46 made Taffy dependency fan-out available, but Phase 47 may need to move actual Taffy use closer to `mesh-core-elements` if the current layout engine lives there.

### Established Patterns

- Retained identity is MESH-owned. Candidate libraries may compute geometry, but stable `NodeId`, runtime keys, dirty categories, and retained render-object synchronization remain MESH contracts.
- Existing layout supports flex-like row/column behavior, padding, gaps, percent dimensions, content/intrinsic sizing, absolute positioning, RTL row mirroring, display none, overflow-aware natural sizing, min/max constraints, and text measurement. Taffy replacement needs explicit mapping decisions and tests for the in-scope subset.
- Renderer migration docs previously emphasized reversibility. Phase 47 deliberately narrows that for layout based on user direction: broad project rollback can still exist at git/feature level, but production behavior should not carry a hidden old-layout fallback for supported layout semantics.
- Shipped-surface proof matters more than isolated synthetic proof. Navigation/audio geometry and interaction hit regions are required acceptance evidence.

### Integration Points

- `crates/core/ui/elements/src/layout.rs`: primary layout implementation boundary to replace or wrap with Taffy.
- `crates/core/ui/elements/Cargo.toml` and workspace `Cargo.toml`: possible dependency-feature adjustment if Taffy must be used in `mesh-core-elements` rather than `mesh-core-render`.
- `crates/core/shell/src/shell/component/rendering.rs`: layout invocation, retained restyle behavior, and profiling integration.
- `crates/core/frontend/render/src/render_object.rs`: geometry dirty propagation after Taffy writes layout.
- `crates/core/shell/src/shell/component/tests`: likely home for shipped-surface parity/regression tests.
- `docs/renderer-migration.md` and `docs/renderer-ownership.md`: documentation must be updated if Phase 47 promotes Taffy layout authority beyond the Phase 46 scaffold.

</code_context>

<specifics>
## Specific Ideas

The user explicitly said: "we should not concern ourselfs with backward compatibility we should replace the relavant code using taffy" and selected strict replacement over unsupported-case fallback compatibility.

</specifics>

<deferred>
## Deferred Ideas

### Reviewed Todos (not folded)

- Audio Popover Transition Delay Polish — deferred to the v1.10 animations and motion-fidelity milestone. It is visible behavior polish around surface transitions, not Phase 47 Taffy layout replacement scope.

</deferred>

---

*Phase: 47-Taffy Layout Adapter Integration*
*Context gathered: 2026-05-18T21:23:26+02:00*
