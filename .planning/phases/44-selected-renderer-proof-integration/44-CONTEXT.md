# Phase 44: Selected Renderer Proof Integration - Context

**Gathered:** 2026-05-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 44 integrates the Phase 43 selected MESH-owned focused-crate path behind a constrained production proof boundary. The proof should adapt existing retained MESH data into focused layout, text, paint, and accessibility evidence while preserving the current renderer and presentation ownership model.

This phase does not replace `mesh-core-render`, replace `mesh-core-presentation`, adopt Blitz directly, introduce Winit as production shell ownership, build a general `.mesh` to HTML/Blitz translator, fix unrelated audio popover transition polish, or design the broad renderer migration plan. Phase 45 owns broad migration planning.

</domain>

<decisions>
## Implementation Decisions

### Selected Path And Boundary

- **D-01:** Phase 44 advances the MESH-owned focused-crate path selected by Phase 43. Blitz remains reference/blocker evidence until the high-level `blitz` crate compile blocker and shell ownership questions are resolved.
- **D-02:** The proof should sit behind existing MESH renderer/presentation ownership. It may add an adapter/proof boundary, but MESH `WidgetNode`/retained node identity, render-object dirty slots, retained display-list concepts, and presentation damage plumbing remain authoritative.
- **D-03:** The first production proof should target a navigation/audio shipped-surface slice and convert retained MESH data into focused-crate evidence. It should not attempt a whole-renderer replacement or a new surface authoring model.

### Preserved Contracts

- **D-04:** MESH stable node IDs remain the identity source across layout, text, paint, interaction, diagnostics, profiling, and accessibility mapping.
- **D-05:** Typed invalidation categories must remain visible through the proof path. At minimum, the integration must preserve existing retained-tree categories and render-object categories for geometry/material/text/accessibility changes.
- **D-06:** Damage and profiling payloads must continue through existing debug/profiling boundaries rather than a new disconnected evidence channel.
- **D-07:** Focused-crate adaptation failures are non-fatal diagnostics. Missing or unsupported adapter data should be observable without crashing the shipped surface.

### Text, Selection, And Accessibility

- **D-08:** Text proof should use the focused path for layout/shaping evidence while keeping current theme-owned selection color behavior authoritative: `color.selection-background` and `color.selection-foreground` stay shell/theme-owned.
- **D-09:** Selection proof must cover geometry and paint behavior through the selected path, not just static text labels.
- **D-10:** Accessibility proof should expose an AccessKit-compatible retained-node update boundary keyed from MESH node IDs. Phase 44 only needs a clear update boundary and tests, not a complete cross-platform accessibility runtime.

### Dependency And Rollout Discipline

- **D-11:** Root workspace dependency adoption should be limited to the focused crates needed for the constrained proof. Taffy, Parley, AnyRender-style paint boundary work, and AccessKit are the preferred candidates from Phase 42/43; Skia/rust-skia remains fallback evidence only if the selected paint boundary cannot satisfy the proof.
- **D-12:** New code should be reversible and locally bounded. Prefer a focused adapter module, proof-only feature, or similarly narrow integration point over spreading crate-specific concepts through shell/component code.
- **D-13:** Existing navigation/audio automated behavior coverage must continue to pass. Phase 44 may add focused regression tests, but should not rewrite shipped surfaces to make the proof easier.

### Folded Todos

None.

### the agent's Discretion

These decisions were selected through the execute-mode fallback because interactive `request_user_input` was unavailable in this runtime. Downstream planners may choose exact file boundaries and feature-gating mechanics, but should preserve the constrained proof scope, focused-crate direction, existing MESH ownership contracts, and shipped-surface behavior requirements above.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase 44 Scope

- `.planning/ROADMAP.md` - Phase 44 goal, dependency on Phase 43, and success criteria for INTG-01 through INTG-04.
- `.planning/REQUIREMENTS.md` - Renderer integration requirements: INTG-01, INTG-02, INTG-03, and INTG-04.
- `.planning/PROJECT.md` - Current v1.8 milestone framing, retained-rendering history, and validated renderer milestones.

### Phase 43 Decision And Evidence

- `.planning/phases/43-comparable-renderer-prototype-proofs/43-CONTEXT.md` - Prototype scope and selected comparison criteria feeding this phase.
- `.planning/phases/43-comparable-renderer-prototype-proofs/43-PROTOTYPE-COMPARISON.md` - Final comparison selecting the MESH-owned focused-crate path and documenting the Blitz blocker.
- `.planning/phases/43-comparable-renderer-prototype-proofs/43-PHASE44-HANDOFF.md` - Required Phase 44 integration boundary, preserved contracts, proof targets, and non-goals.
- `.planning/prototypes/phase43/README.md` - Prototype harness scope, commands, fixtures, evidence outputs, and non-goals.
- `.planning/prototypes/phase43/fixtures/phase43-scenarios.json` - Shared shipped-surface scenarios for navigation/audio proof continuity.
- `.planning/prototypes/phase43/evidence/focused-crate.md` - Focused-crate retained evidence for Taffy layout, Parley text, display-slot paint commands, interactions, and AccessKit mapping.
- `.planning/prototypes/phase43/evidence/blitz-reference.md` - Blitz reference blocker evidence and reproduction command.
- `.planning/prototypes/phase43/output/focused-crate.json` - Structured focused-crate output that Phase 44 can use as proof shape input.

