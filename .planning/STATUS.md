# Status

**Updated:** 2026-08-08

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

Component memoization now validates descendant state through a parent/child
runtime-generation index with aggregate subtree stamps, avoiding the former
all-runtime descendant scan while preserving nested-state invalidation.

Retained layout now receives sparse owned snapshots for layout-dirty nodes and
updates their Taffy styles/text contexts directly. The persistent Taffy mapping
and structural/unkeyed reconciliation fallback are now authoritative across
incremental frames.

The typed `mesh.packages` provider now owns module installation and removal as
well as provider selection, enablement, and profile switching. Local and Git
sources update the installed graph, lock, and profile state through the same
core path used by the CLI.

## Verification

- Runtime-generation index correctness: passed.
- Component-memo integration: 11 passed, 3 ignored.
- Retained layout parity and sparse dirty-node release gate: passed.
- Release index gate: 3/3 runs passed.
- `mesh.packages` install/uninstall mapping compiled; shell library check passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- The current `cargo check -p mesh-core-shell` attempt is blocked before Rust
  compilation because the environment lacks the native `xkbcommon.pc` file.
