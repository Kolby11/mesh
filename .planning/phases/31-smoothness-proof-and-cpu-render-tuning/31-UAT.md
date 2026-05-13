---
status: partial
phase: 31-smoothness-proof-and-cpu-render-tuning
source:
  - .planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-01-PLAN.md
started: "2026-05-13T18:33:02+02:00"
updated: "2026-05-13T19:48:18+02:00"
---

# Phase 31 UAT - Smoothness Proof and CPU Render Tuning

## Current Test

number: 2
name: surface_open_close
expected: |
  Audio popover opens and closes without a visible stall and keeps icon/text layout correct.
awaiting: live retest after 31-03 gap closure

## Tests

### 1. hover
expected: Navigation-bar pointer hover responds without visible paint hitching and keeps hover/focus visuals correct.
benchmark_ref: `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-01-BENCHMARK.md` scenario `hover`
correctness_check: Hover state appears on the intended navigation-bar control only, no adjacent control changes unexpectedly, and the surface does not visibly flash or repaint unrelated regions.
result: pass
reported: "User confirmed live UAT matches expected behavior."
severity: none

### 2. surface_open_close
expected: Audio popover opens and closes without a visible stall and keeps icon/text layout correct.
benchmark_ref: `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-01-BENCHMARK.md` scenario `surface_open_close`
correctness_check: Popover content, icons, text, clipping, and background remain visually stable while opening and closing; no stale pixels remain after close.
result: pending
reported: "no when i open it iam unable to click buttons in the navigation bar on first try it take 3 attempts"
fix_evidence: "31-03 changed pointer-open popover activation to register without stealing focus, preserved keyboard activation focus transfer, and added a regression proving pointer activation emits ActivatePopover with focus=false. Awaiting live retest."
severity: none

### 3. pointer_update
expected: Audio slider/control pointer update tracks input without visible repaint lag and keeps control state correct.
benchmark_ref: `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-01-BENCHMARK.md` scenario `pointer_update`
correctness_check: Slider thumb, filled track, displayed value, and command dispatch state remain synchronized during pointer movement.
result: pending
reported: "it does not lag but grabbing it right after we open the volume surface does not allow drag instantly we need to start dragging again for it to work"
fix_evidence: "31-03 avoids pointer-open focus theft so the first popover pointer interaction can establish slider drag normally; existing first-grab and slider synchronization regressions pass. Awaiting live retest."
severity: none

### 4. keyboard_traversal
expected: Tab focus traversal moves focus visibly without lag and keeps focus-visible styling correct.
benchmark_ref: `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-01-BENCHMARK.md` scenario `keyboard_traversal`
correctness_check: Focus advances through the navigation-bar focus chain in order, exactly one control has focus-visible styling, and no pointer-hover styling is introduced by keyboard movement.
result: pass
reported: "User confirmed live UAT matches expected behavior."
severity: none

### 5. backend_update
expected: Audio backend state update refreshes visible values without a stall and keeps service-driven UI state correct.
benchmark_ref: `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-01-BENCHMARK.md` scenario `backend_update`
correctness_check: Backend-provided audio availability, volume percent, muted state, and visible labels update consistently without layout corruption or stale text.
result: pending
reported: "on first toggle it correcty mutes, then on the second it does not unmute in the navigation bar then it seems to work again but flipped since the second turn flipped it"
fix_evidence: "31-03 sends idempotent set_muted requests when supported and adds shell-level optimistic mute state that holds requested mute state across stale backend updates until confirmation. Awaiting live retest."
severity: none

## Summary

total: 5
passed: 2
issues: 0
pending: 3
skipped: 0
blocked: 0

## Gaps

- truth: "Audio popover opens and closes without a visible stall and keeps icon/text layout correct."
  status: fixed_pending_uat
  reason: "User reported: it does open on the correct place but when i want to close it using the button that i opened it with i need to click it three times, also the slider does not allow drag on the initial grab, i need to grab it again so this indicates some problem with focus, also the surface fades in but on close instantly disappears we should be able to configure the display transition using css and maybe component and shell config"
  severity: major
  test: 2
  root_cause: "Popover visibility, focus ownership, and hide animation are split across local navigation-bar state, shell visibility events, and popover helpers. The navigation bar toggles a local `audio_surface_hidden` flag and emits popover activation/hide requests, while the shell asynchronously updates surface visibility and transfers focus to the popover; this can leave the trigger and target briefly disagreeing about whether the popover is open. Surface hide also calls through the existing immediate hide path with no exiting state, so CSS transitions can fade in content but cannot keep the surface mapped long enough to fade out."
  artifacts:
    - path: "modules/frontend/navigation-bar/src/main.mesh"
      issue: "Owns local popover hidden state and emits position/activate/hide requests without a single authoritative open/closing state."
    - path: "crates/core/shell/src/shell/component/shell_component.rs"
      issue: "Surface visibility changes synchronize portal state, focus, and keyboard ownership after shell events rather than through an explicit popover transition lifecycle."
    - path: "crates/core/shell/src/shell/runtime/request.rs"
      issue: "HideSurface immediately marks the surface hidden and broadcasts visibility without an exit-transition phase."
  missing:
    - "Make popover toggle state derive from shell-confirmed visibility or a shell-owned toggle request so the trigger button can close the currently visible popover on the next click."
    - "Preserve first pointer-down interaction after popover focus transfer so the slider drag starts on the initial grab."
    - "Add configurable surface show/hide transition support so close can keep the surface mapped until the display transition completes."
  debug_session: ".planning/debug/phase31-live-uat-diagnosis.md"
