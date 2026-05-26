---
phase: 91-shell-proof-docs-and-hardening
title: Shell Proof Docs And Hardening Context
status: planned
requirements:
  - ELEMPROOF-01
  - ELEMPROOF-02
  - ELEMPROOF-03
  - ELEMPROOF-04
  - ELEMPROOF-05
  - ELEMPROOF-06
---

# Phase 91 Context

## Goal

Finish the v1.16 native element milestone by proving shipped shell surfaces use the element library, hardening docs/accessibility/diagnostics, and preparing the milestone for audit and cleanup.

## Locked Decisions

- Do not create a separate gallery/proof UI.
- Aggressively migrate shipped navigation/audio/debug/quick-settings where the migration is low-risk and preserves behavior.
- Prioritize visual/UI polish over unrelated broad-suite cleanup.
- Run milestone audit and complete/cleanup if verification passes.

## Current Proof Coverage

- Navigation uses native `button`, child `icon`, `select`, and `option`.
- Audio popover uses native `slider`, `button`, and icons.
- Debug inspector uses native `tabs`, `tab`, `list`, `list-item`, and `empty-state`.
- Text-selection proof covers selectable text.

## Out Of Scope

- New gallery/debug proof surface.
- Full modal focus trap/backdrop.
- Rich table/tree native behavior.
- Fixing unrelated existing broad-suite failures unless they block Phase 91 proof.
