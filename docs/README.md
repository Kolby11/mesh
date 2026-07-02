# MESH Documentation

Docs for the MESH shell framework. For a high-level project description see
[`../README.md`](../README.md).

## The specification

**[`spec/`](./spec/README.md)** is the unified, authoritative specification —
module system, installation & health, components & props, styling & theming,
icons, fonts, localization, settings, accessibility, keyboard, automation
IPC, and MCP. Start there for any design or contract question.

## Codebase orientation

- **[`llm-context.md`](./llm-context.md)** — crate map, module layout, key
  data flows, common task entry points. The primary orientation guide for
  working on this codebase.
- **[`crate-boundaries.md`](./crate-boundaries.md)** — crate responsibility
  boundaries.

## Frontend reference

- [`frontend/mesh-syntax.md`](./frontend/mesh-syntax.md) — `.mesh` component
  syntax: tags, interpolation, bindings, event handlers, accessibility.
- [`frontend/elements.md`](./frontend/elements.md) — native element taxonomy.
- [`frontend/slots.md`](./frontend/slots.md) — slot points for extending
  surfaces without forking them.
- [`frontend/renderer-contract.md`](./frontend/renderer-contract.md) —
  renderer expectations for module authors.
- [`frontend/html-css-transition.md`](./frontend/html-css-transition.md) —
  UI XML vocabulary and bounded CSS profile transition sketch.
- [`css-coverage.md`](./css-coverage.md) — supported CSS property coverage.

## Shipped modules

- [`modules/README.md`](./modules/README.md) — core-module index
  (frontends, backends, interface packages, examples).

## Performance & rendering internals

- [`performance-roadmap.md`](./performance-roadmap.md) — retained rendering,
  invalidation, damage tracking, GPU sequencing.
- [`renderer-migration.md`](./renderer-migration.md) /
  [`renderer-ownership.md`](./renderer-ownership.md) — renderer internals.
