---
phase: 87-layout-and-display-elements
title: Layout And Display Elements Validation
status: planned
---

# Phase 87 Validation

## Nyquist Coverage Targets

| Requirement | Validation |
|-------------|------------|
| ELEMLAYOUT-01 | Metadata tests assert `box`, `row`, `column`, `grid`, `stack`, `spacer`, `divider`, and `scroll-area` are present. Compiler tests assert source tags lower safely and preserve source semantics. |
| ELEMLAYOUT-02 | Tests cover layout attributes for alignment, spacing, sizing, overflow, scroll, grid tracks, grid placement, and stack layering metadata. |
| ELEMLAYOUT-03 | Existing package and shipped surface tests continue to pass for current `box`, `row`, `column`, scroll, and tooltip/title behavior. |
| ELEMLAYOUT-04 | Metadata/compiler tests cover `section`, `header`, `footer`, `group`, and `form-row` roles, labels, and style hooks. |
| ELEMLAYOUT-05 | Diagnostic tests cover invalid layout child/attribute combinations, unsupported complex grid syntax, and structure misuse. |
| ELEMDISPLAY-01 | Metadata/compiler/LSP tests cover `text`, `icon`, `image`, `badge`, `progress`, `tooltip`, `avatar`, and `shortcut`; `meter` remains taxonomy/docs-only. |
| ELEMDISPLAY-02 | Runtime/compiler tests cover accessible labels, roles, value metadata, and style hooks for display primitives. |
| ELEMDISPLAY-03 | `progress` tests cover determinate and indeterminate state plus min/max/current values. `meter` deferral is documented. |
| ELEMDISPLAY-04 | Existing tooltip tests plus one semantic `<tooltip>`/tooltip-attribute test prove pointer/keyboard ownership remains accessible. |
| ELEMDISPLAY-05 | Focused tests are added in `mesh-core-elements`, `mesh-core-component`, `mesh-core-frontend`, `mesh-tools-lsp`, and shell tooltip tests where needed. |

## Commands

- `nix develop -c cargo test -p mesh-core-elements phase87_layout_display_contract`
- `nix develop -c cargo test -p mesh-core-component planned_native_tags`
- `nix develop -c cargo test -p mesh-core-frontend phase87_layout_display`
- `nix develop -c cargo test -p mesh-tools-lsp phase87`
- `nix develop -c cargo test -p mesh-core-shell shipped_navigation_volume_icon_inherits_button_click_and_tooltip`
- `nix develop -c cargo test -p mesh-core-elements -p mesh-core-component -p mesh-core-frontend -p mesh-tools-lsp`
- `nix develop -c cargo fmt --check`

## Exit Criteria

- All Phase 87 docs and plans exist and are committed.
- All three implementation plans have summaries.
- Verification report is `status: passed`.
- `ROADMAP.md` and `STATE.md` mark Phase 87 complete.