- truth: "Audio popover opens and closes without a visible stall and keeps icon/text layout correct."
  status: fixed_pending_uat
  reason: "User reported: no when i open it iam unable to click buttons in the navigation bar on first try it take 3 attempts"
  severity: major
  test: 2
  root_cause: "31-02 fixed the parent portal toggle path, but live pointer activation still crosses surfaces while the audio popover owns keyboard focus and may still be mapped or transitioning. The shell clears keyboard ownership on the first pointer press, and the navigation component only observes final `SurfaceVisibilityChanged` events, so the first click after opening can be consumed by focus/ownership reconciliation instead of completing the intended button action."
  artifacts:
    - path: "crates/core/shell/src/shell/runtime/wayland.rs"
      issue: "Pointer press claims keyboard focus before component click dispatch, but the first press after popover activation can be used to clear transferred popover ownership."
    - path: "crates/core/shell/src/shell/runtime/request.rs"
      issue: "Transferred keyboard ownership and closing transition state are managed separately from pointer click delivery."
    - path: "modules/frontend/navigation-bar/src/main.mesh"
      issue: "Navigation toggle state is local to `audio_surface_hidden` and does not model an opening/open/closing shell-confirmed lifecycle."
  missing:
    - "Add a full shell-level regression for opening the audio popover, then clicking navigation buttons immediately on the first try."
    - "Make pointer clicks outside a transferred popover both clear popover keyboard ownership and still deliver the original click to the physical target."
    - "Expose shell-confirmed popover open/closing state to the trigger so it cannot require multiple clicks while transition/focus state settles."
  debug_session: ".planning/debug/phase31-live-uat-diagnosis.md"
- truth: "Audio slider/control pointer update tracks input without visible repaint lag and keeps control state correct."
  status: fixed_pending_uat
  reason: "User reported: it has a slight lag espetially on bigger value changes, also increasing the values using the buttons does not move the slider"
  severity: major
  test: 3
  root_cause: "Shell-side slider preservation is user-input-only and wins over the reactive `value` attribute in `annotate_runtime_tree`. After a drag or keyboard step records `slider_values[root/...]`, later Lua updates from the popover +/- buttons or backend service state can update `slider_value`, labels, and service commands, but the painted slider still reads the preserved shell value. Large value jumps also force script-state rebuild/full repaint work, so stale preservation makes the UI appear both laggy and disconnected."
  artifacts:
    - path: "crates/core/shell/src/shell/component/runtime_tree.rs"
      issue: "`slider_values` is preferred over the node `value` attribute whenever a key is present."
    - path: "crates/core/shell/src/shell/component/input/widgets.rs"
      issue: "Pointer interaction stores persistent slider values with no reconciliation against later script-owned value changes."
    - path: "modules/frontend/audio-popover/src/main.mesh"
      issue: "Volume buttons update `pending_slider_value` and service state, but no shell slider preservation entry is cleared or reconciled."
  missing:
    - "Reconcile or clear preserved slider state when the reactive `value` attribute changes outside an active drag."
    - "Add regression coverage proving popover +/- buttons and backend updates move the visible slider after prior slider interaction."
    - "Keep active-drag preservation intact so stale backend updates do not snap the slider during a drag."
  debug_session: ".planning/debug/phase31-live-uat-diagnosis.md"
