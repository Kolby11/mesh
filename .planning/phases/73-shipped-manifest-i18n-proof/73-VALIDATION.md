---
phase: 73
status: planned
created: 2026-05-24
---

# Phase 73 Validation Strategy

| ID | Plan | Requirement(s) | Claim | Test Type | Command | Status |
|----|------|----------------|-------|-----------|---------|--------|
| T-73-01 | 01 | MPROOF-01, MPROOF-02 | Shipped navigation manifest uses explicit localized keybind text and no suspicious raw i18n diagnostics | unit | `cargo test -p mesh-core-module shipped_navigation_manifest_uses_explicit_localized_keybind_text -- --nocapture` | pending |
| T-73-02 | 01 | MPROOF-03 | Installed graph preserves shipped navigation keybind translation keys | unit | `cargo test -p mesh-core-module shipped_module_graph_preserves_navigation_localized_keybind_text -- --nocapture` | pending |
| T-73-03 | 01 | MPROOF-03 | Real navigation runtime/debug metadata resolves shipped localized keybind text | unit | `cargo test -p mesh-core-shell navigation_shipped_keybind_metadata_resolves_from_i18n_catalogs -- --nocapture` | pending |
| T-73-04 | 01 | MPROOF-04 | Author docs explain manifest i18n support and field-local translation objects | docs | `rg -n 'field-local localized text|Raw strings are literals|mesh.contributes.i18n' docs/module-system.md` | pending |
