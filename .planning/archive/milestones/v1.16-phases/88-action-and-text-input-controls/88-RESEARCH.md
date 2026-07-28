---
phase: 88-action-and-text-input-controls
title: Action And Text Input Controls Research
status: complete
---

# Phase 88 Research

## Findings

### Action Controls

The runtime already has a single `button` primitive. `lower_source_tag` maps `button`, `icon-button`, `toggle-button`, `command-button`, and `link-button` to `button`. Pointer click targeting and keyboard activation both route through the same click handler path.

Research implication: Phase 88 should reinforce one native button behavior and add metadata/diagnostics/docs around configured button use cases, not add runtime primitives.

### Button Content

Existing markup supports child elements inside a button, and icon rendering is already a dedicated `icon` primitive. Putting an icon inside button markup keeps composition explicit and avoids duplicating icon attributes on buttons.

Research implication: docs and LSP should guide authors toward `<button><icon ... /><text>...</text></button>`.

### Text Input Variants

The compiler already lowers `input`, `textarea`, `search`, `password`, `number-input`, `stepper`, and compatibility text input tags to runtime `input`. `default_input_type` sets type metadata for many variants.

Research implication: Phase 88 should add missing source metadata, validation, and tests around the existing input path instead of creating variant widgets.

### Shell Input Runtime

Shell input handling already supports focus, typed character insertion, backspace/delete, enter/change dispatch, selection copy, focus traversal, keyboard handlers, and value preservation keyed by runtime node key. Numeric stepping behavior exists for sliders and can provide a pattern for number/stepper constraints without introducing another native widget.

Research implication: keep focus/traversal/selection behavior unchanged, add narrow tests proving source variants remain coherent.

### LSP And Docs

LSP tag knowledge is hand-maintained. Phase 87 added source tags there, so Phase 88 should update the same table with single-button guidance and input variant attributes.

Research implication: add completion metadata and tests rather than attempting generation from element contracts in this phase.

## Risks

| Risk | Mitigation |
|------|------------|
| Compatibility button tags imply separate native behavior | Docs/LSP mark them as configured-button aliases and tests assert they lower to `button`. |
| Button-level icon shortcuts duplicate `<icon>` | Do not add `icon`, `name`, or `src` to button metadata; document child markup. |
| Numeric controls drift from existing input handling | Validate numeric attributes in metadata and keep runtime as `input`. |
| Textarea implies full multiline editing | Preserve source semantics but diagnose unsupported multiline editing if needed. |

## Recommended Plan Shape

1. Add element contract metadata and diagnostics for one-button action controls and configured input variants.
2. Extend compiler/accessibility/runtime tests for source semantics, events, focus, keyboard, and numeric constraints.
3. Update LSP/docs and close with focused/package verification.
