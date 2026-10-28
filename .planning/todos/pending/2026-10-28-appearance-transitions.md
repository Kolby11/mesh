# Appearance transitions (theme, icon theme, font)

Backlog: "Animate appearance switches" in `docs/BACKLOG.md`.

## Goal

Theme/mode, icon-theme and font-family switches ease in rather than snapping.
Reduced motion keeps the instant swap. No cost when idle.

## Starting points

- Theme: `apply_set_theme`/mode path in `crates/core/shell/src/shell/runtime/theme.rs`
  installs a prepared theme in one commit. Colour tokens could ride the existing
  `TransitionAnimator` by treating the token delta as a transition from the
  previous computed style; check the known `transition:` first-property-only
  limitation first.
- Icons: `reconcile_icon_shell_default` publishes a new registry and advances the
  resource revision, so every icon repaints at once. Keep the previous raster
  until the new one is ready (the repaint path already shows nothing while a job
  is pending) and crossfade old to new per icon node over a short duration.
- Fonts: `apply_set_font_family` changes metrics, so layout moves too. A plain
  crossfade of the surface snapshot is likely the only smooth option; animating
  glyph morphs is out of scope.

## Open questions

- Per-node crossfade versus a whole-surface snapshot crossfade (one mechanism for
  all three, but costs a retained copy and interacts with damage tracking).
- Duration/easing as theme tokens so themes can restyle or disable it.
- Record measurements in `.planning/log/performance-log.md` when it lands.
