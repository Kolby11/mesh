# Spike Manifest

## Idea

Investigate whether a Skia-backed renderer can materially improve MESH rendering performance by replacing or supplementing the current custom/tiny-skia/resvg/cosmic-text/swash low-level paint path while preserving the retained widget tree, render-object tree, retained display list, damage policy, profiling, and shell presentation architecture.

## Requirements

- Skia work is a high-priority next-milestone direction after v1.5, but production migration requires benchmark and visual-correctness proof.
- The first spike must stay isolated from production renderer code and prove build/runtime feasibility before touching `mesh-core-render`.
- Any Skia backend must consume MESH-like retained display-list commands rather than replacing `.mesh`, layout, input, module, or service architecture.

## Spikes

| # | Name | Type | Validates | Verdict | Tags |
|---|------|------|-----------|---------|------|
| 001 | skia-retained-display-list-painter | standard | Given a standalone Rust harness with MESH-like display-list primitives, when rendered through `skia-safe` into a CPU raster surface, then it produces deterministic pixels and can be built in this repo environment | VALIDATED | skia, renderer, spike, cpu-raster |
