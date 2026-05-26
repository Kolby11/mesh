---
phase: 79
phase_name: automatic-rerender-dependencies
status: complete
created: 2026-05-26
requirements:
  - LUARERENDER-01
  - LUARERENDER-02
  - LUARERENDER-03
  - LUARERENDER-04
  - LUARERENDER-05
  - LUARERENDER-06
---

# Phase 79: Automatic Rerender Dependencies - Context

## Existing Implementation

- Service proxy field reads already record top-level field dependencies through `tracked_service_fields`.
- Shell service updates compare changed payload fields against tracked fields before requesting script rebuild.
- Locale changes already reset component runtime/render caches and invalidate script state.
- Theme changes already reset retained render caches, mark render hooks pending, and invalidate script state.
- Bound child instance refresh writes into parent runtime state, which marks state dirty and invalidates the tree after handler execution.
- `mesh.request_redraw` and explicit invalidation paths remain available as compatibility/debug escape hatches.

## Storage

`self.storage` dependency tracking is explicitly reserved for the v1.15 persistent storage milestone. Phase 79 only records the dependency model expectation.
