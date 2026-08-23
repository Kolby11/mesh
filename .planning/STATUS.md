# Status

**Updated:** 2026-08-23

## Now

Text measurement passes a complete shaping/wrapping context (font style,
tracking, direction, whitespace, language/features, resource/measurer
revisions) through core layout and the renderer. Intrinsic and retained caches
key every input, and retained layout remeasures when a measurer/resource
revision changes without an ordinary dirty flag.

CSS custom-property resolution, retained explicit/inherited style masks, and
the canonical generated element schema landed 2026-08-23 — see
[`.planning/log/2026-08.md`](log/2026-08.md) for the dated entries.

## Next

Make popover placement tokens and trigger/surface relationships typed,
validated, and observable across promotion (`docs/BACKLOG.md`).

## Blocked / open follow-ups

- Live multi-output membership and compositor conformance matrix (deferred,
  simulator-covered only).
- Connection recreation after a Wayland connection loss (separate follow-up).
