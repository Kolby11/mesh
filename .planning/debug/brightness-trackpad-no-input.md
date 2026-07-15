---
status: resolved
trigger: "Brightness button does nothing for a two-finger trackpad scroll; live logs contain no brightness command or scroll activity."
created: "2026-07-15T20:49:49+02:00"
updated: "2026-07-15T23:02:00+02:00"
---

# Brightness Trackpad No Input

## Symptoms

- Expected behavior: scrolling down with two fingers over the navigation-bar brightness button decreases brightness; scrolling up increases it.
- Actual behavior: nothing happens.
- Error messages: none. Live logs contain navigation popover activity but no brightness service command or scroll event.
- Timeline: still fails after correcting downstream trackpad delta direction.
- Reproduction: run the Wayland shell, place the pointer over the navigation-bar brightness button, and perform a two-finger vertical trackpad scroll.
- Nearby log: `@mesh/touch-gesture-proof` is configured as a non-docked 0x0 layer surface and clamped to 1x1.

## Current Focus

- hypothesis: confirmed — the Wayland axis adapter only consumed continuous `absolute` deltas and silently dropped valid step-only frames before presentation routing
- test: verify continuous, step-only, and non-finite continuous payload conversion; run the full presentation suite and the shipped brightness integration test
- expecting: all valid nonzero axis payload shapes reach presentation while existing wheel/finger routing and brightness commands remain intact
- next_action: restart the rebuilt shell and verify the physical trackpad; use `RUST_LOG=mesh_core_presentation=debug` if compositor payload inspection is needed
- reasoning_checkpoint:
- tdd_checkpoint:

## Evidence

- timestamp: "2026-07-15T21:12:00+02:00"
  observation: the shipped brightness component registers both `scroll` and `twofingerscroll` on the button, and its existing integration test hits the icon-covered center and produces the expected brightness commands for both `ComponentInput` variants
  implication: source misclassification alone cannot explain total failure; either no axis event is emitted or it is lost before component dispatch
- timestamp: "2026-07-15T21:14:00+02:00"
  observation: `dispatch_wayland` routes both `WindowEvent::Scroll` and `WindowEvent::TwoFingerScroll` to the physical surface and converts them directly to their matching `ComponentInput`
  implication: the shell presentation-routing boundary has no intentional filter specific to trackpad input
- timestamp: "2026-07-15T21:16:00+02:00"
  observation: the only real Wayland conversion lives inline in `PointerHandler::pointer_frame`; it drops axis frames whose `absolute` values are zero and has no focused unit regression coverage
  implication: this is the first unverified boundary and the correct place to make protocol conversion explicit and testable
- timestamp: "2026-07-15T22:56:00+02:00"
  observation: `normalized_axis_delta(0.0, +/-1)` now preserves step-only movement and the focused protocol-boundary tests pass 3/3
  implication: valid axis frames can no longer disappear before source classification and presentation routing
- timestamp: "2026-07-15T23:01:00+02:00"
  observation: the complete `mesh-core-presentation` library suite passes 50 tests (12 ignored), and the shipped navigation brightness integration test passes for wheel and two-finger inputs
  implication: the boundary fix preserves coalescing, surface routing contracts, hit-testing, and brightness service command behavior
- timestamp: "2026-07-15T23:02:00+02:00"
  observation: the supplied logs use the default info filter, while Wayland input dispatch was trace-only and service commands are recorded in diagnostics rather than logged at info
  implication: absence of input lines in the supplied log is expected and was not proof that the component handler was skipped; a debug-level axis log now provides direct boundary evidence

## Eliminated

- hypothesis: brightness lacks an ordinary wheel fallback when the compositor does not identify the source as `finger`
  evidence: the shipped button declares both handlers and the ordinary `Scroll` component test emits `mesh.brightness.decrease`
- hypothesis: shell runtime discards `TwoFingerScroll` during physical-surface routing
  evidence: `split_window_event` and `dispatch_wayland` preserve coordinates/deltas and convert directly to `ComponentInput::TwoFingerScroll`
- hypothesis: the brightness icon child prevents handler bubbling/hit-testing
  evidence: the integration test targets the button center covered by the icon and `pointer_event_handler_hit` walks back to the button handler successfully

## Resolution

- root_cause: the live Wayland pointer adapter derived deltas exclusively from SCTK's continuous `absolute` fields; valid axis frames carrying only `discrete` steps became `(0, 0)` and were discarded before presentation or component hit-testing
- fix: centralize axis normalization, prefer finite continuous motion, fall back to discrete steps, preserve the existing positive-up convention, and emit a debug-level record containing surface, source, coordinates, and normalized deltas
- verification: `nix develop -c cargo test -p mesh-core-presentation --lib` (50 passed, 12 ignored); `nix develop -c cargo test -p mesh-core-shell shipped_navigation_brightness_uses_one_level_icon_and_scrolls_both_input_kinds -- --nocapture` (1 passed)
- files_changed: `crates/core/presentation/src/wayland_surface/handlers.rs`, `.planning/debug/brightness-trackpad-no-input.md`
