---
phase: 13
slug: navigation-bar-rendering-proof
status: complete
created: 2026-05-08
requirements:
  - NAV-01
  - NAV-02
  - NAV-03
  - NAV-04
  - NAV-05
---

# Phase 13 Research: Navigation-Bar Rendering Proof

## Research Complete

Phase 13 is a shipped-surface migration phase, not an engine-expansion phase. The renderer, selection, keyboard, and animation capabilities already exist from Phases 10, 11, and 12. The work now is to apply those contracts coherently to `@mesh/navigation-bar` so the milestone is proven on a real shell surface with visible status copy, intentional responsive compression, and real-surface tests.

## Current State

### Shipped Navigation-Bar Surface

- `modules/frontend/navigation-bar/src/main.mesh` is still a sparse control strip: one `.meta` row containing `SettingsButton`, `VolumeButton`, and `ThemeButton`, plus the `AudioPopover`.
- The stylesheet in `main.mesh` already contains unused richer-surface classes such as `.brand`, `.brand-copy`, `.links`, `.link`, `.active`, and `.focus-diagnostic`. This indicates a prior or partial richer-shell direction that can be repurposed rather than inventing a wholly new styling language.
- `modules/frontend/navigation-bar/src/components/meta-label.mesh` and `meta-pill.mesh` already provide compact presentational text primitives, but they are not wired into the shipped `main.mesh`.
- `modules/frontend/navigation-bar/src/components/battery-button.mesh` already implements a passive battery-status widget with live service bindings, visible text, tooltips, and compact responsive behavior. It is dormant, not currently mounted.
- `modules/frontend/navigation-bar/src/components/battery-widget.mesh` is a deprecated reference-only component and should not be the primary implementation target.

### Existing Control Behavior

- `settings-button.mesh`, `theme-button.mesh`, and `volume-button.mesh` already implement the baseline compact control footprint with hover/focus/active transitions and `40px` sizing.
- `theme-button.mesh` already provides a small glyph rotation on hover, which is useful as a precedent for restrained motion but is not by itself a sufficient Phase 13 custom-keyframe proof.
- `volume-button.mesh` already derives audio tooltip copy from the audio service and is the strongest existing status-aware control.
- `modules/frontend/navigation-bar/config/settings.json` already sets `keyboard_mode: "on_demand"` and a surface shortcut `m` that targets the volume button, so the nav bar is already configured to prove Phase 11 behavior on a real surface.

### Existing Real-Surface Test Coverage

- `crates/core/shell/src/shell/component/tests.rs` already contains the strongest proof pattern for this phase:
  - `navigation_bar_keyboard_shortcut_and_theme_activation_work_on_real_surface`
  - `navigation_bar_pointer_click_updates_real_surface_focus_diagnostic`
  - `navigation_bar_keyboard_activation_opens_volume_surface_on_real_surface`
  - `navigation_bar_keyboard_audio_popover_slider_responds_to_arrow_keys`
- The same test module also contains `container_size_restyle_preserves_runtime_and_local_state`, which is the clearest local analog for a constrained-width proof that exercises container queries and runtime-state preservation without introducing a new test harness.

### Renderer And Authoring Contracts Already Available

- `docs/css-coverage.md` confirms that the nav bar can rely on practical layout, typography, gap, padding, container queries, transitions, and percentage-only `@keyframes`.
- `docs/frontend/mesh-syntax.md` documents `selectable="true"` for passive text and explicitly positions it for status copy and proof surfaces.
- Phase 10 locks selection as narrow and opt-in only. Interactive controls should remain non-selectable.
- Phase 11 locks `:focus-visible`, Tab traversal, control activation defaults, and focused-surface shortcut precedence.
- Phase 12 locks `animation.*` tokens and requires at least one real custom keyframe proof, not just transition metadata.

## Implementation Strategy

### Recommended Proof Shape

1. Migrate `main.mesh` from a control-only strip to a richer shell bar with two clear clusters:
   - a compact passive status cluster that can host visible copy and opt-in selection
   - a control cluster that keeps the existing navigation-bar controls available under compression
2. Reuse existing navigation-bar component assets where possible:
   - continue to use `SettingsButton`, `VolumeButton`, and `ThemeButton`
   - optionally mount the existing `BatteryButton` as a passive shell-status/control-adjacent proof element because it already exists in the module
3. Preserve the deferred-scope boundary:
   - no clock
   - no theme dropdown
   - no battery popover/stats expansion
   - no broader mixer redesign beyond what is necessary for the proof surface
