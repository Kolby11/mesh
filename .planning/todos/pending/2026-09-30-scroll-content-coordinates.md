# Scroll content coordinates and retained raster

**Status:** design for wave 4, items 14–15, 2026-09-30. Item 14 is the next
implementation; item 15 depends on its coordinate and invalidation contract.
This is the scroll-specific part of the Section 12 transform/clip backlog
item, not a second transform system.

## Current cost and correctness constraints

`child_transform` in elements composes the scroll translation into the
cumulative affine transform. `collect_display_entries_with_transform` stores
surface-space bounds and signatures, and `build_paint_subtree_with_transform`
stores that transform and the ancestor clip stack in every paint node.
`RenderObjectFingerprint::geometry_slot` mixes offsets with layout and content
extent. A scroll therefore disables sparse entry patching and clean descendant
reuse, even when every child's authored input is unchanged.

Merely allowing reuse would retain stale geometry. Merely adding a painter
translation would also leave stale damage, blur regions, and replay spans.
Build-time viewport pruning currently discards offscreen commands; reusing that
pruned stream would omit newly exposed content. `DamageRect` is unsigned and
surface-clamped, so it must not become the storage type for content bounds.
Child layout coordinates follow the existing elements contract; do not assume
that resetting to identity produces coordinates relative to the scroll box.

## Item 14: one retained coordinate scope

Represent each scroll content scope by an owner ID, parent scope, affine mapping
to its parent, and viewport clip anchored in the parent's coordinates. Keep
floating, signed content bounds until projecting into surface/device coverage.
Use the existing `AffineTransform`, `AffineClip`, and device edge rounding.
The node's own background, border, and scrollbars remain outside its content
scope. Overflow clips anchor before the translation. Nested scopes compose;
fixed descendants reset to the surface coordinate and clip contract already
used by paint and hit testing, even if an ancestor scroll scope is open.

Separate scroll offsets from layout/extents in the dirty contract. Preserve
changes to layout, scroll limits, child order, resources, and effects as their
existing dirt categories. An offset-only update changes a scope transform and
the viewport's damage, plus scrollbar state; it does not change descendant
paint inputs, content signatures, or retained command segments. Mixed updates
must still rebuild the affected content. Callers without an authoritative dirty
summary retain the conservative rebuild path.

Commands and entry signatures inside the scope reference content coordinates.
Retained replay scopes carry transforms and clips independently of immutable
content. Replay applies one balanced translation around content; selection
must retain matching scope boundaries and any effect dependencies. Reusing an
`Arc` of content cannot copy/project every descendant into a new world-space
command list each frame. Project bounds for selection/damage through the scope
table, and derive blur regions from the same mapping. Compositor regions must
advance their generation when a scope moves even when content signatures do
not change. Input continues using the shared elements affine traversal.

Retain all explicitly visible commands in a scroll scope, including content
outside the current viewport. Cull against the viewport during replay using
retained content bounds. Keep ordinary pruning outside those scopes. Measure
the memory increase for the real Appearance page and a long list; virtualization
is separate work, and this change must not introduce an unbounded raster.

Implement in dependency order: offset-specific dirt; scope geometry and retained
content bounds; builder/entry retention; balanced painter replay and selection;
damage/effect generation. Keep conservative fallback for unsupported or mixed
cases until parity tests cover them. Do not relax the existing reuse guard
globally before all its world-space consumers have migrated.

## Verification and performance gate

First lock retained-versus-fresh pixel equivalence for scrolling content into
view, nested scrolls, fixed descendants, transformed containers, fractional
offsets/scales, overflow clips, shadows, opacity/filter layers, and in-surface
backdrops. Include scroll plus a child material/text change, resource revision,
resize, and both reverse and clamped scrolling. Compare partial repaint with a
fresh full render, not only two retained lists using the same code.

The structural gate requires unchanged descendant command `Arc`s and content
signatures across offset-only updates, with no descendant entry reconstruction.
Measure update and paint separately. Repeat the existing release
`settings_scroll_frame_gate` (920×900 Appearance page, 240-entry catalog,
60 gesture and 30 momentum frames) at least three alternating runs against
checkpoint `1253ce1d`; retain the one-pass/no-restyle assertions. Record ranges,
command rebuild counts, resident memory, and workload in the performance log.

## Item 15: shift retained scrolled pixels

After item 14, cache a byte-bounded viewport raster per eligible scope. Reuse
only when content/resource/effect generations, viewport geometry, scale, clip,
and paint order match, and the displacement is an integral device-pixel
translation. Shift retained pixels with overlap-safe copying, repaint exposed
strips, and replay fixed overlays/scrollbars separately. Damage still includes
the moved viewport for presentation: less raster work does not mean unchanged
screen pixels. Large jumps, fractional displacement, changed content, nested
effects, or backdrop dependencies take ordinary replay until individually
proven safe. Bound bytes, invalidate on removal/scale/resource changes, and
test strip edges and diagonal shifts against full replay before measuring.

Wave 5 follows separately: the partial-repaint backdrop pixel test comes before
its spec and cache implementation, as in the in-surface blur plan.
