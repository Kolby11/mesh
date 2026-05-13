---
phase: 31
type: uat-diagnosis
created: "2026-05-13T19:01:41+02:00"
status: updated
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

## Retest Update - 2026-05-13T19:37:08+02:00

Plan 31-02 improved the broad repaint and stale slider preservation symptoms, but live retest still found three narrower failures:

- Test 2: after opening the volume surface, navigation-bar buttons do not respond on the first click and can require three attempts.
- Test 3: slider value changes no longer visibly lag, but a drag started immediately after opening the volume surface does not bind on the first grab.
- Test 5: first mute toggle updates correctly, but the second toggle does not unmute in the navigation bar, after which state appears flipped.

Updated root causes:

1. **First pointer after popover activation is still a focus/input ownership boundary.** The existing tests cover parent portal toggling and component-local first-grab behavior, but not the full shell path where a newly visible popover owns transferred keyboard focus and another surface receives the next pointer press. The first press can be consumed by ownership cleanup or by hit testing before the newly visible surface has stable paint/size state.
2. **First-grab slider coverage is too low level.** `audio_popover_first_slider_grab_dispatches_change` starts from an already painted component, so it misses the live activation sequence: show surface, focus transfer, first pointer-down, and first pointer-move on a newly mapped surface.
3. **Mute reconciliation is still split.** The popover uses local pending mute state and sends `toggle_mute`; the navigation button renders raw backend state only. A delayed backend event after the second toggle can make the nav icon lag or appear inverted. The audio interface already exposes idempotent `set_muted(device_id, muted)`, which is safer than a second `toggle_mute` when UI state is pending.

Fix direction for 31-03:

- Add shell-level regressions that open the popover and immediately click another nav button or drag the slider without an intervening paint/user retry.
- Make the first pointer event after popover activation deliver to the physical target after clearing transferred keyboard ownership.
- Ensure newly visible fixed-size surfaces have configured geometry and a current tree before the first hit test.
- Replace frontend mute toggles with requested-state `set_muted` calls where supported, and render nav/popover from the same pending/confirmed mute model.

## Retest Update - 2026-05-13T20:08:13+02:00

Plan 31-03 fixed the immediate slider grab path. Live retest now shows two remaining issues:

- Test 2: the trigger can close the popover after pointer hover leaves and re-enters the button, but not immediately while the pointer remains hovering the button.
- Test 5: the mute button/nav icon mismatch persists.

Updated root causes:

1. **Same-hover trigger close still depends on portal tick/interaction refresh.** The open path explicitly calls `mesh.popover.activate(...)`, but the close path only flips `audio_surface_hidden = true` and relies on the portal hidden binding to emit `HideSurface` later. The user's hover-out/back workaround indicates the same-position click path is still missing a shell-confirmed close action or a regression that forces the trigger to close without pointer movement.
2. **Mute state still has two frontend reconciliation models.** Shell optimistic audio state now normalizes `mesh.audio.muted`, but the audio popover still has local `pending_muted_state`/`audio_muted` while the navigation volume button renders canonical service fields. Rapid mute true -> false changes can still leave the popover-local model and nav service model briefly disagreeing.

Fix direction for 31-04:

- Add a same-hover trigger regression: open the popover, keep pointer coordinates on the volume button, click again, and assert a hide request is emitted on the first release.
- Make the close branch issue an explicit shell/popover hide request, while retaining shell visibility events as the source of truth for the portal hidden binding.
- Remove competing popover-local mute pending state and render both popover and navigation from shell-normalized `mesh.audio.muted`.
- Add a cross-component mute regression that verifies nav icon/status and popover mute label agree across false -> true -> false with stale backend updates.
