---
created: 2026-07-15T00:00:00.000Z
title: Multi-finger gesture events (onswipe, onpinch, ontwofingerscroll, touch)
area: input
related_phases: []
files:
  - crates/core/presentation/src/wayland_surface/handlers.rs
  - crates/core/presentation/src/lib.rs
  - crates/core/frontend/host/src/lib.rs
  - crates/core/frontend/compiler/src/render.rs
  - crates/core/shell/src/shell/component/input/mod.rs
  - crates/core/shell/src/shell/component/input/widgets.rs
  - crates/core/ui/component/src/parser/markup.rs
  - crates/tools/lsp/src/knowledge/tags.rs
---

## Status

Implementation complete through Phase 3. Wayland gesture/touch plumbing,
`ComponentInput`, stable target capture, `.mesh` dispatch, LSP metadata,
authoring documentation, and synthesized touch conveniences (`ontap`,
`onlongpress`, `ondoubletap`, and touch-to-`onclick`) are implemented. Live
compositor UAT findings are recorded below.

## Current state (verified 2026-07-15)

The original pre-implementation snapshot below is retained for design history.
The current implementation now covers all three layers described here:

1. **Wayland raw input** (`crates/core/presentation/src/wayland_surface/handlers.rs:230-338`)
   — binds `wl_pointer` only (`Enter`/`Leave`/`Motion`/`Press`/`Release`/`Axis`).
   No `wl_touch`, no `zwp_pointer_gestures_v1` (pinch/swipe/hold) binding
   anywhere in `crates/core/presentation` (confirmed via grep — zero
   hits for touch/gesture/pinch/swipe/multitouch). **This is the actual
   blocker** — nothing upstream produces gesture data to dispatch.
2. **Component input enum** (`crates/core/frontend/host/src/lib.rs:57-85`)
   — `ComponentInput` has `PointerMove`, `PointerLeave`,
   `PointerButton{pressed}`, `Scroll{dx,dy}`, `KeyPressed`,
   `KeyReleased`, `Char`. No gesture variants yet.
3. **Handler dispatch** (`crates/core/shell/src/shell/component/input/widgets.rs:682-727`)
   — `dispatch_scroll_handler` is the template to copy: hit-test for a
   node with a `"scroll"` handler, build a JSON payload (`type`,
   `pointer{x,y}`, `delta{x,y}`, `surface{...}`, `current{...}`), call
   `call_resolved_node_handler(hit.node, "scroll", &[event])`.

The `on*` attribute parser is already generic (`is_event_attr` in
`crates/core/ui/component/src/parser/markup.rs:419-421` accepts any
`on[a-zA-Z]+`), so new event names need **no parser changes** — only
LSP doc updates (`crates/tools/lsp/src/knowledge/tags.rs:123-153`) so
they autocomplete.

## Decisions from planning discussion

- **Scope**: target both trackpad (`zwp_pointer_gestures_v1`) and
  touchscreen (`wl_touch`) protocols in the first pass, not trackpad-only.
- **Naming**: unified event names with payload discrimination, matching
  the existing `onscroll` style — `onswipe` fires for any finger
  count/direction, handler inspects `event.fingers` / `event.direction`;
  `onpinch` fires with `event.scale` / `event.rotation`. Do **not** create
  finger-count-specific names like `onthreefingerswipe`.

## Proposed event vocabulary

**Trackpad (`zwp_pointer_gestures_v1`)**
- `ontwofingerscroll` — distinct from `onwheel`/`onscroll` (discrete wheel
  clicks); trackpad two-finger pan is continuous and higher-resolution.
  Payload: `delta{x,y}`, `pointer{x,y}`.
- `onpinch` — fires through `pinchstart`/`pinchupdate`/`pinchend`
  internally, but surfaces as one handler with `event.phase` (`"start"`,
  `"move"`, `"end"`) plus `scale` (relative to gesture start) and
  `rotation` (degrees).
- `onswipe` — `fingers` (3 or 4, from `zwp_pointer_gestures_v1` swipe
  begin), `direction` (`"up"|"down"|"left"|"right"`, derived from
  dominant delta axis at gesture end), `velocity`.
- `onhold` — two/three-finger press-and-hold with no motion beyond a
  threshold; payload `fingers`, `duration`.

**Touchscreen (`wl_touch`)**
- `ontouchstart` / `ontouchmove` / `ontouchend` / `ontouchcancel` — raw
  per-touch-point events, payload includes a `touches[]` array (id, x, y)
  mirroring the DOM Touch API shape for familiarity.
- `ontap` — synthesized from a single touch-down/up pair within a
  distance+time threshold (avoid double-processing with `onclick`; decide
  whether touch taps should *also* synthesize `onclick` for handler reuse,
  or coexist as a separate event — lean toward *also* firing `onclick` so
  existing click handlers keep working on touch devices).
