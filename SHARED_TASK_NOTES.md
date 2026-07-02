# Shared task notes

## Last iteration (2026-07-02)

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
