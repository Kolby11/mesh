# Phase 83 Context: Lifecycle Persistence

**Milestone:** v1.15 Persistent Storage System
**Date:** 2026-05-26
**Status:** Complete

## Goal

Connect `self.storage` to lifecycle persistence so values load before user code,
flush at teardown/shutdown points, and survive runtime recreation.

## Relevant Existing Context

- Phase 81 added scoped JSON document persistence.
- Phase 82 exposed table-like `self.storage` bindings to frontend and backend
  lifecycle contexts.
- Phase 83 should not add frontend render dependency invalidation. That remains
  Phase 84.

## Implementation Notes

- `ScopedStorage` now tracks dirty state for set/remove/clear operations.
- `flush_if_dirty()` persists only changed documents and clears dirty state on
  success.
- Frontend `unmount` flushes storage after user code runs.
- Backend `stop` flushes storage after user code runs, and also flushes if no
  stop hook exists.
- Frontend and backend contexts expose explicit `flush_storage()` methods for
  orderly shell shutdown paths.
- Persistence failures preserve in-memory values and emit diagnostics.
- Storage load happens when contexts are created, before lifecycle `self`
  tables are passed to user code.