### Current Renderer And Presentation Boundaries

- `.planning/codebase/STACK.md` - Current Rust/Luau/.mesh stack, renderer dependencies, Wayland runtime, and development environment.
- `.planning/codebase/ARCHITECTURE.md` - Shell/frontend/render/presentation architecture, retained rendering boundaries, and anti-patterns.
- `.planning/codebase/INTEGRATIONS.md` - Wayland, IPC, capability, observability, and platform integration context.
- `crates/core/frontend/host/src/lib.rs` - `ShellComponent` trait boundary for rendering, profiling records, invalidation snapshots, and present damage.
- `crates/core/shell/src/shell/component/runtime_tree.rs` - Stable runtime node IDs and retained-tree dirty categories.
- `crates/core/frontend/render/src/render_object.rs` - Retained render-object tree and dirty slots for transform, clip, opacity, geometry, material, text, and accessibility.
- `crates/core/frontend/render/src/display_list.rs` - Retained display-list keys, damage rectangles, repaint policy, batching metrics, and selection payload handling.
- `crates/core/frontend/render/src/lib.rs` - Public render crate exports for display list, render object tree, surface painter, text, and profiling metrics.
- `crates/core/presentation/src/lib.rs` - Current presentation boundary and damage-aware present path.
- `crates/core/presentation/src/wayland_surface/backend.rs` - Wayland damage copy/attach behavior that production proof must not bypass accidentally.

### Shipped Surface And Regression Coverage

- `modules/frontend/navigation-bar/src/main.mesh` - Required navigation bar surface behavior and source structure.
- `modules/frontend/audio-popover/src/main.mesh` - Required audio popover slider, mute, button, and open-close behavior source structure.
- `crates/core/shell/src/shell/component/tests/integration/real_surfaces.rs` - Existing real shipped-surface integration coverage.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` - Existing keyboard/navigation behavior coverage.
- `crates/core/shell/src/shell/component/tests/invalidation/profiling.rs` - Existing profiling/invalidation proof patterns for shipped interaction scenarios.
- `crates/core/shell/src/shell/component/tests/restyle/selection.rs` - Existing selection restyle/geometry behavior coverage.
- `crates/core/frontend/render/src/surface/painter/tests.rs` - Existing paint-level selection color and display-list painter coverage.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- `RetainedWidgetTree` in `crates/core/shell/src/shell/component/runtime_tree.rs` already assigns deterministic stable node IDs and tracks retained dirty categories for layout, style, attributes, children, and state.
- `RenderObjectTree` in `crates/core/frontend/render/src/render_object.rs` already synchronizes retained render objects and tracks dirty slots for geometry, material, text, and accessibility.
- `RetainedDisplayList` in `crates/core/frontend/render/src/display_list.rs` already owns paint-command keys, display primitive slots, damage metrics, repaint policy, batching metrics, and selection payloads.
- `ShellComponent` in `crates/core/frontend/host/src/lib.rs` already exposes profiling records, invalidation snapshots, and present damage, making it the right outer contract to preserve.
- Existing navigation/audio sources and tests provide the shipped-surface behavior guardrails for the proof.

### Established Patterns

- Rust core stays generic; renderer proof work should not introduce service-specific audio branches.
- MESH measures renderer work through retained invalidation, damage, profiling, diagnostics, and shipped-surface behavior rather than standalone demos.
- Presentation remains Wayland/layer-shell oriented with a dev-window fallback. Production proof work should not move shell ownership into Winit or Blitz.
- Selection colors are theme-owned and injected through `_mesh_selection_*` attributes before paint; focused text proof must preserve that authority.

### Integration Points

- A narrow adapter can be placed near the render-object/display-list boundary so focused crates consume MESH retained data while existing shell/component and presentation contracts stay stable.
- Tests should reuse the existing component test modules for shipped-surface behavior, invalidation/profiling payloads, and selection color/geometry proof.
- Diagnostics should flow through existing component/runtime diagnostics paths when focused adaptation cannot represent a retained node or property.

</code_context>

<specifics>
## Specific Ideas

- Treat the Phase 43 focused-crate JSON output as the shape to productize: retained node ID in, layout/text/paint/accessibility evidence out.
- Keep proof output debuggable in the same places developers already inspect retained rendering: invalidation snapshots, profiling rows, damage metrics, and non-fatal diagnostics.
- Prefer proving one real end-to-end retained slice over broad adapter coverage. The required slice is navigation/audio behavior plus text selection and AccessKit-compatible retained-node updates.

</specifics>

<deferred>
## Deferred Ideas

### Reviewed Todos (not folded)

- `Audio Popover Transition Delay Polish` (`.planning/todos/pending/2026-05-13-phase31-audio-popover-transition-delay.md`) - reviewed because Phase 44 preserves navigation/audio shipped-surface behavior. Not folded because it is accepted polish debt about surface transition lifecycle, not part of the renderer proof integration unless a planned regression test naturally observes it.

</deferred>

---

*Phase: 44-Selected Renderer Proof Integration*
*Context gathered: 2026-05-18*
