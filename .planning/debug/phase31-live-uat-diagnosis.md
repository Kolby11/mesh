---
phase: 31
type: uat-diagnosis
created: "2026-05-13T19:01:41+02:00"
status: complete
---

# Phase 31 Live UAT Diagnosis

## Symptoms

- Test 2: The audio popover opens in the correct place, but closing through the same navigation button requires multiple clicks. The slider sometimes does not drag on the first grab after opening. Open fades in, but close disappears immediately.
- Test 3: Larger slider value changes have a visible lag. The popover +/- volume buttons do not move the visible slider.
- Test 5: Muting from the navigation volume control sometimes appears non-responsive or inverted. The popover slider does not respond to button-driven volume changes.

## Evidence

- `modules/frontend/navigation-bar/src/main.mesh` keeps `audio_surface_hidden` locally and sends `shell.position-surface`, `mesh.popover.activate`, `shell.hide-surface`, and `mesh.popover.hide` from the trigger handler.
- `crates/core/shell/src/shell/component/shell_component.rs` synchronizes portal hidden bindings from `SurfaceVisibilityChanged`, clears focus state on hide, and immediately calls `surface.hide()` when `visible` is false.
- `crates/core/shell/src/shell/runtime/request.rs` applies `HideSurface` by setting visibility false and broadcasting `SurfaceVisibilityChanged`; there is no closing/exiting visibility phase.
- `crates/core/shell/src/shell/component/runtime_tree.rs` annotates sliders by preferring `slider_values[key]` over the node `value` attribute.
- `crates/core/shell/src/shell/component/input/widgets.rs` stores slider values during pointer interaction and exposes them through `slider_value`.
- `modules/frontend/audio-popover/src/main.mesh` updates `slider_value`, `pending_slider_value`, and optimistic mute state from button and slider handlers, but shell-side preserved slider values are not reconciled with later script-owned values.
- `modules/frontend/navigation-bar/src/components/volume-button.mesh` renders backend audio state directly, separate from the popover's optimistic pending state.

## Root Causes

1. Popover visibility and focus ownership are split between app-local state, shell visibility events, and popover helper requests. Close transitions are unsupported because hiding immediately unmaps the surface.
2. Preserved shell slider state outlives the interaction that created it and masks subsequent reactive value changes from buttons or backend events.
3. Navigation and popover audio controls use separate state reconciliation rules, so mute/volume UI can temporarily disagree while backend updates and optimistic local updates cross.

## Fix Direction

- Make popover toggle/close behavior shell-confirmed and add regression tests for click-open/click-close parity and first-grab slider drag after focus transfer.
- Reconcile slider preservation with script-owned value changes while preserving active-drag protection against stale backend updates.
- Add a shared audio pending/confirmation rule for mute and volume controls so nav icon, mute button label, percent text, and slider stay in sync.
- Introduce surface display transition config for show/hide, with close keeping the surface mapped until the configured exit transition completes.
