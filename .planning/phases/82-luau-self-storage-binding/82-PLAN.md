# Phase 82 Plan: Luau Self Storage Binding

**Status:** Complete
**Created:** 2026-05-26
**Completed:** 2026-05-26

## Objective

Deliver the Luau `self.storage` API required by STOREAPI-01 through
STOREAPI-06.

## Tasks

1. Add a reusable Lua table proxy for scoped storage.
2. Attach the proxy to frontend lifecycle `self` tables.
3. Attach the proxy to backend lifecycle `self` tables.
4. Convert supported Lua values to JSON values.
5. Treat nil assignment as key removal.
6. Emit diagnostics instead of throwing when unsupported values are assigned.
7. Add focused frontend and backend runtime tests.

## Verification

- `nix develop -c cargo fmt --check`
- `nix develop -c cargo test -p mesh-core-scripting storage --no-fail-fast`
- `nix develop -c cargo test -p mesh-core-scripting self_storage --no-fail-fast`
