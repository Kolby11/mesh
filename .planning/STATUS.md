# Status

**Updated:** 2026-08-24

## Now

Focus, pointer capture, press origin, gesture ownership, and scroll ownership
now publish through one staged `mesh-core-interaction` transaction. The shell
keeps key-based lookup caches for script/render compatibility, while ownership
changes, eligibility reconciliation, typed decisions, categorized dirty-node
output, and speculative rollback share one commit boundary.

Keyframe playback now preserves active progress through pause/resume, including
delay, direction, iteration boundaries, and finite completion state.

Activation revalidates live disabled/inert eligibility for descendants,
captured pointer targets, focused keyboard targets, and synthesized activation
routes before dispatching handlers.

The shared interaction/rendering policy carries visibility, disabled/inert
eligibility, target filtering, and one affine transform/clip contract through
hit testing, focus, scrolling, tooltips, events, paint bounds, and downstream
consumers.

Reduced-motion preferences now flow through one `MotionPolicy` snapshot that
clamps non-essential transitions, keyframes, scrolling, inertia, tooltips, and
surface motion at settings and frame boundaries.

CSS custom-property resolution, retained explicit/inherited style masks, and
the canonical generated element schema landed 2026-08-23 — see
[`.planning/log/2026-08.md`](log/2026-08.md) for the dated entries.

Visibility transition playback now starts from the previously displayed
discrete value and applies the visibility endpoint at the end of an exit.

Validated per-keyframe easing now flows from component and theme keyframes
through shell animation state into the core and tooltip samplers.

The public animation `box-shadow` parser now returns structured errors for
malformed values and rejects comma-separated shadow lists before construction.

Animation instances now use retained node identity, animation-list position,
and declaration generation for stable identity. The shell and core expose
explicit started, continued, replaced, cancelled, completed, and reversed
decisions so style changes do not inherit stale timelines.

`InteractionFrame` now carries the renderer-neutral interaction state,
revision, typed decisions, dirty outputs, immutable tree snapshot, and ordered
phase stamps from input/state through style invalidation, layout, animation,
paint, and semantics.

## Next

Reject absolute, traversal, and symlinked frontend entrypoint/import paths
outside the module root (`docs/BACKLOG.md`).

## Blocked / open follow-ups

- Live multi-output membership and compositor conformance matrix (deferred,
  simulator-covered only).
- Connection recreation after a Wayland connection loss (separate follow-up).
