# Shared Task Notes

## This iteration (2026-07-02)

Worked from `todo.md`. Several backlog items under "Embeddable popovers via
`<popover>` surface promotion" turned out to already be implemented by
earlier passes but were still marked `[ ]` — re-verified and marked `[x]`
with file:line citations:
- pointer-enter hover-bridge fix (was already landed in `2425c33a`)
- `module.json` embeddable-component shape (language-popover/theme-selector
  already ship `mesh.kind: "component"`, no `mesh.surface` block)
- popup content sizing + `xdg_popup.reposition` (already wired)

Then implemented the one genuinely open item from that section: **popup
buffer padding + input region for shadows**. Popup buffers were sized exactly
to the popover's content box, so `box-shadow`/`filter` overshoot on the
popover or any descendant got hard-clipped, and no input region was set for
popups at all. Fixed in `crates/core/shell/src/shell/component/shell_component.rs`
(`popover_content_padding`, `subtree_visual_bounds`, shared `node_visual_bounds`
extracted from the existing damage-rect code) and
`crates/core/shell/src/shell/runtime/render.rs` (`reconcile_child_surfaces`,
`paint_and_present_child_surface`). Test:
`popover_with_descendant_box_shadow_gets_buffer_padding`. Full
`mesh-core-shell` suite (388 tests) + workspace build pass under `nix develop`.

**Build note:** `cargo build` outside `nix develop` fails —
`smithay-client-toolkit` needs `xkbcommon.pc` via pkg-config, which isn't on
PATH outside the nix shell. Always prefix cargo commands with
`nix develop --command`.

## Next up — good candidates from `todo.md`

Remaining open items under "Embeddable popovers" (popover promotion epic):
- **Centralize the popover controller in core** — audio popover migration
  still pending (needs drag/capture state represented in core first).
- **Keyboard/focus + a11y across the surface boundary** — `role="menu"` +
  arrow-key nav exists *within* a tree (`rove_focus_within_parent`,
  `shell/component/input/mod.rs:439`) but cross-surface (parent→popup) focus
  transfer on open is not verified/implemented; worth checking
  `keyboard_focus_surface` handling on popup creation before building
  anything new.
- **`module.json` rework** section still has one real open item: unify the 4
  contribution schemas (theme/icons/i18n/keybinds) — explicitly deferred
  ("E deferred") in the redesign, low priority.

Bigger, well-scoped chunks elsewhere in `todo.md`:
- "Larger refactors" list (split `FrontendSurfaceComponent::paint`,
  `StyleResolver::apply_declaration` table-driven rewrite, etc.) — each is a
  separate reviewed-PR-sized job per the backlog's own note.
- Module system open follow-ups (typed event channels validation on the
  frontend side, multiple instances of the same frontend module, generated
  settings UI) are larger design efforts — read the "Module system
  decisions" memory/doc before starting.

**Before starting any item marked `[ ]`**: grep for it first — this session
found 3 stale checkboxes in one pass. The backlog has accumulated across many
iterations and some "open" items were already finished by later commits
without the checkbox being flipped.
