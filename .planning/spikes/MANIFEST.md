# Spike Manifest

## Idea

Track focused technical spikes that de-risk MESH architecture decisions before
they become milestone work. Earlier spikes shaped the painter-engine direction;
Spike 003 investigates the backend service data, command, and event contract.

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
- Backend services should have three explicit lanes: full current-state
  snapshots for durable data, commands for frontend-to-backend mutation, and
  typed events only when transient facts are genuinely needed.
- Interface-declared events must either become runtime-delivered events or be
  documented as deferred metadata so authors do not assume they can subscribe to
  them.

## Spikes

| # | Name | Type | Validates | Verdict | Tags |
|---|------|------|-----------|---------|------|
| 001 | skia-retained-display-list-painter | standard | Given a standalone Rust harness with MESH-like display-list primitives, when rendered through `skia-safe` into a CPU raster surface, then it produces deterministic pixels and can be built in this repo environment | VALIDATED | skia, renderer, spike, cpu-raster |
| 002 | painter-engine-roadmap | standard | Given the request for a compact web-style painter engine compatible with current XML/CSS/token styling, when decomposed for autonomous LLM execution, then the roadmap defines bounded phases with clear ownership, acceptance criteria, and browser-scope exclusions | VALIDATED | renderer, painter-engine, roadmap, xml-css-token, autonomous |
| 003 | backend-data-event-contract | standard | Given MESH backend providers publish service state and receive service commands, when the runtime bridge and frontend proxy paths are traced, then the data and event contract is clear enough to complete and harden | PARTIAL | backend, services, events, state, luau, runtime |
