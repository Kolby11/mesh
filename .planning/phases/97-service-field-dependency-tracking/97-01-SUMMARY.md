# Plan 97-01 Summary: service_field_reads + TrackingVariableStore

**Completed:** 2026-06-09
**Status:** Done

## What Was Done

- Added `pub service_field_reads: Vec<(String, String)>` as the last field on `WidgetNode` in `crates/core/ui/elements/src/tree.rs`
- Initialised it to `Vec::new()` in `WidgetNode::new()`
- Implemented `TrackingVariableStore<'_>` in `crates/core/frontend/compiler/src/render.rs` — wraps any `VariableStore`, records `(service, field)` for each dotted `get()` call (skips bare lookups)
- Wired `TrackingVariableStore` into `build_element_node`: per-node tracker wraps `state` for attribute evaluation and inline-content resolution; `node.service_field_reads` harvested after both paths; children use original `state`
- Wired `TrackingVariableStore` into `TemplateNode::Expr` arm: text nodes now have `service_field_reads` populated
- Fixed pre-existing missing `use std::collections::HashMap` in `lib.rs` test module

## Test Results

- `tree::tests::new_widget_node_has_empty_service_field_reads` — passes
- `tracking_store_records_dotted_reads` — passes
- `tracking_store_skips_bare_reads` — passes
- `tracking_store_no_cross_contamination` — passes
- `mesh-core-elements`: 102 passed, 0 failed
- `mesh-core-frontend`: 33 passed, 0 failed

## Requirements Satisfied

- SRV-01: Template evaluator records per-node (service, field) pairs during render
