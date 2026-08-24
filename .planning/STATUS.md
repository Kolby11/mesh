# Status

**Updated:** 2026-08-24

## Now

The shared interaction/rendering policy now carries visibility, transformed
translation/scale geometry, disabled/inert eligibility, and target filtering
through hit testing, focus, scrolling, tooltips, events, paint, and
accessibility. The immutable frame boundary remains the validated downstream
snapshot; see the dated log entry for this policy work.

CSS custom-property resolution, retained explicit/inherited style masks, and
the canonical generated element schema landed 2026-08-23 — see
[`.planning/log/2026-08.md`](log/2026-08.md) for the dated entries.

## Next

Use one affine transform/clip contract for hit testing, paint bounds, scrolling,
and focus geometry (`docs/BACKLOG.md`).

## Blocked / open follow-ups

- Live multi-output membership and compositor conformance matrix (deferred,
  simulator-covered only).
- Connection recreation after a Wayland connection loss (separate follow-up).
