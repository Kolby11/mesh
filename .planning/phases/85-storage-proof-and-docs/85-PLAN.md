# Phase 85 Plan: Storage Proof And Docs

**Status:** Complete
**Created:** 2026-05-26
**Completed:** 2026-05-26

## Objective

Satisfy STOREPROOF-01 through STOREPROOF-06.

## Tasks

1. Confirm regression coverage for scoping, atomic persistence, corrupt
   recovery, invalid diagnostics, and instance isolation.
2. Confirm frontend runtime coverage for reads, writes, removes, snapshots,
   lifecycle persistence, and render dependency invalidation.
3. Confirm backend runtime coverage for provider storage, lifecycle flush, and
   invalid diagnostics.
4. Add shipped UI proof by persisting navigation language selection.
5. Update author documentation for the storage contract.
6. Verify diagnostics are visible through existing diagnostics/health lanes
   without leaking stored values.

## Verification

- `nix develop -c cargo fmt --check`
- `nix develop -c cargo test -p mesh-core-scripting storage --no-fail-fast`
- `nix develop -c cargo test -p mesh-core-shell navigation_language_button_publishes_locale_request_on_real_surface --no-fail-fast`
