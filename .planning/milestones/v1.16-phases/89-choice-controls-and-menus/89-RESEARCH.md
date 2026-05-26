---
phase: 89-choice-controls-and-menus
title: Research
status: complete
---

# Research

## Findings

- The compiler already preserves source semantics through `data-mesh-element`, which is the correct bridge for native behavior while keeping the small runtime primitive vocabulary.
- Existing input behavior is mostly runtime-tag based. Phase 89 needs helper predicates for source-aware focusable, checkable, radio, select, option, menu, and menu item detection.
- Runtime tree annotation currently writes value only for runtime `input` and checked state only for runtime `switch`/`checkbox`. Since source choice tags can lower to `input` or `row`, annotation must use source semantics too.
- The shipped language selector is a good migration proof because it already has two static options and a single service request side effect.
- The element contract already contains the taxonomy. The work is refining fields, diagnostics, and tests rather than inventing a separate element registry.

## Implementation Direction

- Keep the frontend lowering conservative; add source-aware behavior in shell helpers and compiler defaults.
- Represent static `option` children as rendered child rows/text so popup/listbox content is visible in the tree.
- Use existing event handler normalization (`onchange` -> `change`, `onactivate` -> `activate`) and dispatch through existing handler calls.
- For menu roving focus, handle ArrowUp/ArrowDown from focused menu items and skip disabled items.
- For radios, use the parent `radio-group` and `name`/`value` metadata to keep group selection exclusive.
