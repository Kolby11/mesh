# Shared task notes

## This iteration (2026-07-02)

- Verified and closed two todo.md items that were actually already fixed by
  prior commits but left unchecked: the promoted-popover hover-bridge
  pointer-enter fix (`2425c33a`) and the backend `init`/frontend `onRender`
  legacy-lifecycle fallback removal (this iteration — migrated the one
  remaining shipped straggler `modules/frontend/debug-inspector/src/main.mesh`
  from `onRender` to `render`, then dropped both fallback code paths and
  updated ~10 test files that still used the legacy names).
- Full workspace test suite passes at the pre-existing baseline (see below).

## Known pre-existing breakage (not caused by this session, needs its own pass)

`mesh-core-animation`'s unit tests **do not compile** on current `main`:
- `crates/core/ui/animation/src/keyframes.rs:242` — `AnimatableStyle` literal
  missing the `visibility` field.
- `crates/core/ui/animation/src/transition.rs:587` — references
  `ComputedStyle.transition` (singular), but the real field is `transitions`.

This blocks a plain `cargo test --workspace`; use
`cargo test --workspace --exclude mesh-core-animation` until it's fixed.
Also note `style::tests::shipped_audio_style_fixture_resolves_painter_relevant_values`
in `mesh-core-elements` is a separately-known pre-existing failure (see
memory `project_animation_engine`).

Worth fixing next — small, isolated compile fix, then re-run `mesh-core-animation`'s
suite to see if it reveals real regressions once it compiles again.

## Environment note

Bare `cargo build`/`cargo test` fails outside the nix shell (`smithay-client-toolkit`
can't find `xkbcommon` via pkg-config). Always prefix Rust builds/tests with
`nix develop --command ...` in this repo.

## Good next todo.md items (scoped, not yet started)

- Fix the `mesh-core-animation` compile break above (quick, unblocks full-suite runs).
- `service_name_from_interface` dedup, `ModuleKind::{FontPack,Library}` lossy
  conversion, `BackendScriptContext`/`ScriptContext` constructor explosion —
  all in the "Migration tech-debt" section, all small and well-scoped.
- Any of the "Cheap quality wins" style items would be `Split
  FrontendSurfaceComponent::paint` or `StyleResolver::apply_declaration`
  table-driven refactor — larger, riskier, better as their own reviewed PRs.
- Frontend-side typed/declared event channel validation (todo.md "Make event
  channels typed and declared" — backend-side landed, frontend-side open).

## Popup padding iteration (2026-07-02)

Implemented the popup "buffer padding + input region for shadows" todo.md item
(under "Embeddable popovers via `<popover>` surface promotion"). Promoted
`xdg_popup` buffers now pad beyond the measured content box for descendant
box-shadow/blur overflow, offset the positioner to keep content anchored, and
mask the Wayland input region back down to the true content rect. See the
todo.md entry for full details and file/function pointers
(`popover_content_padding`, `shadow_filter_extended_bounds` in
`crates/core/shell/src/shell/component/shell_component.rs`;
`ChildSurfaceRequest.surface_size`/`content_offset` in
`crates/core/frontend/host/src/lib.rs`; consumer in
`crates/core/shell/src/shell/runtime/render.rs`
`reconcile_child_surface_requests`/`paint_and_present_child_surface`).

**Not verified on a live compositor** (this environment has no Wayland
session) — the anchor-offset compensation math is a best-effort derivation
from the xdg_positioner protocol semantics, reasoned through for the shipped
`anchor="bottom" gravity="bottom"` case. Worth a visual check on a real
compositor (Hyprland/wlroots) to confirm the popover shadow renders and the
bubble content still lands exactly under its trigger.

Build/test: use `nix develop --command cargo build|test ...` — bare `cargo`
fails outside the nix shell (missing `xkbcommon` pkg-config for
`smithay-client-toolkit`). Full workspace build and `mesh-core-shell` test
suite (388 tests) both pass clean.

## Next up

Good next candidates from todo.md, roughly in order of self-contained value:

1. **Content sizing + reposition** (same popover-promotion section) —
   `xdg_popup.reposition` when the anchor moves (output/exclusive-zone
   change), noting the xdg_wm_base v3+ requirement.
2. **Keyboard/focus + a11y across the surface boundary** for popups —
   `role="menu"`, arrow-key nav, focus traversal crossing the popup boundary.
3. Cheap/contained cleanup items under "Migration tech-debt" — e.g. verify and
   drop the legacy `init`/`onRender` backend lifecycle-name fallbacks if no
   shipped module still uses them.
4. Larger refactors (`FrontendSurfaceComponent::paint` split,
   `StyleResolver::apply_declaration` table-driven rewrite,
   `installed_graph.rs::build_graph_diagnostics` pass extraction) — bigger
   diffs, better as their own reviewed PRs per the todo.md note.

Avoid re-touching the popup padding math without re-reading the "Not verified"
caveat above — it hasn't regressed anything in unit tests but has never been
seen render for real.

## Tooltip extraction iteration (2026-07-02)

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

## Backlog verification iteration (2026-07-02)

Several "Embeddable popovers via `<popover>` surface promotion" backlog items
were re-verified and marked done because earlier commits had already landed
them: the pointer-enter hover-bridge fix (`2425c33a`), the embeddable component
`module.json` shape for language/theme popovers, and popup content sizing plus
`xdg_popup.reposition`.

Before starting any unchecked `todo.md` item, grep for its implementation first.
This backlog has accumulated across many iterations, and some open checkboxes
were already finished by later commits without being flipped.
