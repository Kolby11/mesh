# Phase 43: Comparable Renderer Prototype Proofs - Context

**Gathered:** 2026-05-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 43 builds comparable throwaway renderer prototypes for the two paths selected by Phase 42: a Blitz reference path and a MESH-owned focused-crate path. Both prototypes must cover the same shipped-surface slice: navigation bar and audio popover, with visual output plus hover, click, slider, and open-close interaction shape.

This phase proves comparable evidence only. It does not replace the production renderer, wire a production render path, implement real backend runtime behavior, or restore full diagnostics/profiling payloads. Phase 44 owns selected-path integration.

</domain>

<decisions>
## Implementation Decisions

### Prototype Fidelity

- **D-01:** Prototype comparison targets structural and behavioral parity, not pixel-perfect production fidelity. The prototypes should make layout, control shape, text/icon presence, and interaction flow comparable enough to choose a Phase 44 direction.
- **D-02:** Visual evidence should include deterministic snapshots or screenshot-like outputs when feasible, but a prototype may pass with structured render/layout output if the path cannot render pixels without exceeding throwaway scope.
- **D-03:** Interaction evidence must cover hover, click, slider movement/release, and audio popover open-close behavior for both prototype paths. It does not need real backend audio state.

### Shared Inputs

- **D-04:** Both prototypes should use the same scenario fixture set: navigation bar baseline, navigation bar audio-trigger interaction, audio popover visible state, audio popover slider interaction, and audio popover close path.
- **D-05:** The MESH-owned focused-crate prototype should render from retained MESH-shaped data: stable node identity, layout/style/text/icon data, and display-list-like paint commands or an equivalent intermediate fixture.
- **D-06:** The Blitz reference prototype may use an HTML/CSS-equivalent fixture for the same two surfaces instead of ingesting `.mesh` directly. Direct `.mesh` to Blitz translation is out of scope unless it is the cheapest way to produce comparable evidence.

### Blocker Threshold

- **D-07:** If Blitz cannot render the required surface slice inside throwaway scope, that is acceptable only when the blocker is concrete and reproducible: exact attempted harness, crate/API boundary, error or mismatch, and why it blocks direct/reference evidence.
- **D-08:** Do not spend the phase forcing Blitz through production Wayland/layer-shell integration. Blitz direct adoption remains behind Phase 42's Wayland shell model and browser-engine-level overhead blockers.
- **D-09:** The focused-crate path should prefer the Phase 42 candidates: Taffy for layout, Parley for text, AnyRender for paint abstraction, and AccessKit for retained-node accessibility boundary notes. Skia/rust-skia remains fallback evidence only.

### Comparison Output

- **D-10:** The final comparison should use the same headings for both paths: visual/layout fidelity, interaction shape, retained identity fit, accessibility boundary, build/dependency cost, blocker evidence, and Phase 44 integration readiness.
- **D-11:** Phase 43 should identify the path that advances to Phase 44, but it should not design the full migration plan. Phase 45 owns broad migration planning.

### Folded Todos

None.

### the agent's Discretion

These decisions were selected through the execute-mode fallback because interactive `request_user_input` was unavailable in this runtime. Downstream planners may adjust implementation mechanics if they preserve the locked Phase 42 scope, the two-surface comparison, and throwaway-harness constraint.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase 43 Scope

- `.planning/ROADMAP.md` — Phase 43 goal, dependencies, and success criteria.
- `.planning/REQUIREMENTS.md` — PROTO-01, PROTO-02, and PROTO-03 requirements.
- `.planning/PROJECT.md` — Current v1.8 milestone framing and validated renderer history.

### Phase 42 Decision Inputs

- `.planning/phases/42-renderer-architecture-decision-matrix/42-DECISION-MATRIX.md` — Source-backed candidate outcomes, hard blockers, path scores, and final dual-prototype verdict.
- `.planning/phases/42-renderer-architecture-decision-matrix/42-PHASE43-HANDOFF.md` — Prototype paths, required surfaces, required interactions, non-goals, and scope guard.
- `.planning/phases/42-renderer-architecture-decision-matrix/42-SOURCE-INVENTORY.md` — Local and external source evidence for crate/path decisions.
- `.planning/phases/42-renderer-architecture-decision-matrix/42-VERIFICATION.md` — Phase 42 verification result and requirement coverage.

