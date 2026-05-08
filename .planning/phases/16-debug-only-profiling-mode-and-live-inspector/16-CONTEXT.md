# Phase 16: Debug-Only Profiling Mode and Live Inspector - Context

**Gathered:** 2026-05-08
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase turns the existing profiling runtime work into a shipped developer-facing inspector experience. It adds a debug-only profiling mode inside the current shell debug path and replaces the native debug panel with a `.mesh`-driven inspector surface that consumes shell-exposed debug/profiling endpoints.

This phase covers:
- Keeping profiling activation inside the existing debug path, with explicit profiling collection state.
- Replacing the current native right-side debug panel with a right-side `.mesh` inspector.
- Shipping the inspector as a core frontend module/package that behaves like a normal `.mesh` surface/component.
- Exposing shell-owned debug/profiling data through endpoints that the built-in inspector uses as a reference consumer.
- Providing inspector views for overview, surfaces, backend services, and a scaffolded benchmark/interaction section.
- Making the inspector tolerate empty or sparse recent-sample data without breaking the UI.

This phase does not add:
- A separate profiling-only diagnostics path.
- Full benchmark scenario launch/proof flows; those belong to Phase 17.
- Trace capture, replay, or long-lived profile persistence.
- A broader shell/debug architecture rewrite outside the debug/profiling inspector surface.

</domain>

<decisions>
## Implementation Decisions

### Activation Model
- **D-01:** Profiling remains reachable only through the existing debug path; Phase 16 must not introduce a separate profiling-only diagnostics entrypoint.
- **D-02:** Profiling is an explicit toggle or mode inside the debug flow, not something that automatically activates when the debug UI appears.
- **D-03:** Enabling profiling does not automatically open the inspector and does not automatically switch the current debug UI into profiling views.
- **D-04:** Profiling collection state is independent from inspector visibility.
- **D-05:** Once enabled, profiling remains active for the current shell session until it is explicitly turned off.

### Inspector Host Surface
- **D-06:** Phase 16 replaces the current native debug panel with a `.mesh`-driven inspector rather than embedding profiling content into the old native panel.
- **D-07:** The inspector keeps the current right-side panel placement pattern so the interaction model stays familiar while the implementation shifts to `.mesh`.
- **D-08:** The inspector is shipped by core but should behave like a normal frontend `.mesh` module/surface rather than a one-off native diagnostics widget.
- **D-09:** The inspector's only special capability is access to shell-exposed debug/profiling endpoints.
- **D-10:** The built-in inspector is a reference consumer of the debug API, not a privileged private UI; in principle, user-authored modules should be able to build the same kind of panel using the same debug API surface.
- **D-11:** The inspector should ship as an internal core frontend module/package that the shell loads when debug mode is active.

### Benchmark View Boundary
- **D-12:** Phase 16 includes a benchmark/interaction inspector view, but only as a scaffold; the canonical repeatable benchmark flows remain Phase 17 work.
- **D-13:** The scaffolded benchmark view should define the benchmark categories and explain what each one is intended to measure rather than acting as a generic placeholder.
- **D-14:** The benchmark scaffold should include categories for hover, surface open/close, slider or pointer-driven update, keyboard traversal, and backend-driven update.

### the agent's Discretion
- Planner/researcher may choose the exact UX for switching between debug inspector views, as long as profiling activation remains explicit and separate from inspector visibility.
- Planner/researcher may choose the exact packaging and module-loading mechanism for the internal core frontend module/package, as long as it behaves like a normal `.mesh` consumer and stays shell-shipped.
- Planner/researcher may choose the exact debug endpoint shape exposed to the inspector module, as long as the built-in inspector remains a reference consumer and the API could in principle be used by user-authored modules too.
- Planner/researcher may choose the exact empty-state presentation for sparse surfaces/services/benchmark categories, as long as the inspector stays stable and informative when no recent samples exist.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone and Phase Scope
- `.planning/PROJECT.md` - v1.3 milestone framing, including the locked debug-only profiling rule and the requirement that the inspector use normal `.mesh` UI primitives.
- `.planning/REQUIREMENTS.md` - `PROF-01`, `INSP-01`, `INSP-02`, and `INSP-03`, plus the Phase 17/18 boundaries that must not be pulled forward.
- `.planning/ROADMAP.md` - Phase 16 goal, dependency on Phases 14 and 15, and the required inspector views.
- `.planning/STATE.md` - current milestone position and carried-forward project decisions.

### Prior Phase Context That Must Carry Forward
- `.planning/phases/14-profiling-data-model-and-timing-hooks/14-CONTEXT.md` - locked profiling session behavior, bounded rolling storage, and the rule that profiling is separate from overlay visibility.
- `.planning/phases/14-profiling-data-model-and-timing-hooks/14-VERIFICATION.md` - proof that the debug-only profiling contract and shell/per-surface timing model are already in place.
- `.planning/phases/15-backend-and-surface-attribution/15-VERIFICATION.md` - proof that backend provider/stage attribution and stable per-surface rollups now exist for the inspector to consume.
- `.planning/phases/13-navigation-bar-rendering-proof/13-CONTEXT.md` - prior real-surface proof philosophy and existing shell-like surface expectations that influence Phase 17 benchmark anchors.

