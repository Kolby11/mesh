# Feature Research: v1.8 Rendering Engine Architecture

## Table Stakes

- A written adopt-vs-build decision for Blitz, backed by prototype evidence.
- A renderer abstraction boundary that preserves MESH retained identity, invalidation categories, damage tracking, profiling, diagnostics, and shell request flow.
- Layout proof on shipped surfaces using current behavior as the baseline.
- Text proof covering current text rendering plus selection/cursor geometry risks.
- Paint proof comparing current software path against Skia or AnyRender-backed output.
- Accessibility proof that retained node identity can produce stable AccessKit-style updates.
- Build and dependency proof covering Linux/Nix development, CI, binary size, and GPU/backend feature flags.

## Differentiators

- Keep MESH's shell-specific `.mesh` rendering model while borrowing proven browser-engine crates only where they pay for themselves.
- Preserve debug observability through the migration rather than treating the renderer as a black box.
- Make renderer backend selection a measured, reversible boundary instead of a one-way rewrite.

## Anti-Features

- Do not attempt full HTML/CSS/browser compatibility in this milestone.
- Do not rewrite windowing/input around Winit unless it solves a specific MESH Wayland shell problem.
- Do not adopt Stylo directly without proving that its browser-grade complexity fits MESH's constrained CSS model.
- Do not remove existing shipped-surface behavior before a new path matches it under tests and benchmarks.

## Candidate User-Visible Outcome

By the end of v1.8, MESH should not merely have a research document. It should have a small proof path showing the chosen rendering direction can render a real shipped surface with retained invalidation, text/layout behavior, accessibility metadata, and profiling still visible.
