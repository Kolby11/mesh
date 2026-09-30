# Bounded scroll raster cache

**Status:** wave 4 item 15 design, 2026-09-30. Content-space scopes have landed;
the implementation and measurements are in the performance log. This note
describes remaining raster work, not another backlog.

## Admission and ownership

Cache a rectangular, transparent content raster per eligible scroll scope,
not the whole surface and not its background, border, or anchored scrollbars.
Apply the owner's rounded clip when compositing the raster. Baking that mask
into the cache and shifting it would move its corners and create edge holes.
Use the existing resource byte budget/cache mechanisms. Bound aggregate bytes
and entry count, reject oversized allocations, and evict removed scopes.

Initially admit only the translation-only scopes already supported by the
retained display list, without parent filter layers, backdrop dependencies,
fixed overlays, nested scopes, or filtered paint-node selection. Nested
scrollers inside a scope retain baked inner geometry: an inner offset change
invalidates the content raster. Complex scopes keep ordinary replay; the
general transform/clip work remains in Section 12 of the backlog.

## Cache contract

Identity includes owner, content command generations/handles, resource revision,
viewport geometry, scale, clip geometry and paint order. A scope translation
alone must not change content identity. Keep signed floating content bounds;
only coverage and displacement at the raster boundary become device pixels.
Do not cache readback-dependent effects or compose cached transparent pixels
with source-copy over the existing surface background.

Reuse only for integral physical-pixel displacement with unchanged raster
dimensions and identity. Fractional displacement, resize, scale/resource/content
changes, large jumps, and unsupported dependencies take full content replay.
Use overlap-safe row copying. Clear newly exposed pixels to transparent and
repaint at most two disjoint exposed strips for diagonal movement. Then source-
over composite the raster in command order and replay anchored scrollbars.
Damage still includes the entire moved viewport for presentation.

Partial surface damage must not mark an incompletely painted cache valid.
Cache construction and strip repair paint the required raster region independent
of the surface's selected damage clips. Keep profiling and ordinary loops
semantically identical, and expose cache hit/miss/repaint-area measurements.

## Verification

Start with an independent byte oracle for every small integral shift, including
diagonal, reverse, zero, extreme signed values, jumps beyond the viewport,
transparent pixels, and disjoint strip coverage. Then compare cache-backed
partial paint with cache-disabled full paint at scales 1, 1.25 and 2, including
fractional fallback, rounded edges, overlapping translucent widgets, nested
scrollbars, mixed content edits, resource revision, resize and cache eviction.

Repeat `settings_scroll_frame_gate` in release with its 920×900 Appearance
surface, 240-entry catalogs, 60 gesture plus 30 momentum frames. Collect at
least three alternating runs against the item-14 commit, frame p50/p90, separate
update/paint timing, repainted pixels, cache bytes and process memory. Keep the
one-pass/no-restyle and command-retention gates. Fractional momentum may remain
uncached: measure hit rate rather than claiming every frame benefits.

Wave 5 follows independently, starting with its partial-repaint blur pixel
regression before a detailed in-surface blur spec/cache contract.
