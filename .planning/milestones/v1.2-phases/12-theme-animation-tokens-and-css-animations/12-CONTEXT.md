# Phase 12: Theme Animation Tokens and CSS Animations - Context

**Gathered:** 2026-05-08
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase adds theme-owned animation tokens and custom CSS keyframe animation support for MESH shell UI. It should make animation authoring feel like practical CSS for shell surfaces while preserving the milestone boundary: predictable software-rendered animation over the visual properties MESH already knows how to interpolate.

This phase covers:
- Renaming theme motion tokens from `motion.*` to `animation.*`.
- Separating primitive animation tokens from shell default animation recipes.
- Allowing CSS animation and transition declarations to reference theme animation tokens.
- Supporting custom `@keyframes` rules for the existing transition-safe animatable property set.
- Scheduling keyframe animations across frames, preserving active animations across normal reactive rebuilds, and honoring iteration, direction, fill mode, and play state.
- Producing clear compile and runtime diagnostics for invalid animation declarations, unsupported keyframes, unresolved animation names, and invalid token references.

This phase does not add full browser animation compatibility, GPU transform/filter/compositing, broader CSS at-rule support, full navigation-bar migration proof, or package/module manifest redesign.

</domain>

<decisions>
## Implementation Decisions

### Motion Token Shape
- **D-01:** The canonical animation namespace is `animation.*`, not `motion.*`.
- **D-02:** Existing `motion.*` theme tokens should be hard-renamed to `animation.*`; Phase 12 should not preserve `motion.*` as a long-term alias.
- **D-03:** Themes should keep flat animation tokens rather than introduce nested structured preset objects in the first release.
- **D-04:** Primitive animation token values and shell default animation recipes must be separate concepts.
- **D-05:** Primitive tokens include values such as `animation.duration.fast` and `animation.curves.bezier.one`.
- **D-06:** Shell default animation recipes live under names such as `animation.default.border-radius`.
- **D-07:** Default animation recipes compose primitive values with explicit `token(...)` references. Example: `animation.default.border-radius: "border-radius token(animation.duration.fast) token(animation.curves.bezier.one)"`.
- **D-08:** Component CSS should continue using explicit `token(...)` syntax for animation token references.

### Keyframe Scope
- **D-09:** Custom `@keyframes` should support the same broad transition-safe visual property set that MESH already interpolates for transitions.
- **D-10:** The supported keyframe property set includes the current transition-safe properties such as color, background color, border color, border radius, border width, opacity, width, height, min/max dimensions, padding, margin, transform, font size, letter spacing, line height, gap, and insets.
- **D-11:** Keyframes support percentage stops, including intermediate percentages.
- **D-12:** The first release supports percentages only; it does not accept `from` or `to` aliases.
- **D-13:** Keyframe blocks define property values only. Animation properties such as `animation-duration`, `animation-delay`, and `animation-timing-function` may reference `token(animation.*)`.
- **D-14:** Keyframe stop values do not need theme token references in the first release.

### Animation Runtime Rules
- **D-15:** Keyframe animations continue across restyles and reactive tree rebuilds when the element's stable `_mesh_key` and animation name remain present.
- **D-16:** A rebuild should not restart a running keyframe animation just because the component re-rendered.
- **D-17:** Completed animations respect `animation-fill-mode`, including `none`, `forwards`, `backwards`, and `both`.
- **D-18:** Phase 12 supports both finite numeric iteration counts and `infinite`.
- **D-19:** `animation-play-state: paused` is supported. Paused animations freeze the current frame and keep their animation state.
- **D-20:** Existing transition scheduling should remain compatible with Phase 9's stable runtime-state model and Phase 11's focus/keyboard rebuild behavior.

### Diagnostics Boundary
- **D-21:** Keyframe validation is strict. If a `@keyframes` block contains unsupported properties, reject the entire block with a diagnostic rather than partially running it.
- **D-22:** If a syntactically valid `@keyframes` block contains no properties from the supported transition-safe animation set, reject or diagnose it as non-runnable and do not register the animation.
- **D-23:** Diagnostics should surface at both compile/parse time and runtime.
- **D-24:** Compile/parse diagnostics catch static keyframe syntax and unsupported-property issues.
- **D-25:** Runtime surface diagnostics catch unresolved animation names, invalid runtime references, and token-resolution failures that depend on resolved theme/style state.
- **D-26:** Invalid animation token references fail hard with diagnostics. Example: `token(animation.duration.fastest)` should prevent that animation declaration from running instead of silently falling back.