### Existing Debug and Profiling Runtime
- `crates/core/foundation/debug/src/lib.rs` - canonical `DebugSnapshot`, `DebugTab`, profiling snapshot types, and backend/surface profiling contract.
- `crates/core/shell/src/shell/runtime/debug.rs` - current debug snapshot assembly path and deterministic ordering for profiling data.
- `crates/core/shell/src/shell/runtime/profiling.rs` - bounded runtime profiling collector used by the future inspector.
- `crates/core/shell/src/shell/runtime/request.rs` - debug request handling and profiling toggle request path.
- `crates/core/shell/src/shell/types.rs` - `CoreRequest` definitions and shell component/runtime request boundaries.
- `crates/core/shell/src/shell/ipc.rs` - current shell IPC commands for `shell:debug_overlay`, `shell:debug_profiling`, and `shell:debug_cycle_tab`.
- `crates/core/ui/render/src/surface/debug_overlay.rs` - existing native debug panel implementation that Phase 16 replaces.

### Frontend Module and Surface Patterns
- `modules/frontend/navigation-bar/src/main.mesh` - shipped shell surface pattern with component imports, shell events, and container-query styling.
- `modules/frontend/audio-popover/src/main.mesh` - shipped `.mesh` proof for compact shell UI, service-driven state, and control layout.
- `modules/frontend/navigation-bar/module.json` - current frontend module packaging pattern in the repo.
- `modules/frontend/audio-popover/module.json` - companion frontend module packaging pattern for a shell-owned popover surface.

### Testing and Proof
- `crates/core/shell/src/shell/tests.rs` - existing shell profiling tests and likely home for Phase 16 inspector/debug-path regression coverage.
- `crates/core/shell/src/shell/component/tests.rs` - real-surface shell proof pattern relevant when the inspector becomes a shipped `.mesh` surface.
- `.planning/codebase/CONVENTIONS.md` - `.mesh` file conventions and shell/frontend naming patterns.
- `.planning/codebase/STRUCTURE.md` - module layout and the current boundary between core shell code and frontend modules.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/foundation/debug/src/lib.rs` already exposes the profiling snapshot model Phase 16 needs; the inspector can consume shell, surface, and backend summaries without redefining the contract.
- `crates/core/shell/src/shell/runtime/debug.rs` already builds one deterministic `DebugSnapshot`, which is the natural data source for any debug/profiling inspector endpoints.
- `crates/core/shell/src/shell/ipc.rs` and `crates/core/shell/src/shell/types.rs` already define the current debug entry path and toggle requests, so Phase 16 should extend that flow rather than bypass it.
- `modules/frontend/navigation-bar/src/main.mesh` and `modules/frontend/audio-popover/src/main.mesh` are concrete `.mesh` examples for shipped shell-owned UI modules, component composition, and shell event interactions.
- `modules/frontend/*/module.json` files show the current repo pattern for shipping frontend surfaces as module packages.

### Established Patterns
- The shell already treats profiling as debug-only state and keeps collection separate from visible UI, so the inspector should consume that behavior rather than changing it.
- `.mesh` surfaces are authored as normal frontend modules with component imports, shell event hooks, and theme/container-query styling; the inspector should follow that pattern even if it is core-shipped.
- Real shell proof work prefers shipped surfaces and shell-level tests over one-off native diagnostics UI.
- The current native debug overlay is a right-side panel with view switching, which constrains the replacement inspector to stay compact and navigable even when implemented in `.mesh`.

### Integration Points
- New debug/profiling endpoint exposure will connect through `crates/core/shell/src/shell/runtime/debug.rs`, `crates/core/shell/src/shell/runtime/request.rs`, `crates/core/shell/src/shell/ipc.rs`, and `crates/core/shell/src/shell/types.rs`.
- The `.mesh` inspector module will likely live under `modules/frontend/`-style packaging, but be loaded internally by the shell when debug mode is active.
- The current native overlay renderer in `crates/core/ui/render/src/surface/debug_overlay.rs` is the direct replacement target.
- Regression proof likely needs both shell profiling tests in `crates/core/shell/src/shell/tests.rs` and real-surface/module integration checks in `crates/core/shell/src/shell/component/tests.rs`.

</code_context>

<specifics>
## Specific Ideas

- The built-in inspector should behave like a normal frontend `.mesh` consumer of shell debug data, not like a private native diagnostics view.
- Replacing the native panel is acceptable as long as the right-side debug-panel feel remains familiar.
- The benchmark view should establish the final information architecture early, even if the actual repeatable benchmark flows land in Phase 17.

</specifics>

<deferred>
## Deferred Ideas

- Implement canonical repeatable benchmark launch/proof flows in the inspector; that remains Phase 17 work.
- Add trace capture, replay, or persistence behavior; that remains outside the first live rolling profiler.
- Expand the debug/profiling surface replacement into a broader debug architecture rewrite beyond the inspector surface.

</deferred>

---

*Phase: 16-Debug-Only Profiling Mode and Live Inspector*
*Context gathered: 2026-05-08*
