# Phase 84 Plan: Storage Rerender Integration

**Status:** Complete
**Created:** 2026-05-26
**Completed:** 2026-05-26

## Objective

Deliver storage render dependency tracking for STORERENDER-01 through
STORERENDER-04.

## Tasks

1. Track storage reads while frontend render lifecycle code runs.
2. Track storage writes by key.
3. Dirty script state only when a changed key was read by the latest render.
4. Leave unwatched key writes as in-memory/persistence changes without rebuild
   invalidation.
5. Keep explicit redraw behavior independent.
6. Add focused runtime tests for watched and unwatched key behavior.

## Verification

- `nix develop -c cargo fmt --check`
- `nix develop -c cargo test -p mesh-core-scripting storage_render_reads --no-fail-fast`
- `nix develop -c cargo test -p mesh-core-scripting storage --no-fail-fast`