4. Put selection proof directly on passive status copy in the main bar using `selectable="true"` and avoid making the control glyphs or button labels selectable.
5. Use subtle token-driven transitions across the bar, then add exactly one bounded custom keyframe moment, likely on a status accent or compact state-indicator element rather than on the whole container.

### Battery Reuse Decision

The existing `battery-button.mesh` changes planning in an important way:

- Reusing it as a visible status/control-adjacent element is still compatible with Phase 13, because the code already exists and it helps prove richer shell status composition.
- Expanding it into a new popover, deeper statistics UI, or a new interaction surface would violate the deferred scope from discussion.

So the safe boundary is: **existing battery status component may be mounted or adapted as part of the richer proof surface; new battery feature work remains out of scope.**

### Responsive Strategy

- Keep the current container-query model in `main.mesh`; do not invent a second responsive system.
- Preserve the core controls first.
- Compress secondary status text first, then hide or shorten secondary copy before shrinking affordances below the current compact button scale.
- If battery status is used, its visible text is a natural candidate to collapse before the icon/control footprint disappears.

### Test Strategy

Phase 13 should extend the existing real-surface test style instead of introducing a separate visual harness.

Recommended additions:

1. A real-surface nav-bar test proving the richer bar contains selectable passive text and still keeps control activation/shortcut behavior intact.
2. A focused constrained-width test that paints the nav bar at at least two widths and asserts:
   - secondary text compresses or disappears
   - core controls remain present
   - runtime state and layout remain stable
3. A motion-oriented test that proves the custom keyframe declaration is parsed/retained and survives real-surface rebuilds using the already-established animation test patterns.

## Files Most Likely To Change

- `modules/frontend/navigation-bar/src/main.mesh`
- `modules/frontend/navigation-bar/src/components/settings-button.mesh`
- `modules/frontend/navigation-bar/src/components/theme-button.mesh`
- `modules/frontend/navigation-bar/src/components/volume-button.mesh`
- `modules/frontend/navigation-bar/src/components/battery-button.mesh`
- `modules/frontend/navigation-bar/src/components/meta-label.mesh`
- `modules/frontend/navigation-bar/src/components/meta-pill.mesh`
- `modules/frontend/navigation-bar/COMPONENTS.md`
- `crates/core/shell/src/shell/component/tests.rs`
- `docs/frontend/mesh-syntax.md`

## Risks And Mitigations

| Risk | Mitigation |
|------|------------|
| The phase drifts into new shell features instead of proof work | Keep the plan anchored to the existing module and explicitly defer clock/theme-dropdown/popover expansion. |
| Selection proof weakens control clarity | Restrict `selectable="true"` to passive status text only and add tests that keep control behavior intact. |
| Responsive behavior becomes accidental because unused legacy classes are mixed in ad hoc | Rebuild the nav-bar layout around explicit status/control clusters and pin compact behavior with a constrained-width test. |
| Animation proof becomes decorative noise | Use transitions broadly but limit custom keyframes to one bounded accent/state moment. |
| Existing keyboard/shortcut tests regress during layout migration | Extend existing real-surface tests instead of replacing them; keep `m` shortcut and focused-control activation as locked checks. |
| Dormant battery component creates scope ambiguity | Reuse only the existing visible battery status behavior; do not add the deferred popover/stats feature. |

## Validation Architecture

### Test Layers

1. Real-surface shell tests in `mesh-core-shell`:
   - prove the migrated nav bar still supports keyboard shortcut routing and control activation
   - prove passive selectable status text exists on the primary surface
   - prove the custom keyframe proof survives component rebuild patterns

2. Constrained-width shell test in `mesh-core-shell`:
   - render the nav bar at a normal width and a compact width
   - assert secondary status copy compresses/disappears first
   - assert controls remain present and focusable

3. Docs/module consistency checks:
   - keep `COMPONENTS.md` and any author docs aligned with the shipped richer proof surface

### Commands

- Focused shell checks: `nix develop -c cargo test -p mesh-core-shell navigation_bar`
- Focused animation shell checks: `nix develop -c cargo test -p mesh-core-shell keyframe_animation`
- Full phase check: `nix develop -c cargo test -p mesh-core-shell navigation_bar -- --nocapture`

## Planning Notes

- This phase does not need new renderer crates or parser work; it should stay concentrated in the navigation-bar module and shell tests.
- Existing dormant components (`BatteryButton`, `MetaLabel`, `MetaPill`) should bias the plan toward reuse before new component creation.
- Execution should end with the navigation bar clearly functioning as the milestone proof surface, not just with prettier CSS.
