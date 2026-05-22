---
spike: 002
name: painter-engine-roadmap
type: standard
validates: "Given the request for a compact web-style painter engine compatible with current XML/CSS/token styling, when decomposed for autonomous LLM execution, then the roadmap defines bounded phases with clear ownership, acceptance criteria, and browser-scope exclusions"
verdict: VALIDATED
related: [001]
tags: [renderer, painter-engine, roadmap, xml-css-token, autonomous]
---

# Spike 002: Painter Engine Roadmap

## What This Validates

Given MESH's current retained renderer, XML/.mesh authoring model, CSS-like
style parser, token styling, animation support, and Skia painter direction, when
the requested painter engine is decomposed into autonomous work, then the result
is a bounded roadmap that supports practical web-style rendering features
without becoming a full browser engine.

## Research

No new external dependency research was needed for this spike. It builds on:

| Source | Finding | Impact |
|--------|---------|--------|
| Spike 001 | `skia-safe` can render MESH-like retained display-list primitives in this repo environment. | Skia remains the concrete painter backend target. |
| Phase 51 | The backend-neutral painter command model, capabilities, diagnostics, helper migration map, and Vello compatibility notes already exist. | The new roadmap starts after the contract boundary rather than replacing it. |
| Current renderer docs | MESH owns retained identity, style/layout, animation state, display-list ordering, damage, presentation, and diagnostics. | Roadmap phases must not move render-engine ownership into Skia. |
| v1.2/v1.4/v1.5 history | CSS-like styling, token resolution, animation, retained display lists, damage, and shipped-surface smoothness already exist. | The painter engine should preserve and deepen current semantics, not restart from browser primitives. |

**Chosen approach:** update the active v1.10 roadmap into a painter-engine
roadmap with autonomous phase seeds. Keep Phase 51 complete, then split the
remaining work into style profile, element lowering, Skia primitives, effects,
animation, damage/stacking, observability/rollback, and shipped proof.

## How to Run

This is a planning spike. The next executable command is:

```bash
$gsd-plan-phase 52
```

The expected flow is:

```text
Phase 52 -> plan XML/CSS/token style profile compatibility
Phase 53 -> plan element and retained display-list primitive coverage
Phase 54 -> plan Skia shape/path/border migration
Phase 55 -> plan effects/layers/shadows/blur/images/gradients
Phase 56 -> plan animation and transition paint integration
Phase 57 -> plan stacking/clipping/visual-bounds/damage correctness
Phase 58 -> plan backend diagnostics/capabilities/rollback
Phase 59 -> plan shipped-surface proof and docs
```

## What to Expect

The roadmap now treats the painter engine as a compact shell UI rendering
engine:

- compatible with current XML/.mesh source, CSS-like styling, and theme tokens;
- capable of elements, controls, borders, paths, selection highlights, shadows,
  blur, filters, opacity, layers, images, gradients, and supported animations;
- strict about diagnostics for unsupported browser-style features;
- explicit that full HTML/CSS/DOM/browser compatibility is out of scope;
- ready for autonomous phase planning because every phase has task seeds and
  success criteria.

## Investigation Trail

- Loaded the spike workflow and existing spike manifest.
- Reviewed Spike 001 to avoid repeating Skia feasibility work.
- Reviewed the active v1.10 roadmap, requirements, state, and Phase 51 context.
- Found that the active roadmap was Skia-centric and explicitly deferred broad
  animation work, which did not fully match the user's requested painter engine.
- Expanded v1.10 from phases 51-55 to phases 51-59 while preserving completed
  Phase 51.
- Added requirements for XML/CSS/token style compatibility, element lowering,
  text boundary preservation, effects, animation, damage, diagnostics, rollback,
  and shipped proof.
- Added autonomous execution rules to the roadmap so later LLM plans have clear
  ownership and scope constraints.

## Results

Validated.

Artifacts updated:

- `.planning/ROADMAP.md`
- `.planning/REQUIREMENTS.md`
- `.planning/PROJECT.md`
- `.planning/STATE.md`
- `.planning/spikes/MANIFEST.md`
- `.planning/spikes/002-painter-engine-roadmap/README.md`

Key decision: MESH should implement a bounded shell UI subset of web-style
rendering. The painter engine supports practical CSS-like features needed by
MESH surfaces, but does not adopt arbitrary HTML parsing, DOM APIs, browser
layout modes, network resource loading, or web compatibility quirks.
