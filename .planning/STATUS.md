# Status

**Updated:** 2026-08-24

## Now

The immutable frame boundary now captures one validated tree after state,
style, layout, and semantic phases. It carries phase stamps, stable identities,
and semantic diffs for downstream consumers; the shell retains it alongside the
mutable working tree.

CSS custom-property resolution, retained explicit/inherited style masks, and
the canonical generated element schema landed 2026-08-23 — see
[`.planning/log/2026-08.md`](log/2026-08.md) for the dated entries.

## Next

Share visibility, transformed geometry, disabled/inert eligibility, and target
filtering across interaction, rendering, focus, scrolling, tooltips, and
accessibility (`docs/BACKLOG.md`).

## Blocked / open follow-ups

- Live multi-output membership and compositor conformance matrix (deferred,
  simulator-covered only).
- Connection recreation after a Wayland connection loss (separate follow-up).
