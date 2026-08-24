# Status

**Updated:** 2026-08-24

## Now

The shared interaction/rendering policy now carries visibility, disabled/inert
eligibility, target filtering, and one affine transform/clip contract through
hit testing, focus, scrolling, tooltips, events, paint bounds, and the
downstream consumers.

CSS custom-property resolution, retained explicit/inherited style masks, and
the canonical generated element schema landed 2026-08-23 — see
[`.planning/log/2026-08.md`](log/2026-08.md) for the dated entries.

## Next

Prevent disabled and inert nodes, including descendants and captured targets,
from receiving activation (`docs/BACKLOG.md`).

## Blocked / open follow-ups

- Live multi-output membership and compositor conformance matrix (deferred,
  simulator-covered only).
- Connection recreation after a Wayland connection loss (separate follow-up).