- truth: "Audio slider/control pointer update tracks input without visible repaint lag and keeps control state correct."
  status: fixed_pending_uat
  reason: "User reported: it does not lag but grabbing it right after we open the volume surface does not allow drag instantly we need to start dragging again for it to work"
  severity: major
  test: 3
  root_cause: "The automated first-grab test covers the audio-popover component after it is already painted, but the live failure happens immediately after shell activation. The first pointer press can arrive before the popover has a stable painted tree/surface size or while transferred keyboard ownership is being cleared, so hit testing does not establish `active_slider_key`; the second drag works after the surface has painted and focus state is settled."
  artifacts:
    - path: "crates/core/shell/src/shell/runtime/wayland.rs"
      issue: "Pointer events are routed with fallback surface sizing when a newly visible surface may not have a known paint buffer yet."
    - path: "crates/core/shell/src/shell/component/input/mod.rs"
      issue: "Slider drag only starts when the pointer-down hit test finds the slider and sets `active_slider_key` on that first press."
    - path: "crates/core/shell/src/shell/component/tests/interaction/policy.rs"
      issue: "Current first-grab regression bypasses the live shell activation path and starts from an already rendered component."
  missing:
    - "Add a shell integration test that opens the popover and immediately presses/moves on the slider before an extra user interaction."
    - "Ensure first input after surface show uses the configured surface size and a current painted tree before hit testing."
    - "Keep slider `active_slider_key` establishment independent of focus-transfer cleanup."
  debug_session: ".planning/debug/phase31-live-uat-diagnosis.md"
- truth: "Audio backend state update refreshes visible values without a stall and keeps service-driven UI state correct."
  status: fixed_pending_uat
  reason: "User reported: mostly yes but when i mute the volume the volume button in the navigation bar sometimes does not react, even acts as inverted, also the slider does not respond to the button volume change"
  severity: major
  test: 5
  root_cause: "The navigation volume button and audio popover reconcile audio state independently. The popover applies optimistic mute/volume state before backend confirmation, the navigation button renders only the latest backend `audio.muted`/`audio.percent`, and preserved shell slider state can prevent the popover slider from reflecting backend/button volume changes. Without a shared pending/reconciliation rule, rapid mute or volume changes can briefly look inverted or non-responsive."
  artifacts:
    - path: "modules/frontend/navigation-bar/src/components/volume-button.mesh"
      issue: "Renders backend audio state only and has no pending mute/volume reconciliation path."
    - path: "modules/frontend/audio-popover/src/main.mesh"
      issue: "Uses local optimistic mute/volume updates that can diverge from navigation-button backend-only rendering."
    - path: "crates/core/shell/src/shell/component/runtime_tree.rs"
      issue: "Preserved slider values can hide backend-driven value updates."
  missing:
    - "Unify audio UI reconciliation so nav icon, mute label, percent text, and slider agree after mute and volume button actions."
    - "Add tests for mute toggle/backend update ordering and for volume button changes propagating to the popover slider."
    - "Ensure service updates clear stale pending state only when they confirm the requested mute/volume value."
  debug_session: ".planning/debug/phase31-live-uat-diagnosis.md"
- truth: "Audio backend state update refreshes visible values without a stall and keeps service-driven UI state correct."
  status: fixed_pending_uat
  reason: "User reported: on first toggle it correcty mutes, then on the second it does not unmute in the navigation bar then it seems to work again but flipped since the second turn flipped it"
  severity: major
  test: 5
  root_cause: "The popover has pending mute state, but the navigation volume button still renders raw backend `audio.muted`. The command sent by the popover is also `toggle_mute`, so a delayed or stale backend update can leave the nav button showing the old state while the user's second click has already flipped the requested state, making subsequent displays appear inverted."
  artifacts:
    - path: "modules/frontend/navigation-bar/src/components/volume-button.mesh"
      issue: "Reads backend `audio.muted` directly and has no pending requested mute state."
    - path: "modules/frontend/audio-popover/src/main.mesh"
      issue: "Tracks pending mute state locally and sends non-idempotent `audio.toggle_mute()` commands."
    - path: "modules/interfaces/audio.toml"
      issue: "Provides idempotent `set_muted(device_id, muted)` but the frontend still uses `toggle_mute` for the popover mute action."
  missing:
    - "Use an idempotent requested mute value for frontend mute actions where the interface supports `set_muted`."
    - "Share pending mute confirmation state between navigation and popover, or derive both from a single frontend-owned audio UI state."
    - "Add regression coverage for mute false -> true -> false with stale/interleaved backend confirmations."
  debug_session: ".planning/debug/phase31-live-uat-diagnosis.md"

## Completion Instructions

Final Phase 31 acceptance requires tests 2, 3, and 5 to pass a live retest after the 31-03 gap-closure implementation. Automated regressions pass; live confirmation is still required.

## Acceptance Note

Live UAT was performed against the shipped navigation/audio surfaces after Plan 31-02. Hover and keyboard traversal passed. Plan 31-03 implemented targeted fixes for tests 2, 3, and 5; final Phase 31 acceptance requires a live retest of those rows.