### the agent's Discretion
- Planner/researcher may choose the exact Rust representation for keyframe rules, stops, and active animation state, as long as the locked percentage-only syntax and strict diagnostics are preserved.
- Planner/researcher may choose where to share or migrate interpolation math between `mesh-core-shell` and `mesh-core-render`, as long as shell-owned runtime identity still uses stable `_mesh_key`.
- Planner/researcher may choose the exact animation-frame scheduling mechanism, as long as active animations mark surfaces dirty while running and stop producing redraw churn after completion.
- Planner/researcher may choose the exact diagnostic data structures, as long as diagnostics are visible to authors and distinguish compile-time parse errors from runtime unresolved references.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone and Prior Phase Context
- `.planning/PROJECT.md` - v1.2 milestone boundary and practical shell UI renderer goal.
- `.planning/REQUIREMENTS.md` - `ANIM-01` through `ANIM-05`, plus out-of-scope browser/GPU animation boundaries.
- `.planning/ROADMAP.md` - Phase 12 goal, success criteria, and dependencies on Phase 8 and Phase 9.
- `.planning/STATE.md` - current milestone position and carried-forward project decisions.
- `.planning/phases/09-responsive-and-interaction-reactivity/09-CONTEXT.md` - stable `_mesh_key` runtime state, restyle/rebuild preservation, and animation deferral to Phase 12.
- `.planning/phases/10-selectable-text-and-clipboard-copy/10-CONTEXT.md` - selection/keyboard coexistence boundaries that animations must not regress.
- `.planning/phases/11-keyboard-navigation-and-shortcuts/11-CONTEXT.md` - focus-visible, keyboard, and navigation-bar boundaries that animations must preserve.

### Theme and Authoring Docs
- `config/themes/mesh-default-dark.json` - existing `motion.*` tokens that Phase 12 should hard-rename to `animation.*`.
- `config/themes/mesh-default-light.json` - light-theme counterpart to keep in sync with the dark theme token rename.
- `docs/css-coverage.md` - current transition/animation metadata documentation and diagnostics boundary.
- `docs/theming/themes.md` - theme token documentation and the current motion-token namespace.
- `docs/frontend/mesh-syntax.md` - author-facing CSS examples and transition documentation.

### Style Parsing and Resolution
- `crates/core/ui/component/src/parser.rs` - current `@keyframes` rejection test; Phase 12 should replace this boundary with strict supported keyframe parsing.
- `crates/core/ui/component/src/parser/styles.rs` - style parser and at-rule handling for custom keyframes.
- `crates/core/ui/elements/src/style/types.rs` - `TransitionStyle`, `AnimationStyle`, iteration count, direction, fill mode, play state, and supported CSS property list.
- `crates/core/ui/elements/src/style/parse.rs` - current transition/animation shorthand parsing; note that existing keyframe skeleton mentions `from`/`to`, but Phase 12 locks percentages only.
- `crates/core/ui/elements/src/style/resolve.rs` - style resolution and token/variable handling for transition and animation properties.

