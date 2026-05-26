# Phase 81 Context: Storage Foundation

**Milestone:** v1.15 Persistent Storage System
**Date:** 2026-05-26
**Status:** Complete

## Goal

Add the shell-owned scoped storage subsystem that future Luau `self.storage`
bindings can use for component/provider instance persistence.

## Relevant Existing Context

- v1.14 added `self.meta` identity for frontend and backend runtime contexts.
- Frontend `self.meta` currently exposes `module_id`, `component_id`, `kind`,
  `instance_id`, and `diagnostics_id`.
- Backend `self.meta` currently exposes `module_id`, `provider_id`, `kind`,
  `instance_id`, and `diagnostics_id`.
- Phase 81 must not expose `self.storage` yet; that is Phase 82.
- Persistence must remain shell-owned and JSON-like.

## Implementation Notes

- Added the storage foundation to `mesh-core-scripting` so both frontend and
  backend runtime bindings can share the same Rust implementation.
- `StorageScope` models frontend component and backend provider identities.
- `StorageManager` owns root-relative deterministic paths.
- `ScopedStorage` owns the JSON object document and supports load, get, set,
  remove, clear, snapshot, and persist.
- Scope path segments include a readable sanitized prefix plus hex-encoded raw
  bytes, avoiding unsafe path traversal and preserving deterministic identity.
- Corrupt, unreadable, and non-object files recover to empty documents with
  diagnostics instead of crashing runtime setup.

## Out Of Scope

- No Luau `self.storage` table binding.
- No lifecycle load/flush wiring.
- No render dependency tracking.
- No user-facing storage docs or shipped product proof.
