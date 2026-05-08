# Phase 13: Navigation-Bar Rendering Proof - Context

**Gathered:** 2026-05-08
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase applies the completed rendering-system upgrade to the shipped `@mesh/navigation-bar` surface so the milestone is proven on a real shell component rather than only on engine fixtures.

This phase covers:
- Migrating the existing navigation bar from a compact control strip into a richer proof-focused shell surface.
- Using the expanded CSS subset for layout, spacing, typography, visual states, and responsive behavior in the shipped navigation bar.
- Proving existing keyboard, focus-visible, selection, and animation capabilities on the navigation bar itself.
- Adding automated proof around real-surface behavior plus at least one constrained-width case.

This phase does not add new renderer capabilities, new shell feature domains, or new service/product surfaces beyond what is needed to prove the finished v1.2 rendering contract on the existing navigation bar.

</domain>

<decisions>
## Implementation Decisions

### Proof Surface Shape
- **D-01:** Phase 13 should turn the navigation bar into a richer proof-focused surface rather than keeping it as a minimal compact control strip.
- **D-02:** The richer proof should still feel like shell chrome, not a dashboard or a new application surface.
- **D-03:** The main content model should be one compact status cluster plus controls, not several independent labeled sections spread across the entire bar.
- **D-04:** The proof should add visible text/status copy to the navigation bar itself so selection, responsive layout, and typography are demonstrated on the primary surface.

### Responsive Behavior
- **D-05:** When width gets tight, secondary status text should compress or disappear before the main interactive controls do.
- **D-06:** Phase 13 should preserve control availability under constrained widths rather than hiding core controls first.
- **D-07:** Responsive behavior should be intentional and tested, not just a side effect of reduced spacing.

### Animation Proof
- **D-08:** The navigation bar should use subtle token-driven transitions broadly across the surface.
- **D-09:** Phase 13 should include one clear custom keyframe moment so the proof visibly demonstrates Phase 12 keyframe support, not just transition metadata.
- **D-10:** The custom keyframe proof should be deliberate and restrained rather than turning the whole bar into an atmospheric animated scene.

### Test and Proof Strategy
- **D-11:** `NAV-05` should be satisfied primarily with real-surface shell tests against the shipped navigation bar.
- **D-12:** Phase 13 should add one focused constrained-width proof so responsive behavior is explicitly covered.
- **D-13:** The phase should avoid relying on docs-only or manual-only proof for the navigation-bar migration.

### Scope Guardrails
- **D-14:** Phase 13 is a proof/migration phase, not a new shell-feature expansion phase.
- **D-15:** Reorganizing the existing navigation-bar layout is in scope when it serves the proof surface.
- **D-16:** New feature ideas raised during discussion such as a clock, theme dropdown, expanded volume mixer, and battery-status popover are deferred to future phases rather than folded into Phase 13.

### the agent's Discretion
- Planner/researcher may choose the exact visual arrangement of the compact status cluster and the control cluster, as long as the bar remains shell-like and richer than the current compact strip.
- Planner/researcher may choose the exact responsive text-compression strategy, as long as it preserves interactive controls before secondary copy.
- Planner/researcher may choose which single element or state transition carries the explicit keyframe proof, as long as the bar otherwise stays restrained.
- Planner/researcher may choose the exact automated test shape for the constrained-width proof, as long as it complements the real-surface shell tests rather than replacing them.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone and Phase Scope
- `.planning/PROJECT.md` - v1.2 milestone framing and the requirement that navigation-bar migration acts as the milestone proof.
- `.planning/REQUIREMENTS.md` - `NAV-01` through `NAV-05`, plus the out-of-scope boundary against broader shell rewrites.
- `.planning/ROADMAP.md` - Phase 13 goal, success criteria, and dependency on Phases 10, 11, and 12.
- `.planning/STATE.md` - current milestone position and carried-forward decisions.

### Prior Phase Context That Must Carry Forward
- `.planning/phases/10-selectable-text-and-clipboard-copy/10-CONTEXT.md` - selectable-text scope is still narrow and opt-in; selection belongs only where appropriate.
- `.planning/phases/11-keyboard-navigation-and-shortcuts/11-CONTEXT.md` - locked keyboard traversal, focus-visible, and surface-shortcut rules that the nav bar must now prove.
- `.planning/phases/12-theme-animation-tokens-and-css-animations/12-CONTEXT.md` - locked `animation.*` token contract, percentage-only keyframes, and strict animation diagnostics that Phase 13 must showcase rather than redefine.

