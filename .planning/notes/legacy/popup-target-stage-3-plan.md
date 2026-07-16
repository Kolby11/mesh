# Stage 3 — Shell: one component → base surface + N popup targets

## Context

`<popover>` promotion (the embeddable-popover initiative, todo.md 2026-06-21) is
realized at runtime by painting a popover's subtree into its own compositor
`xdg_popup` surface so it can extend past the host surface's fixed buffer.
**Stage 2** landed the presentation primitive (`wayland_surface/popup.rs`,
`configure_popup`/`destroy_popup`/`take_dismissed_popups`/`popup_supported` on
`PresentationEngine`, and the element-model `PopoverPlacement` in
`crates/core/ui/elements/src/popover.rs`).

Today the shell is hardwired 1:1 — each `FrontendSurfaceComponent` (one Lua VM)
maps to exactly one `surface_id` with one `paint_buffer`, one
`known_surface_size`, one `last_surface_config` (`ComponentRuntime` in
`shell/types.rs`). The render loop (`runtime/render.rs`), input router
(`runtime/wayland.rs`), and `component_index_for_surface` all assume that.

**This stage** generalizes the *shell plumbing* so a single component can own its
base surface plus N popup child surfaces, all driven by the **same VM and the same
widget tree**: per-target paint buffers, present iteration, pointer **and**
keyboard input routing back into the same VM with popup-local→tree coordinate
translation, focus traversal across the surface boundary, and element-metrics that
stay unified (because it is one tree).

**Out of scope (Stage 4):** the popover *controller* — the state machine that
decides *when* to promote/anchor/dismiss (`anchor={refs.x}`, hover-bridge,
one-open-per-group, grab-serial acquisition). Stage 3 exposes the mechanism and a
**minimal deterministic driver** (`<popover>` subtree promotes while shown) only so
the plumbing can be exercised end-to-end in tests.

## Key insight that shrinks the work

A promoted `<popover>` is **a subtree of the component's single widget tree**, not a
second component. Therefore:

- **Element metrics / `refs.<name>` are already unified** — `publish_element_metrics`
  (`component/interaction_state.rs`) walks the one tree at origin (0,0); popover
  nodes are already in it. No per-surface metrics split is needed.
- **Keyboard focus traversal across the boundary is naturally unified** — there is one
  `focused_key` and `collect_focus_traversal` already runs over the whole tree
  including popover nodes. Crossing the boundary is *not* the cross-surface marshalled
  Tab transfer used between separate-component popovers; it is ordinary in-tree
  traversal. The only new work is making the shell accept popup surface_ids as
  belonging to the owning component for routing/keyboard-focus.
- The painter already supports an origin offset: `paint_frontend_tree_at`
  (`crates/core/frontend/render/src/surface/mod.rs:87`).

## Design

The **component** owns per-target render state (subtree, buffer paint, coord origin);
the **shell** owns per-target presentation bookkeeping (buffer, placement, present).
Coordinate model: a popup's buffer-local (0,0) corresponds to the popover subtree's
top-left in parent-tree space. Translate by **subtracting** the subtree origin when
painting, **adding** it when routing input — then hit-test the one tree as usual.

## Changes

### 1. Trait contract — `crates/core/frontend/host/src/lib.rs`
Add a presentation-agnostic popup-target description and target-aware methods to
`ShellComponent` (all defaulted so non-frontend components are unaffected):

- `pub struct PopupTarget { surface_id, parent_surface_id, placement: PopoverPlacement,
  anchor_rect: (i32,i32,i32,i32), size: (u32,u32), grab_serial: Option<u32> }`
  (re-export `PopoverPlacement` from `mesh-core-elements`).
- `fn popup_targets(&self) -> Vec<PopupTarget> { Vec::new() }` — currently-promoted popovers.
- `fn paint_popup(&mut self, popup_surface_id: &str, theme, w, h, buffer, scale) -> Result<(), ComponentError>`
  (default `Ok(())`).
- Target-aware input + readback, base path delegates with `None`:
  `fn handle_input_for_surface(&mut self, surface: Option<&str>, theme, w, h, input)`
  (default forwards to `handle_input`), `take_popup_present_damage(&mut self, id)`,
  `popup_display_list(&self, id)`, `popup_content_input_size(&self, id)`.

### 2. Shell per-component popup bookkeeping — `crates/core/shell/src/shell/types.rs`
`ComponentRuntime` grows `popups: HashMap<SurfaceId, PopupRuntimeState>`, where
`PopupRuntimeState { paint_buffer, known_surface_size, last_placement, last_grab_serial }`.
Mirrors the existing base-surface fields, one set per popup.

### 3. Surface→component resolution + lifecycle — `crates/core/shell/src/shell/mod.rs`
- `component_index_for_surface` also matches a component's `popups` keys (add a
  `popup_surface_owner: HashMap<SurfaceId, usize>` rebuilt when targets change, or scan).
