---
status: diagnosed
phase: 31-smoothness-proof-and-cpu-render-tuning
source:
  - .planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-01-PLAN.md
started: "2026-05-13T18:33:02+02:00"
updated: "2026-05-13T19:01:41+02:00"
---

# Phase 31 UAT - Smoothness Proof and CPU Render Tuning

## Current Test

[testing complete]

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
result: issue
reported: "it does open on the correct place but when i want to close it using the button that i opened it with i need to click it three times, also the slider does not allow drag on the initial grab, i need to grab it again so this indicates some problem with focus, also the surface fades in but on close instantly disappears we should be able to configure the display transition using css and maybe component and shell config"
severity: major

### 3. pointer_update
expected: Audio slider/control pointer update tracks input without visible repaint lag and keeps control state correct.
benchmark_ref: `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-01-BENCHMARK.md` scenario `pointer_update`
correctness_check: Slider thumb, filled track, displayed value, and command dispatch state remain synchronized during pointer movement.
result: issue
reported: "it has a slight lag espetially on bigger value changes, also increasing the values using the buttons does not move the slider"
severity: major

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
result: issue
reported: "mostly yes but when i mute the volume the volume button in the navigation bar sometimes does not react, even acts as inverted, also the slider does not respond to the button volume change"
severity: major

## Summary

total: 5
passed: 2
issues: 3
pending: 0
skipped: 0
blocked: 0

## Gaps

- truth: "Audio popover opens and closes without a visible stall and keeps icon/text layout correct."
  status: failed
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
- truth: "Audio slider/control pointer update tracks input without visible repaint lag and keeps control state correct."
  status: failed
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
- truth: "Audio backend state update refreshes visible values without a stall and keeps service-driven UI state correct."
  status: failed
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

## Completion Instructions

Final Phase 31 acceptance requires each scenario result to be set to `pass`, `issue`, `blocked`, or `skipped` before verification. Update the summary totals so they add up to `total: 5`. Set frontmatter `status: complete` only when no test remains awaiting manual action or blocked.

## Acceptance Note

Live UAT was performed against the shipped navigation/audio surfaces. Two scenarios passed and three major interaction/state gaps were diagnosed; final Phase 31 acceptance requires executing the gap-closure plan and rerunning these UAT rows.
