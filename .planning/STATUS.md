# Status

**Updated:** 2026-08-08

This page describes the present. History lives in [`log/`](log/); open work
lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md).

## Now

Author-declared visual customization is now live as a module-system feature.
Named `mode="customizable"` slots select ordered public component placements
from profile schema 3, while ordinary slots keep automatic extension-point
behavior. Core provides generic discovery, validation, optimistic generation
checks, sparse persistence, and mounting through `mesh.composition`; it owns no
editor-specific UI.

Contribution roots now retain their authored layout and interaction styles
through click/hover repaints, including when a contributor comes from a
different module than the host. The navigation control cluster is intrinsically
measured again instead of reserving a fixed width.

Parent pointer dispatch now preserves content geometry for spanning surfaces,
and the render loop converts compositor-reported padded sizes back to content
before remeasuring, so activating a navigation control cannot feed the
tooltip-padded buffer size back into the retained layout.

The navigation bar proves the path with start, center, and end slots and author
defaults. The replaceable `@mesh/composition-editor` window is mounted by the
desk composition and can add, remove, reorder, move, reset, and configure
public scalar props. Structural, behavioral, and style changes remain normal
`.mesh`, Luau, and CSS source edits.

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

Pure paint-only scroll frames now reuse computed styles and retained layout, so
large Appearance resource lists do not rerun the full style walk on every tick.
The Appearance scroll regression covers the bounded icon/font catalog path.

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

- Node-slot parser/compiler, profile merge, module graph, and request mapping:
  passed.
- Shipped frontend compilation, navigation layout/raster/hover/keyboard/pointer
  behavior, and LSP manifest schema check: passed.
- `mesh-core-module`: 198 passed, 3 ignored.
- Full `mesh-core-shell --lib`: 656 passed, 125 ignored; five known fixture
  failures remain in shipped-navigation layout expectations and debug/theme
  manifest fixtures.
- `cargo fmt --all -- --check` and `git diff --check`: passed.

- Runtime-generation index correctness: passed.
- Component-memo integration: 11 passed, 3 ignored.
- Retained layout parity and sparse dirty-node release gate: passed.
- Release index gate: 3/3 runs passed.
- `mesh.packages` install/uninstall mapping compiled; shell library check passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- Appearance bounded-list and retained-scroll regressions passed; the release
  paint-only gate passed 3/3 samples.
- The current `cargo check -p mesh-core-shell` attempt is blocked before Rust
  compilation because the environment lacks the native `xkbcommon.pc` file.
