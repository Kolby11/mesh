# Phase 82 Context: Luau Self Storage Binding

**Milestone:** v1.15 Persistent Storage System
**Date:** 2026-05-26
**Status:** Complete

## Goal

Expose the Phase 81 storage foundation through lifecycle `self.storage` tables
for frontend and backend Luau scripts.

## Relevant Existing Context

- Phase 81 added `StorageScope`, `StorageManager`, `ScopedStorage`, scoped
  paths, document operations, atomic persistence, and corrupt-file diagnostics.
- v1.14 added lifecycle `self` tables for frontend `init/render` and backend
  `start/stop`.
- Phase 82 is API-level binding only. Lifecycle flush behavior and render
  dependency invalidation remain later phases.

## Implementation Notes

- Added a shared `create_lua_storage_table` helper in the storage foundation.
- Reads use `self.storage.key` or `self.storage["key"]`.
- Writes use normal assignment for JSON-like values.
- `nil` assignment removes a key.
- `self.storage:snapshot()` returns a JSON-like table snapshot.
- Unsupported values are rejected non-fatally.
- Frontend invalid storage writes go through `ScriptDiagnostic` with
  `interface = "self.storage"`.
- Backend invalid storage writes are collected through backend storage
  diagnostics for runtime visibility.