### Navigation-Bar Surface and Settings
- `modules/frontend/navigation-bar/src/main.mesh` - top-level navigation bar layout, existing audio popover integration, and current surface styles.
- `modules/frontend/navigation-bar/src/components/volume-button.mesh` - current strongest proof control for hover/focus/active transitions and audio affordance.
- `modules/frontend/navigation-bar/src/components/settings-button.mesh` - shipped navigation control candidate for right-side placement and responsive clustering.
- `modules/frontend/navigation-bar/src/components/theme-button.mesh` - shipped theme control candidate for the animation and control proof surface.
- `modules/frontend/navigation-bar/config/settings.json` - surface keyboard mode and focused-surface shortcut defaults currently shipped with the navigation bar.
- `modules/frontend/navigation-bar/module.json` - navigation-bar settings schema, accessibility role, and capability boundary.

### Authoring and Styling Contracts
- `docs/css-coverage.md` - supported CSS, responsive behavior, transitions, and keyframe boundary that the proof surface should exercise.
- `docs/frontend/mesh-syntax.md` - current author-facing guidance for selectors, container queries, keyboard handlers, selection, and animation syntax.
- `docs/theming/themes.md` - theme token model, including `animation.*` primitives and default animation recipe structure.
- `config/themes/mesh-default-dark.json` - canonical dark-theme tokens and default animation recipes the bar will consume.
- `config/themes/mesh-default-light.json` - canonical light-theme counterpart that should stay in sync with the proof surface.

### Runtime and Test Integration Points
- `crates/core/shell/src/shell/component/tests.rs` - existing real-surface shell proof pattern and the likely home for Phase 13 proof tests.
- `.planning/codebase/TESTING.md` - test strategy conventions for real-surface shell tests and constrained-width proof additions.
- `.planning/codebase/CONVENTIONS.md` - `.mesh` component conventions and test naming/style patterns relevant to the phase.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `modules/frontend/navigation-bar/src/main.mesh` already owns the shipped surface structure and audio-popover integration, so Phase 13 can migrate a real surface rather than introducing a new proof fixture.
- `modules/frontend/navigation-bar/src/components/volume-button.mesh`, `settings-button.mesh`, and `theme-button.mesh` already prove button-level hover/focus/active styling and give the phase concrete controls to preserve while enriching the layout.
- `modules/frontend/navigation-bar/config/settings.json` already enables `keyboard_mode: "on_demand"` and a surface shortcut, so the nav bar is already positioned as the real proof surface for Phase 11 behaviors.
- `crates/core/shell/src/shell/component/tests.rs` already contains real-surface shell tests, which is the strongest existing pattern for `NAV-05`.

### Established Patterns
- Prior phases already locked the renderer/interaction contract; this phase should consume those capabilities on a shipped module rather than changing engine semantics.
- `.mesh` surface files stay compact and componentized: top-level surface in `main.mesh`, local reusable pieces in `src/components/`.
- Shell proof work prefers real-surface tests over synthetic abstractions when the milestone is about shipped behavior.
- Current container-query usage in the nav bar already establishes a responsive hook, so Phase 13 should extend a real pattern instead of introducing a different responsive system.

### Integration Points
- Main layout migration will happen in `modules/frontend/navigation-bar/src/main.mesh`.
- Control-cluster adjustments will likely touch `modules/frontend/navigation-bar/src/components/*.mesh`.
- Responsive behavior should stay expressed through the navigation bar's own CSS/container-query rules.
- Automated proof should primarily extend `crates/core/shell/src/shell/component/tests.rs`, with one focused constrained-width proof layered on top.

</code_context>

<specifics>
## Specific Ideas

- Keep the surface shell-like: one compact status cluster plus controls, not a wide dashboard.
- Use the main navigation bar itself as the place where visible text/status copy lives, so selection and responsive proof happen on the primary shipped surface.
- Preserve controls under constrained width and degrade secondary copy first.
- Use subtle animation tokens pervasively, but choose one clear keyframe-driven moment as the explicit Phase 12 proof.

</specifics>

<deferred>
## Deferred Ideas

- Add a clock component with configurable `hh/mm` or `hh/mm/ss` display and hover date.
- Change the theme button into a dropdown selection from available themes.
- Expand the volume mixer into a broader feature surface.
- Add battery status with a hover popover and battery statistics.

These are valid future capabilities, but they are outside the current proof/migration scope of Phase 13.

</deferred>

---

*Phase: 13-Navigation-Bar Rendering Proof*
*Context gathered: 2026-05-08*
