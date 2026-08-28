# CSS-derived surface geometry

## Goal

Delete `mesh.surface.exclusive_zone` and `mesh.surface.margins` as authored
numbers. The root component's measured CSS box becomes the single source of
surface geometry, so changing a bar's height in CSS cannot leave a stale
reservation behind in `module.json`.

## Current boundary

`docs/spec/03-components.md` §2 already states that sizing lives in the root
component's CSS and is measured at paint time. Placement fields in
`SurfaceLayoutSection` contradict that for two values:

- `exclusive_zone: i32` — reserved compositor space, restated by hand.
- `margins: SurfaceMargins` — a per-edge inset, restated by hand.

Both are read in `crates/core/surface-config/src/lib.rs` into the surface
layout, then pushed to the compositor from
`crates/core/shell/src/shell/component/rendering/mod.rs` via
`set_exclusive_zone` and `set_margin`. Nothing ties either number to what the
root actually measures, so they drift silently: the shipped navigation bar went
from 56px to 40px and both numbers had to be retuned by hand in a separate
file, with no diagnostic if they had not been.

`anchor`, `layer`, `keyboard_mode`, and `visible_on_start` are genuine policy
and stay in the manifest. This item is only about the two geometric fields.

## Design

1. **Root margin means surface margin.** A layer surface has no parent box to
   sit inside, so `margin` on the surface root has no layout meaning today.
   Give it one: the resolved root `margin` edges lower to `set_margin` instead
   of to Taffy. One authored concept, one place to write it.
2. **Reservation is derived.** After the root box is measured, the anchored
   axis of its outer box (border box plus that edge's margin) is the exclusive
   zone. A top-anchored 40px bar with an 8px top margin reserves 48 without
   anyone writing 48.
3. **Overlap stays declarative, because it is policy, not geometry.** A surface
   that must not reserve space (an overlay, a HUD) needs a way to say so.
   Add one CSS-side declaration on the root — working name
   `exclusive-zone: none | auto` — defaulting to `auto`. This is the only new
   authored knob and it replaces an integer with an intent.
4. **Parametrization keeps working through props, not the manifest.** This is
   the open question from the request, and `<props>` already answers it: a
   `bar_height` prop projects into `prop(bar_height)` in the root CSS, the root
   measures to that value, and the reservation follows. The profile/settings
   override path is unchanged, and the manifest holds no geometry at all.
5. **Recommit on change, not per frame.** Measured geometry changes at runtime
   (a prop write, a container query crossing, a font-metrics change). Route the
   derived size/zone/margins through the existing surface-policy revision diff
   so an unchanged frame commits nothing.

## Risks

- **Circular measurement.** A percentage height on the root resolves against
  the surface height, which is what we are deriving. Reject it at compile time
  with a diagnostic naming the property, rather than resolving to zero.
- **Resize feedback.** Committing a height can change the available width,
  which can change the measured height again. Needs either a settle rule or a
  bounded fixed-point pass with a diagnostic when it fails to converge.
- **Compositor timing.** `set_exclusive_zone` and `set_margin` are per-commit
  layer-shell state; deriving them late must not land after the buffer attach
  for that frame.

## Migration

1. Derive both values, and let a present manifest field win with a deprecation
   diagnostic naming the CSS that would have produced the value instead.
2. Port the shipped modules to the derived path and delete their fields.
3. Remove the fields from `SurfaceLayoutSection`, leaving a manifest
   diagnostic that points at the CSS replacement.
