# Shared task notes

Working from `todo.md`. This iteration extracted `compute_tooltip_state()` out of
`FrontendSurfaceComponent::paint()` (`crates/core/shell/src/shell/component/shell_component.rs`),
the first of two extraction targets called out under "Larger refactors" for that
~486-line function. Verified with `nix develop --command cargo test -p mesh-core-shell --lib`
(387 passed, 0 failed) — this repo's cargo needs `nix develop` (plain `cargo check`
fails on the `xkbcommon` pkg-config dependency for `smithay-client-toolkit`).

## Next up

1. **`paint_pixel_regions()` extraction** (same todo.md item, now the only piece left).
   In `paint()`, after the tooltip/damage-rect computation, there's a `match`-like
   if/else chain that clears + calls
   `mesh_core_render::paint_selected_display_list_for_module_with_profiling_metrics`
   once for full-surface, once for the bounding-rect path, once for single-rect, and
   once in a loop for multi-rect — four near-identical call sites differing only in
   the clear region and the `Some((x,y,w,h))` damage arg. Collapse into a helper that
   takes `buffer`, `scale`, an iterator/slice of damage rects, `selected_paint`,
   `tooltip` + `current_tooltip_damage`, and the module id, and returns merged
   `PaintProfilingMetrics`. Re-run the same test command after.
2. Otherwise pick the next unchecked item in `todo.md` — good next candidates are the
   other "Larger refactors" (e.g. `handle_component_input` split, or
   `install_host_api` split) or the P1 renderer/scripting perf items marked with a
   milestone tag.

## Notes
- `nix develop --command cargo test -p <crate> --lib` is the standard verification
  loop here; the workspace won't `cargo check` without the nix shell.
- Don't touch `.planning/` GSD scaffolding unless a skill explicitly asks — this repo
  layers a plain `todo.md` backlog on top of that infra and this loop is driven from
  the plain backlog.
