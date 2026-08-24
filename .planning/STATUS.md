# Status

**Updated:** 2026-08-24

## Now

Activation now revalidates live disabled/inert eligibility for descendants,
captured pointer targets, focused keyboard targets, and synthesized activation
routes before dispatching handlers.

The shared interaction/rendering policy carries visibility, disabled/inert
eligibility, target filtering, and one affine transform/clip contract through
hit testing, focus, scrolling, tooltips, events, paint bounds, and downstream
consumers.

CSS custom-property resolution, retained explicit/inherited style masks, and
the canonical generated element schema landed 2026-08-23 — see
[`.planning/log/2026-08.md`](log/2026-08.md) for the dated entries.

## Next

Preserve keyframe progress across pause/resume and iteration-boundary changes
(`docs/BACKLOG.md`).

## Blocked / open follow-ups

- Live multi-output membership and compositor conformance matrix (deferred,
  simulator-covered only).
- Connection recreation after a Wayland connection loss (separate follow-up).
