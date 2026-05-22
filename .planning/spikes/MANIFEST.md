# Spike Manifest

## Idea

Investigate and shape MESH's painter-engine direction: a Skia-backed,
backend-neutral, retained painter pipeline that can support practical shell UI
features from the current XML/.mesh, CSS-like styling, theme tokens, animation
system, retained display list, damage policy, profiling, and shell presentation
architecture without becoming a full browser engine.

## Requirements

- Skia work is a high-priority next-milestone direction after v1.5, but production migration requires benchmark and visual-correctness proof.
- The first spike must stay isolated from production renderer code and prove build/runtime feasibility before touching `mesh-core-render`.
- Any Skia backend must consume MESH-like retained display-list commands rather than replacing `.mesh`, layout, input, module, or service architecture.
- The painter engine must remain compatible with current XML/.mesh authoring,
  CSS-like style parsing, and token styling.
- MESH should implement a bounded shell UI subset of web-style rendering, not
  arbitrary HTML/CSS/DOM/browser compatibility.
- Autonomous implementation phases need explicit ownership boundaries,
  diagnostics expectations, verification commands, and rollback or compatibility
  paths.

## Spikes

| # | Name | Type | Validates | Verdict | Tags |
|---|------|------|-----------|---------|------|
| 001 | skia-retained-display-list-painter | standard | Given a standalone Rust harness with MESH-like display-list primitives, when rendered through `skia-safe` into a CPU raster surface, then it produces deterministic pixels and can be built in this repo environment | VALIDATED | skia, renderer, spike, cpu-raster |
| 002 | painter-engine-roadmap | standard | Given the request for a compact web-style painter engine compatible with current XML/CSS/token styling, when decomposed for autonomous LLM execution, then the roadmap defines bounded phases with clear ownership, acceptance criteria, and browser-scope exclusions | VALIDATED | renderer, painter-engine, roadmap, xml-css-token, autonomous |