### Existing Surface Inputs

- `modules/frontend/navigation-bar/src/main.mesh` — Required navigation bar surface behavior and source structure.
- `modules/frontend/audio-popover/src/main.mesh` — Required audio popover slider, mute, and open-close behavior source structure.

### Renderer Architecture Context

- `.planning/codebase/STACK.md` — Current Rust/Luau/.mesh stack, renderer dependencies, Wayland runtime, and development environment.
- `.planning/codebase/ARCHITECTURE.md` — Shell/frontend/render/presentation architecture and anti-patterns.
- `.planning/codebase/INTEGRATIONS.md` — Wayland, IPC, capability, observability, and platform integration context.
- `crates/core/frontend/render/src/display_list.rs` — Current retained display-list, damage, batching, and paint-command boundary.
- `crates/core/frontend/render/src/render_object.rs` — Retained render-object dirty slots, including accessibility.
- `crates/core/presentation/src/lib.rs` — Current dev-window/layer-shell presentation boundary and damage-aware present path.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- `modules/frontend/navigation-bar/src/main.mesh` — Source for the navigation bar fixture: status text, control cluster, volume trigger, theme/settings controls, and embedded audio popover portal.
- `modules/frontend/audio-popover/src/main.mesh` — Source for the audio popover fixture: header/icon state, volume slider, mute button, volume up/down buttons, and exiting opacity class.
- `crates/core/frontend/render/src/display_list.rs` — Reusable model for retained paint commands, damage rectangles, repaint policy, and batching metrics.
- `crates/core/frontend/render/src/render_object.rs` — Reusable model for stable node identity and dirty categories that the focused-crate prototype should preserve.
- `crates/core/presentation/src/lib.rs` — Reference for why Winit/Blitz shell ownership should stay harness-only in Phase 43.

### Established Patterns

- MESH keeps Rust core generic and routes shell behavior through retained frontend/runtime structures, not service-specific renderer branches.
- Current presentation is Wayland/layer-shell oriented with a dev-window fallback; generic app-window assumptions must stay isolated to throwaway harnesses.
- Existing renderer progress is measured through retained invalidation, display-list damage, profiling, diagnostics, and shipped-surface behavior rather than synthetic demos alone.
- Frontend authored surfaces are `.mesh` components, but Phase 43 does not need to build a general `.mesh` to Blitz translator.

### Integration Points

- Prototype artifacts should live under Phase 43 planning/spike scope or isolated crate/example paths chosen by the planner; they must not be production-wired into `mesh-core-render` or `mesh-core-presentation`.
- If the focused-crate prototype needs real code, the safest boundary is an isolated harness that consumes MESH-shaped retained fixtures and emits comparable layout/paint/accessibility output.
- If the Blitz path needs a window/event loop, Winit is acceptable for the throwaway harness only.

</code_context>

<specifics>
## Specific Ideas

- Use both required surfaces, not one. Navigation bar proves shell-strip layout and control click/hover behavior; audio popover proves popover open-close behavior and slider interaction.
- Prefer comparable evidence over maximal implementation depth. A concrete Blitz blocker is a valid outcome if it is source-backed and reproducible.
- Keep the final Phase 43 comparison directly useful for Phase 44: which path should integrate, which existing MESH boundaries it preserves, and which risks remain.

</specifics>

<deferred>
## Deferred Ideas

### Reviewed Todos (not folded)

- `Audio Popover Transition Delay Polish` (`.planning/todos/pending/2026-05-13-phase31-audio-popover-transition-delay.md`) — reviewed because it matched the Phase 43 audio popover/open-close surface. Not folded into Phase 43 as a fix item; it remains accepted polish debt. Phase 43 may record whether prototype open-close evidence naturally illuminates this delay, but must not spend scope implementing a shell-owned transition lifecycle.

</deferred>

---

*Phase: 43-Comparable Renderer Prototype Proofs*
*Context gathered: 2026-05-18*
