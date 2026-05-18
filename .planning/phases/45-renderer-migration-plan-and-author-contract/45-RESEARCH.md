# Phase 45 Research - Renderer Migration Plan and Author Contract

## Research Complete

Phase 45 is a documentation and planning phase. The implementation should produce source-backed docs that translate Phase 42 through Phase 44 renderer evidence into a future migration roadmap, a renderer ownership classification, and an author-facing `.mesh` renderer contract.

## Inputs Read

- `.planning/phases/45-renderer-migration-plan-and-author-contract/45-CONTEXT.md`
- `.planning/ROADMAP.md`
- `.planning/REQUIREMENTS.md`
- `.planning/STATE.md`
- `.planning/phases/43-comparable-renderer-prototype-proofs/43-PROTOTYPE-COMPARISON.md`
- `.planning/phases/44-selected-renderer-proof-integration/44-INTEGRATION-EVIDENCE.md`
- `.planning/phases/44-selected-renderer-proof-integration/44-VERIFICATION.md`
- `crates/core/frontend/render/README.md`
- `crates/core/frontend/compiler/README.md`
- `docs/frontend/html-css-transition.md`
- `docs/frontend/mesh-syntax.md`
- `docs/module-system.md`
- `docs/extensibility.md`

## Planning Findings

### Artifact Set

The cleanest executable split is three documentation artifacts:

1. `docs/renderer-migration.md` - broad phased migration roadmap, reversibility model, rollout gates, dependency implications, feature flag expectations, CI checks, binary/build risk, and rollback criteria.
2. `docs/renderer-ownership.md` - authoritative/adapter-owned/replacement-candidate classification for current renderer, shell, compiler, presentation, proof, and candidate crate boundaries.
3. `docs/frontend/renderer-contract.md` - author-facing explanation of what renderer migration means for `.mesh` UI, shipped shell surfaces, stable semantics, and explicit non-goals.

`docs/frontend/mesh-syntax.md` and `docs/module-system.md` should link to the author contract so plugin authors can find it from existing authoring docs.

### Current Boundaries To Preserve

`crates/core/frontend/render/README.md` states the current split:

- `mesh-core-component` parses author-facing `.mesh` source.
- `mesh-core-frontend` compiles and lowers frontend source into widget trees.
- `mesh-core-elements` exposes retained widget/tree/style/layout APIs.
- `mesh-core-render` paints widget trees into `PixelBuffer`s.
- `mesh-core-presentation` presents `PixelBuffer`s through dev-window or layer-shell backends.
- `mesh-core-shell` glues runtime events, service state, surface config, frontend output, rendered frames, and presentation events.

The migration plan should keep this split as the baseline and mark replacements as later gated work, not as current authority.

### Phase 44 Evidence To Carry Forward

Phase 44 verified that the focused proof path:

- preserves `NodeId` and `stable_node_id`,
- exposes dirty geometry/material/text/accessibility categories,
- preserves damage/profiling payloads,
- routes focused proof diagnostics as non-fatal degraded diagnostics,
- preserves theme-owned selection colors and selection geometry,
- derives AccessKit-compatible update IDs from retained nodes,
- preserves shipped navigation/audio surface behavior.

These are migration gates. New renderer paths must preserve or replace each behavior before becoming authoritative.

### Author Contract Boundaries

`docs/frontend/mesh-syntax.md` already states the current public authoring model:

- `.mesh` single-file components use `<template>`, `<script lang="luau">`, and `<style>`.
- Built-in tags are MESH UI primitives, not HTML compatibility tags.
- PascalCase tags are imported custom components.
- Interface proxies and service state are consumed through `mesh.*` imports / service proxy behavior.
- Element metrics are available through `refs`, not through browser DOM APIs.

The Phase 45 author contract should preserve that surface and explicitly state that renderer migration does not make `.mesh` into HTML/CSS browser content, expose proof snapshots as public APIs, or promise arbitrary DOM/web platform behavior.

### Rollout And Dependency Guardrails

The roadmap must include a required checklist before broad adoption:

- feature flag or equivalent reversible path,
- Nix/Linux/native dependency impact,
- root workspace dependency additions,
- binary-size/build-time risk note,
- workspace tests and focused renderer proof tests,
- shipped navigation/audio regression tests,
- selection proof tests,
- invalidation/damage/profiling evidence,
- AccessKit-compatible update evidence,
- documented rollback path.

## Validation Architecture

Phase 45 validation is documentation validation plus source-backed grep checks. It does not need runtime tests unless a later executor changes code unexpectedly.

Quick command:

```bash
rg -n "Reversibility|Feature flag|Linux/Nix|Binary|Rollback|MIGR-01|MIGR-02|MIGR-03" docs/renderer-migration.md docs/renderer-ownership.md docs/frontend/renderer-contract.md
```

Full command:

```bash
rg -n "authoritative|adapter-owned|replacement candidate|NodeId|typed invalidation|damage|profiling|diagnostics|AccessKit|theme-owned selection|Blitz|Taffy|Parley|AnyRender|Skia" docs/renderer-migration.md docs/renderer-ownership.md docs/frontend/renderer-contract.md
```

Coverage commands:

```bash
rg -n "renderer contract|renderer migration" docs/frontend/mesh-syntax.md docs/module-system.md
rg -n "Audio Popover Transition Delay|Define Module Install Requirement Resolution|not folded|deferred" .planning/phases/45-renderer-migration-plan-and-author-contract/45-CONTEXT.md
```

## Risks

- The migration roadmap can accidentally read like approval for immediate renderer replacement. Mitigate by requiring explicit reversible gates and preserving current authority.
- The ownership classification can drift from source layout. Mitigate by requiring exact paths and classifications for the current render/compiler/presentation/shell files.
- The author contract can overpromise browser compatibility. Mitigate by explicitly documenting `.mesh` as MESH UI primitives, not HTML/CSS browser semantics.
- Build/dependency risks can be hidden in prose. Mitigate by requiring a concrete checklist with Linux/Nix, native library, binary-size/build-time, and rollback fields.

## Recommended Plan Split

| Plan | Output | Wave | Requirements |
|------|--------|------|--------------|
| 45-01 | `docs/renderer-migration.md` | 1 | MIGR-01, MIGR-03 |
| 45-02 | `docs/renderer-ownership.md` | 1 | MIGR-02 |
| 45-03 | `docs/frontend/renderer-contract.md`, links in `docs/frontend/mesh-syntax.md` and `docs/module-system.md` | 2 | MIGR-01, MIGR-03 |

## RESEARCH COMPLETE