- `claim_keyboard_focus_for_surface` / `keyboard_focus_surface` accept popup ids.
- Register popup `SurfaceState` entries in `core.surfaces` so visibility/lifecycle
  helpers treat them as real surfaces.

### 4. Render loop — `crates/core/shell/src/shell/runtime/render.rs`
After the existing base-surface present, for each `component.popup_targets()`:
- diff against `runtime.popups`: **new/changed placement** → `presentation_engine.configure_popup(id, PopupConfig{..})` (translate `PopoverPlacement`→presentation `PopupPlacement` here — the shell depends on both crates); **gone** → `destroy_popup(id)` + drop state.
- ensure a physical-size `paint_buffer` (reuse the existing alloc-cap logic), call
  `component.paint_popup(id, …)`, then `present_with_damage(id, …, take_popup_present_damage(id))`.
- skip entirely when `!presentation_engine.popup_supported()` (dev-window / headless):
  popovers stay inline, base path unchanged.
Drain `presentation_engine.take_dismissed_popups()` once per frame (here or in the
event pump) and drop the matching `runtime.popups` + notify the component.

### 5. Input routing — `crates/core/shell/src/shell/runtime/wayland.rs`
When `route_surface_id` resolves to a component via a **popup** id, call the new
`handle_input_for_surface(Some(popup_id), …)` with the popup's `known_surface_size`.
Keyboard events already route through `keyboard_focus_surface`; once that may be a
popup id, the same path delivers keys to the owning component.

### 6. FrontendSurfaceComponent — `crates/core/shell/src/shell/component/*`
- **Promoted-popover discovery (minimal driver).** In `finalize_tree`
  (`component/rendering.rs`) collect promoted `<popover>` subtrees: a popover that is
  *shown* (`open`) yields a `PromotedPopover { node_key, origin:(x,y), size:(w,h),
  placement: PopoverPlacement::from_node(node), surface_id:
  "<base>::popover::<node_key>" }`. Store on the component; `popup_targets()` maps these
  to `PopupTarget`. (Deterministic "promote when shown"; the real controller is Stage 4.)
- **Base `paint` excludes promoted subtrees** so they are not double-painted in the base
  buffer (clip/skip by node_key during the base paint pass).
- **`paint_popup`** finds the subtree by `node_key` in `self.last_tree`, paints it via
  `paint_frontend_tree_at` translated by `-origin` (plus shadow/overshoot padding,
  reusing `tooltip_overlay_extra_for_content` in `component/rendering.rs`), and records a
  per-popup display list + damage in a `HashMap<SurfaceId, …>` exposed by the new getters.
- **`handle_input_for_surface(Some(id), …)`** looks up that target's `origin`, adds it to
  the incoming popup-local coords, then runs the existing `handle_input` hit-test against
  the single tree (one shared interaction-state set — `focused_key`, `hovered_key`,
  `scroll_offsets`, `input_values` — so focus/keyboard cross the boundary for free).
- **Cleanup**: when the base surface hides, clear promoted popovers (shell calls
  `destroy_popups_for_parent`); prune any popup-only interaction state.

## Verification

- `cargo check --workspace` and `cargo test -p mesh-core-shell -p mesh-core-frontend-host`
  (build via `nix develop` per project setup).
- **New unit tests (component):** (a) `paint_popup` writes non-transparent pixels for a
  shown popover subtree into a buffer sized to the subtree, with subtree origin mapped to
  (0,0); (b) `handle_input_for_surface(Some(id), click@local)` activates the popover child
  whose tree-space bounds contain `local+origin` (coord-translation proof); (c) base
  `paint` leaves the promoted subtree's region empty in the base buffer.
- **New unit tests (shell):** (d) a popup `surface_id` resolves via
  `component_index_for_surface` to its owner; (e) `render_components` iterates
  `popup_targets()` and calls `configure_popup`/`present` per target (assert against a
  test/stub presentation path; `configure_popup` is a no-op off-Wayland so the loop is
  exercisable headless); (f) a popup id returned by `take_dismissed_popups` drops the
  matching `runtime.popups` entry.
- Reuse the real-surface harness in
  `component/tests/integration/real_surfaces.rs` (StubSurface) to assert end-to-end that
  a component with a shown `<popover>` reports one `popup_target` and paints it.
- Live compositor (wlroots/Hyprland/KDE) behavior is not checkable here; record that as a
  manual follow-up, consistent with Stage 2.

## Notes
- Update `todo.md` (check off the Stage 3 bullet) and the `[[project_popover_promotion]]`
  memory when done.
- Keeps core a wiring layer: no service logic; the component (one VM) does the rendering,
  the shell routes surfaces.
