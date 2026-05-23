---
phase: 65
name: Module Instance Registry
status: ready
---

# Phase 65 Plan: Module Instance Registry

## Goal

Register backend services and frontend modules as stable runtime object instances with inspectable metadata.

## Tasks

1. Add debug data structures for module object instances.
2. Populate module object entries from discovered modules, mounted frontend components, and registered backend providers.
3. Expose the registry through `mesh.debug.module_instances` and the debug interface contract.
4. Verify with formatting, narrow crate checks, and note shell-check environment blockers.

## Acceptance

- `MOBJ-01`: Backend service providers appear as backend object instances with interface/version/lifecycle/active metadata.
- `MOBJ-02`: Frontend modules appear as frontend object instances with stable instance ids and lifecycle/capability metadata.
- `MOBJ-03`: Debug state exposes the registry without service-specific Rust branches.
