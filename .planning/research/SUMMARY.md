# Research Summary: v1.8 Rendering Engine Architecture

## Recommendation

Make v1.8 an architecture decision and proof milestone. Do not commit to replacing MESH's renderer wholesale until Blitz and a MESH-owned focused-crate path have been compared on the same shipped-surface slice.

## Stack Additions to Evaluate

- Blitz as the reference architecture and direct-adoption candidate.
- Taffy for layout.
- Parley for text layout, selection, and future editor needs.
- Skia/rust-skia or AnyRender for paint/backend abstraction.
- AccessKit for accessibility updates.
- Stylo only through Blitz or a tightly scoped style proof.
- Winit, Muda, html5ever, and xml5ever only if the milestone proves they solve a concrete MESH shell/rendering need.

## Feature Table Stakes

- Adopt-vs-build decision for Blitz.
- Prototype evidence for both Blitz and focused-crate paths.
- Retained invalidation, damage, profiling, diagnostics, and accessibility remain visible.
- One real shipped surface renders through the chosen proof path.
- Build/CI/dependency cost is measured before broader migration.

## Watch Out For

- Full browser compatibility is out of scope.
- Winit is not a renderer and may not fit Wayland shell surfaces.
- Stylo and Skia add real power but also integration cost.
- Text and accessibility must be first-class acceptance criteria, not follow-up cleanup.

## Sources

- Blitz: https://github.com/DioxusLabs/blitz
- Skia: https://skia.org/docs/
- rust-skia: https://github.com/rust-skia/rust-skia
- Taffy: https://taffylayout.com/docs
- Parley: https://docs.rs/parley/latest/parley/
- Stylo: https://github.com/servo/stylo
- Winit: https://rust-windowing.github.io/winit/winit/
- AccessKit: https://www.xskit.dev/
- Muda: https://github.com/tauri-apps/muda
- html5ever: https://github.com/servo/html5ever
