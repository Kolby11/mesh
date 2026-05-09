---
status: complete
phase: 24
plan: 1
completed: 2026-05-09
---

# Summary 24-01: Typed Selector Inputs and Rule Index

## Completed

- Added `StyleNodeAttrs` typed selector inputs for tag, classes, class set, id, key, module id, and state.
- Routed selector matching through typed attributes while preserving existing public style resolver APIs.
- Added `StyleRuleIndex` for conservative candidate lookup by tag, class, id, and pseudo-state.
- Used indexed candidate lookup for normal style resolution, retained state restyle, and targeted key restyle.
- Preserved fallback behavior for universal and unknown selector dependencies.
- Added tests proving indexed selector resolution still applies tag, class, id, and compound state rules correctly.

## Files Changed

- `crates/core/ui/elements/src/style/resolve.rs`
- `crates/core/ui/elements/src/style.rs`

## Verification

- `cargo fmt --check` — passed
- `cargo test -p mesh-core-elements style_rule_index` — passed
- `cargo test -p mesh-core-elements targeted_restyle_recomputes_only_named_stateful_nodes` — passed
- `cargo test -p mesh-core-elements` — passed

## Notes

- Existing string attribute maps remain the compatibility source.
- This phase adds typed/indexed fast paths without replacing the selector engine or requiring interned-only storage.
