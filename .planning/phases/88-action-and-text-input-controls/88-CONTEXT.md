---
phase: 88-action-and-text-input-controls
title: Action And Text Input Controls Context
status: planned
requirements:
  - ELEMACTION-01
  - ELEMACTION-02
  - ELEMACTION-03
  - ELEMTEXT-01
  - ELEMTEXT-02
  - ELEMTEXT-03
  - ELEMTEXT-04
  - ELEMTEXT-05
---

# Phase 88 Context

## Goal

Implement configurable action controls and text/numeric input controls with keyboard, value, and accessibility behavior.

## Locked Decisions

- Implement one native action behavior: `button`.
- Do not implement separate runtime behavior for `icon-button`, `toggle-button`, `command-button`, or `link-button` in Phase 88.
- Icons inside buttons are authored as child markup, for example `<button><icon name="..." /><text>...</text></button>`.
- Do not add button-level `icon`, `name`, or `src` shortcut attributes.
- Keep compatibility/taxonomy tags accepted by parser/compiler, but recommend `button` plus child content and attributes.
- Only split a separate native element later if it needs different focus, event, accessibility, value, or renderer semantics.
- Use one native text input runtime path for `input`, `textarea`, `search`, `password`, `number-input`, and `stepper` source variants.
- Keep textarea editing conservative if the current input engine remains single-line; diagnose unsupported multiline editing.
- Implement numeric min/max/step validation and value coercion around the existing input value path.
- Standardize text/numeric controls on `oninput` for immediate edits and `onchange` for committed values.
- Reuse existing shell focus traversal, input focus, text selection, clipboard, and keyboard activation rules.
- Put validation diagnostics in `mesh-core-elements` and surface them through frontend diagnostics.
- Update docs and LSP; do not migrate shipped surfaces in this phase unless needed for proof.

## Current Code Shape

- `crates/core/frontend/compiler/src/tags.rs` already lowers action variants to `button` and input variants to `input`.
- `crates/core/frontend/compiler/src/render.rs` already preserves `data-mesh-element`, assigns accessibility metadata from source contracts, normalizes event handlers, and sets default `type` for input variants.
- `crates/core/ui/elements/src/element.rs` contains broad taxonomy metadata, common attributes/events, and validation hooks.
- `crates/core/ui/interaction/src/focus.rs` treats `button`, `input`, `slider`, `switch`, and `checkbox` as native focusable nodes.
- `crates/core/shell/src/shell/component/input` already handles pointer focus, keyboard activation, text editing, selection/clipboard, slider stepping, and toggle state.
- `crates/tools/lsp/src/knowledge/tags.rs` is the authoring tag/attribute completion surface.

## Out Of Scope

- Separate native behavior for `icon-button`, `toggle-button`, `command-button`, or `link-button`.
- Button-level icon shortcut attributes.
- Full multiline text editor behavior if the existing input path cannot support it cleanly.
- New URL navigation, browser form behavior, or command routing semantics.
- Shipped surface rewrites.

## Success Proof

- Button metadata and diagnostics encode the single-button model.
- Button keyboard and pointer activation continue to dispatch through existing handler paths.
- Input source variants preserve semantics and configure the existing input path.
- Numeric diagnostics validate min/max/step/value combinations.
- Focus, traversal, text selection, and clipboard behavior remain coherent.
- LSP and docs steer authors to `button` with child `<icon>`/`<text>` markup.