### Runtime Animation and Rendering
- `crates/core/shell/src/shell/component.rs` - `FrontendSurfaceComponent` state ownership, including `style_animations`.
- `crates/core/shell/src/shell/component/animation.rs` - current working transition animation implementation using `_mesh_key`, previous styles, dirty marking, easing, and interpolation.
- `crates/core/shell/src/shell/component/shell_component.rs` - shell component paint/dirty behavior and active animation repaint loop.
- `crates/core/ui/render/src/animation/mod.rs` - renderer-side animation module skeleton and intended home for shared animation primitives.
- `crates/core/ui/render/src/animation/keyframes.rs` - keyframe registry/active animation skeleton; planning must align this with the locked percentage-only and strict-diagnostic decisions.
- `crates/core/ui/render/src/animation/easing.rs` - easing primitives used by transition/keyframe playback.
- `crates/core/ui/render/src/animation/interpolate.rs` - interpolation trait used by animated visual values.
- `crates/core/ui/render/src/animation/transition.rs` - renderer-side transition/animatable style primitives.
- `crates/core/ui/render/src/animation/transform.rs` - transform interpolation and paintability boundaries.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `FrontendSurfaceComponent` already stores `style_animations: HashMap<String, StyleAnimation>` keyed by stable `_mesh_key`, giving Phase 12 a proven identity model for animations across rebuilds.
- `crates/core/shell/src/shell/component/animation.rs` already interpolates the broad transition-safe visual property set and marks the component dirty while animations remain active.
- `AnimationStyle` already stores animation name, duration, delay, easing, iteration count, direction, fill mode, and play state metadata.
- `config/themes/mesh-default-dark.json` and `config/themes/mesh-default-light.json` already contain duration, easing, scale, opacity, and default transition tokens under `motion.*`, providing concrete migration fixtures for `animation.*`.
- `crates/core/ui/render/src/animation/keyframes.rs` already sketches `KeyframeRule`, `KeyframeStop`, `KeyframeRegistry`, and `ActiveKeyframeAnimation`, though the TODO text must be corrected to match this context's percentage-only decision.

### Established Patterns
- Theme tokens are flat dot-separated keys resolved through `token(...)`; Phase 12 should extend that model rather than inventing a separate nested animation schema.
- Parser/resolver/render boundaries from Phase 8 still apply: component parser handles CSS syntax, elements style resolution computes metadata, shell/render animation runtime applies visual changes.
- Runtime state that survives rebuilt frontend trees should be keyed independently of transient node IDs. Phase 9 made stable `_mesh_key` the interaction-state authority, and Phase 12 should follow it for active animations.
- Existing animation work is software-rendered and dirty-frame driven. Active animations should request frames only while needed and stop dirty churn when complete or removed.
- Diagnostics should be author-visible rather than silent. This follows Phase 8's unsupported-property behavior and the ANIM-05 milestone requirement.

### Integration Points
- Rename theme token data and documentation from `motion.*` to `animation.*` in `config/themes/*.json`, `docs/theming/themes.md`, `docs/css-coverage.md`, and `docs/frontend/mesh-syntax.md`.
- Extend `crates/core/ui/component/src/parser/styles.rs` and parser tests so `@keyframes <name> { <percent>% { ... } }` is accepted and unsupported forms are rejected diagnostically.
- Extend style resolution in `crates/core/ui/elements/src/style/resolve.rs` so animation properties can resolve `token(animation.*)` references and fail hard on invalid animation token references.
- Implement keyframe parsing/playback around `crates/core/ui/render/src/animation/keyframes.rs`, reusing interpolation/easing primitives from `crates/core/ui/render/src/animation/`.
- Integrate active keyframe playback with `FrontendSurfaceComponent` in `crates/core/shell/src/shell/component/animation.rs` or migrate shared logic into `mesh-core-render` while preserving shell repaint and dirty semantics.
- Emit runtime diagnostics for unresolved animation names and theme token resolution failures through the existing component/surface diagnostics path.

</code_context>

<specifics>
## Specific Ideas

- Prefer `animation.*` over `motion.*` because the phase is about theme animation tokens and CSS animations.
- Use primitive animation tokens such as `animation.duration.fast` and `animation.curves.bezier.one`.
- Use default animation recipe tokens such as `animation.default.border-radius`.
- Compose default recipes using explicit `token(...)` references, for example: `border-radius token(animation.duration.fast) token(animation.curves.bezier.one)`.
- The keyframe parser should support percentage stops only; do not support `from` or `to` aliases in the first release.
- Keyframe validation should be strict rather than partially permissive.

</specifics>

<deferred>
## Deferred Ideas

- Full browser-compatible CSS animation behavior remains out of scope.
- GPU transform/filter/compositing animation remains out of scope.
- Token references inside keyframe stop values are deferred beyond the first release.
- `from` and `to` aliases for keyframes are deferred beyond the first release.
- Full navigation-bar migration and proof remain Phase 13.

### Reviewed Todos (not folded)
- `Create unified package and module manifest phase` - reviewed during Phase 12 cross-reference, but not folded because the user explicitly requested it as a separate future phase about package/module manifest structure, module management, icon pack installation, and interface declarations.

</deferred>

---

*Phase: 12-Theme Animation Tokens and CSS Animations*
*Context gathered: 2026-05-08*