- `onlongpress` — touch-down held past a threshold without movement.
- `ondoubletap` — two `ontap`s within a time+distance window.

## Plan

### Phase 1 — Wayland protocol bindings (complete)

- Bind `zwp_pointer_gestures_v1` in the layer-shell/dev-window backend
  setup (`crates/core/presentation/src/wayland_surface/`), request pinch,
  swipe, and hold gesture objects per pointer.
- Bind `wl_touch` alongside the existing `wl_pointer` binding, handle
  `down`/`up`/`motion`/`frame`/`cancel`.
- New raw event types (mirror the existing `DevWindowEvent` pattern) —
  e.g. `DevWindowEvent::{GestureSwipe, GesturePinch, GestureHold,
  TouchDown, TouchMove, TouchUp, TouchCancel}`.
- Check what the target compositors actually support (`zwp_pointer_gestures_v1`
  is a well-established protocol on wlroots/KWin/Mutter, but verify swipe
  fires with 3 vs 4 fingers consistently — some compositors only forward
  3-finger swipes to clients and reserve 4-finger for compositor-level
  workspace switching. Document this as a known variance, not a bug.)

### Phase 2 — `ComponentInput` + dispatch (complete)

- Extend `ComponentInput` (`crates/core/frontend/host/src/lib.rs`) with
  gesture/touch variants.
- Add `dispatch_swipe_handler` / `dispatch_pinch_handler` /
  `dispatch_touch_handler` etc. in `widgets.rs`, modeled on
  `dispatch_scroll_handler` — same hit-test-then-call-handler shape, new
  JSON payload shape per event above.
- Decide gesture target resolution: hit-test at gesture start position
  and keep dispatching to that same node for the gesture's duration
  (matches pointer-capture semantics used for drag), not per-frame
  re-hit-testing.

### Phase 3 — authoring surface

- Add all new event names to `crates/tools/lsp/src/knowledge/tags.rs` so
  they autocomplete and hover-document like `onclick`/`onchange`.
- Document in `docs/frontend/mesh-syntax.md` (or wherever the event list
  is documented) with payload shapes.
- Decide default behavior when no handler is registered — gestures should
  not block/consume normal scroll or click handling on the same node
  unless a handler is present (avoid regressing existing scroll UX on
  nodes that don't opt into gestures).

## Live Wayland UAT (2026-07-15)

The available live session runs Hyprland 0.55.4 with a GXTP5100 touchpad.
`hyprctl devices` reports no touchscreen device, so raw `wl_touch`, tap,
double-tap, long-press, and touch-to-`onclick` behavior could not be exercised
on physical touchscreen hardware in this session.

This compositor configuration reserves both tested multi-finger families:

| Fingers | Live Hyprland binding | Expected client observation |
| ------- | --------------------- | --------------------------- |
| 3 | swipe → move window; pinch → fullscreen | compositor may consume the gesture before MESH receives `zwp_pointer_gestures_v1` events |
| 4 | horizontal → workspace; up/down → overview | compositor may consume the gesture before MESH receives it |

That confirms the important compositor variance: finger count alone cannot
guarantee client delivery. On this installation both three- and four-finger
gestures conflict with compositor actions; on a compositor/session without
those bindings the same protocol events can reach MESH. The proof surface is
`modules/frontend/touch-gesture-proof/src/main.mesh`. A final hardware pass
should temporarily disable one binding at a time, launch that surface, and
confirm `start`/`move`/`end` payloads versus the compositor-owned case.

Automated UAT covers the client-side contract independently of compositor
delivery: the complete interaction-navigation group passes all swipe, pinch,
hold, two-finger-scroll, raw-touch, tap, double-tap, long-press, and synthesized
click tests. Two unrelated existing navigation popover geometry assertions
remain red (`114x80` actual versus `112x74` expected).

## Risks / notes

- No touch hardware in typical MESH deployment (laptop/desktop shell) —
  touchscreen support is lower priority in practice than trackpad
  gestures; trackpad gestures are the primary user-facing win (e.g.
  three-finger swipe to switch workspaces from a panel widget, two-finger
  scroll inside a popover list not already covered by `onwheel`).
- Compositor support for `zwp_pointer_gestures_v1` varies; must degrade
  gracefully (no gesture events fire, no crash) on compositors that don't
  advertise the global — matches the existing pattern for optional
  protocols elsewhere in `mesh-core-presentation`.
- Gesture payloads should stay JSON-shaped consistent with the existing
  scroll/pointer event payloads so Luau handler code style stays uniform
  across event types.
