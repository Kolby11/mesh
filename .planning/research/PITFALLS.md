# Pitfalls Research: v1.8 Rendering Engine Architecture

## Main Risks

- **Renderer rewrite trap:** replacing too much at once would erase the retained-rendering proof accumulated in v1.3-v1.5.
- **Browser-engine scope creep:** Blitz, Stylo, html5ever, and xml5ever can pull the project toward browser compatibility that MESH does not need.
- **Lost observability:** adopting a renderer as a black box could hide invalidation, damage, timing, and diagnostic data that current MESH workflows rely on.
- **Accessibility regression:** visual proof is insufficient if retained node identity no longer maps to accessibility updates.
- **Text regression:** Parley or Skia text paths must preserve selection, copy, highlight, and future IME/editor needs.
- **Windowing mismatch:** Winit is useful for apps, but MESH is Wayland-native shell software; do not assume app-window abstractions fit panels/popovers/layer surfaces.
- **Build cost surprise:** Skia and browser-grade crates can increase build time, binary size, native dependency needs, and CI complexity.

## Prevention Strategy

- Make the first phase explicitly decision-oriented and source-backed.
- Require two comparable prototypes before choosing direct Blitz adoption or a MESH-owned path.
- Keep all shipped-surface proofs tied to existing benchmarks and live interaction expectations.
- Add acceptance criteria for profiling payloads, accessibility metadata, diagnostics, and build/CI cost.
- Treat HTML/XHTML parsing as optional document input, not as a default authoring model.

## Phase Placement

- Phase 42 should produce the decision scorecard and prototype boundaries.
- Phase 43 should prove layout/text/style feasibility.
- Phase 44 should prove paint/backend feasibility.
- Phase 45 should integrate the chosen path behind a constrained feature flag or proof surface.
- Phase 46 should document migration and decide whether broad rollout is ready.
