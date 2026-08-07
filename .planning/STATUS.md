# Status

**Updated:** 2026-08-07

This page describes the present. History lives in [`log/`](log/); open work
lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md).

## Now

Overflow child surfaces now derive automatically for nested absolutely
positioned content that escapes the parent buffer, while explicit popover
subtrees and intentionally clipped regions remain owned by their existing
surface.

The production audio controls now live in the navigation bar's shared component
VM as an in-tree promoted popover. Core slider capture keeps drag updates alive
while the pointer crosses the popup boundary; no standalone audio surface is
enabled by the shipped graph.

Promoted legacy popovers now measure their first frame against the known parent
surface bounds while keeping the axes intrinsically sized, so the `(1, 1)` popup
positioner placeholder cannot permanently collapse cross-axis content.

The component settings boundary is clean: frontend runtimes expose declared
`props.*` and the typed `mesh.settings` service, without a raw top-level
`settings` namespace. The regression and props integration suite pass.

The retained display-list renderer is the production paint path. Legacy
PixelBuffer icon/glyph entrypoints and recursive widget helpers are restricted
to tests, eliminating the corresponding production dead-code warnings.

The Phase26 real-surface proof is deterministic under the default parallel
shell suite. Its icon-cache assertion now covers both raster-backed and
font-glyph-backed semantic icon packs. The broader shell baseline still has
known pre-existing failures; do not treat them as caused by this work.

## Verification

- `mesh-core-render`: 201 passed, 0 failed, 37 ignored.
- Props/settings integration: 7 passed.
- Phase26 real-surface proof: passed.
- `cargo check -p mesh-core-render` and `cargo fmt --all -- --check`: passed.
