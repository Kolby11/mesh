# 98-02 Summary: Service Event Field-Level Fan-Out

**Plan:** 98-02-PLAN.md
**Status:** Complete
**Date:** 2026-06-09

## What was built

Added field-level service event filtering to `handle_service_event()`:

- **Previous payload capture** — `cached_service_payloads.get()` is called BEFORE `insert()` to save the previous payload for diffing.

- **`json_field_diff()`** helper in `component.rs` — Compares two JSON payload objects and returns `Vec<(service, field)>` of changed keys (added, removed, or modified). Non-object payloads return empty diff.

- **Narrow eligibility check** — After diff, each `(service, field)` pair is checked against `NodeServiceFieldDependencies::nodes_reading_field()`. If any pair has a non-empty result, `invalidate_script_state_narrow()` is called instead of `invalidate_script_state()`. If no template node reads any changed field, full `invalidate_script_state()` is used (preserving the Lua-side `tracked_service_fields_changed` fallback).

## Key files modified

| File | Change |
|------|--------|
| `shell_component.rs` | Modified `handle_service_event()` with payload capture + narrow eligibility routing |
| `component.rs` | Added `json_field_diff()` helper |
| `tests/invalidation/service_fanout.rs` | New test file with 6 `json_field_diff` unit tests |
| `tests/invalidation.rs` | Added `mod service_fanout` |

## Self-Check: PASSED

The field-level diff correctly identifies added/removed/modified keys. The intersection check uses the bidirectional `NodeServiceFieldDependencies` index from Phase 97. The Lua-side `tracked_service_fields_changed()` fallback is preserved.
