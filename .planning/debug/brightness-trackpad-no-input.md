---
status: resolved
trigger: "Brightness button does nothing for a two-finger trackpad scroll; live logs contain no brightness command or scroll activity."
created: "2026-07-15T20:49:49+02:00"
updated: "2026-07-15T23:20:43+02:00"
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

- hypothesis: confirmed — the installed module graph has no `mesh.brightness` declaration or provider, so the optional `require("mesh.brightness")` captured nil and every brightness handler returned before producing a request
- test: install a real brightness provider in the shipped graph, verify backend discovery/launch selection, and exercise its read plus increase command path with a mocked `brightnessctl`
- expecting: the shipped navigation runtime receives a non-nil proxy and scroll-generated service commands reach an active backend provider
- next_action: rebuild/restart and physically verify brightness changes through the host `brightnessctl` permissions/device
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
- timestamp: "2026-07-15T21:07:08+02:00"
  observation: after rebuilding and restarting at commit `c88240f0`, the physical two-finger scroll still does not change brightness; the supplied default-info logs contain no input-family evidence
  implication: the step-only-frame hypothesis is falsified for the user's device, and the session must resume at the live Wayland event-family boundary
- timestamp: "2026-07-15T23:07:00+02:00"
  observation: the local Wayland implementation binds `zwp_pointer_gestures_v1` and registers swipe/pinch/hold objects when pointer capability appears, but its own protocol comments and event types distinguish two-finger scrolling as ordinary `wl_pointer.axis` from compositor gestures
  implication: translating two-finger scroll from swipe updates would conflate distinct protocols; the next evidence must come from the axis boundary
- timestamp: "2026-07-15T23:08:00+02:00"
  observation: `@mesh/touch-gesture-proof` is clamped to a protocol size of 1x1; although its right-edge anchors remain, a nonzero 1px height cannot span the output
  implication: it can intercept at most one compositor pixel and cannot explain a dead 40x40 brightness button target
- timestamp: "2026-07-15T23:11:27+02:00"
  observation: navigation-bar-only INFO logs now expose raw/normalized axis values, scroll handler hit or miss (including target key/tag), and final `mesh.brightness` service command dispatch; pointer startup also reports gesture registration
  implication: the next physical reproduction will distinguish compositor/surface delivery, source classification, hit-testing, and script/service dispatch without relying on DEBUG filters
- timestamp: "2026-07-15T23:11:27+02:00"
  observation: `nix develop -c cargo check -p mesh-core-presentation -p mesh-core-shell`, all three axis normalization tests, and the shipped navigation brightness integration test pass
  implication: the observability changes compile and preserve the previously tested protocol conversion and component behavior
- timestamp: "2026-07-15T21:13:47+02:00"
  observation: the physical trackpad emits `source=Some(Finger)` axis frames on `@mesh/navigation-bar`; normalized nonzero deltas repeatedly hit the brightness button's `twofingerscroll` handler at target `root/0/2/0/0`, but no `dispatching brightness service command` line follows
  implication: compositor delivery, axis conversion, physical-surface routing, coordinate hit-testing, and handler lookup are proven; the failure is inside handler execution/runtime state or request production
- timestamp: "2026-07-15T23:18:00+02:00"
  observation: the shipped `modules/` tree contains no declaration or provider for `mesh.brightness`, while the navigation manifest marks it optional and Luau `require` returns nil when an optional interface has no selected provider
  implication: `onBrightnessScroll` deterministically exits at `if not brightness then return`, exactly matching the live handler-hit/no-command evidence
- timestamp: "2026-07-15T23:19:00+02:00"
  observation: the passing real-surfaces integration fixture manually registers a synthetic `@mesh/brightness-interface` contract and `@mesh/backlight-brightness` provider before mounting navigation
  implication: the test proved handler payload and request production only under provider-present state, but could not detect that the installed product graph lacked that state
- timestamp: "2026-07-15T23:20:43+02:00"
  observation: the installed graph now selects the bundled `@mesh/backlight-brightness` provider, backend launch discovery includes it, and the backend runtime test reads 50% then routes `increase({amount=5})` to `brightnessctl set 5.0%+`
  implication: the live component now receives a real brightness proxy and has an active command target instead of returning early

## Eliminated

- hypothesis: brightness lacks an ordinary wheel fallback when the compositor does not identify the source as `finger`
  evidence: the shipped button declares both handlers and the ordinary `Scroll` component test emits `mesh.brightness.decrease`
- hypothesis: shell runtime discards `TwoFingerScroll` during physical-surface routing
  evidence: `split_window_event` and `dispatch_wayland` preserve coordinates/deltas and convert directly to `ComponentInput::TwoFingerScroll`
- hypothesis: the brightness icon child prevents handler bubbling/hit-testing
  evidence: the integration test targets the button center covered by the icon and `pointer_event_handler_hit` walks back to the button handler successfully
- hypothesis: the physical trackpad event never reaches the brightness handler
  evidence: live INFO evidence shows repeated `two-finger scroll handler hit` records for the brightness button key with nonzero normalized deltas
- hypothesis: the two-finger event payload has the wrong shape or zero delta in Luau
  evidence: live logs show nonzero `dy` at dispatch, and the same `call_resolved_node_handler` path produces commands in the provider-present synthetic test

## Resolution

- root_cause: `mesh.brightness` was declared optional by navigation and quick settings but the shipped installed graph contained neither a contract declaration nor backend provider; `require("mesh.brightness")` therefore returned nil at component initialization and the scroll handler intentionally returned without emitting a service command. The synthetic UI test hid the product-graph gap by fabricating the missing provider.
- fix: add and explicitly select the bundled `@mesh/backlight-brightness` backend with an inline `mesh.brightness` contract, brightnessctl-backed state polling, and set/increase/decrease command handlers; remove the temporary live INFO probes after they isolated the boundary.
- verification: `nix develop -c cargo test -p mesh-core-scripting --lib` (173 passed, 19 ignored); focused installed-graph, backend-lifecycle, and shipped navigation brightness tests (3 passed). The complete shell suite reached 481 passed / 88 ignored but retains 18 unrelated baseline failures across rendering, settings/theme fixtures, element refs, popover sizing, icon registry state, and native scroll fallback.
- files_changed: `config/module.json`, `modules/backend/backlight-brightness/module.json`, `modules/backend/backlight-brightness/src/main.luau`, `crates/core/runtime/scripting/src/backend/tests.rs`, `crates/core/shell/src/shell/tests.rs`, `docs/modules/backend/core/README.md`, `.planning/debug/brightness-trackpad-no-input.md`
