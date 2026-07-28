---
phase: 88-action-and-text-input-controls
title: Action And Text Input Controls Validation
status: planned
---

# Phase 88 Validation

## Nyquist Coverage Targets

| Requirement | Validation |
|-------------|------------|
| ELEMACTION-01 | Metadata/compiler tests cover `button`; compatibility tags remain accepted and lower to `button`. |
| ELEMACTION-02 | Metadata/tests cover pressed, disabled, default, destructive, busy, and keybind-aware button state. |
| ELEMACTION-03 | Existing and focused tests prove pointer activation, keyboard activation, accessibility role, and Luau handler dispatch. |
| ELEMTEXT-01 | Compiler/tests cover `input`, `textarea`, `search`, and `password` source variants configuring runtime `input`. |
| ELEMTEXT-02 | Compiler/tests cover `number-input` and `stepper` source variants plus min/max/step metadata. |
| ELEMTEXT-03 | Diagnostics/tests cover value, placeholder, disabled, read-only, required, invalid, input, and change behavior. |
| ELEMTEXT-04 | Shell/input tests prove focus traversal, selection/clipboard, and editing remain coherent. |
| ELEMTEXT-05 | Focused tests cover editing, value events, validation diagnostics, traversal, and accessibility metadata. |

## Commands

- `nix develop -c cargo test -p mesh-core-elements phase88`
- `nix develop -c cargo test -p mesh-core-frontend phase88`
- `nix develop -c cargo test -p mesh-core-interaction phase88`
- `nix develop -c cargo test -p mesh-core-shell input`
- `nix develop -c cargo test -p mesh-tools-lsp phase88`
- `nix develop -c cargo test -p mesh-core-elements -p mesh-core-component -p mesh-core-frontend -p mesh-core-interaction -p mesh-tools-lsp`
- `nix develop -c cargo fmt --check`

## Exit Criteria

- Phase 88 docs and plans are committed.
- All implementation summaries are written.
- Verification report is `status: passed`.
- `ROADMAP.md`, `STATE.md`, and `REQUIREMENTS.md` mark Phase 88 complete.
