---
phase: 39-contribution-and-interface-extension-index
plan: 02
subsystem: module-graph
tags: [providers, capabilities, backend-lifecycle, diagnostics]
requires:
  - phase: 39-01
    provides: separated provider/interface/frontend requirement graph boundary
provides:
  - provider metadata including version, base module, provider id, and explicit capabilities
  - backend lifecycle missing-capability diagnostics
  - generic non-audio provider routing proof
affects: [phase-39, backend-lifecycle, interface-registry]
tech-stack:
  added: []
  patterns: [explicit capability comparison, data-driven provider routing]
key-files:
  created:
    - .planning/phases/39-contribution-and-interface-extension-index/39-02-SUMMARY.md
  modified:
    - crates/core/extension/module/src/package/installed_graph.rs
    - crates/core/extension/module/src/package/tests.rs
    - crates/core/shell/src/shell/backend/mod.rs
    - crates/core/shell/src/shell/backend/candidates.rs
    - crates/core/shell/src/shell/tests.rs
key-decisions:
  - "BackendProviderNode capability metadata is copied only from backend module manifest capabilities."
  - "Interface contract required capabilities are checked against selected provider declarations."
  - "Missing capability is its own backend lifecycle status."
patterns-established:
  - "Provider compatibility diagnostics compare explicit contract and manifest strings without service-specific branches."
requirements-completed: [EXT-02, EXT-04]
duration: 31min
completed: 2026-05-17
---

# Phase 39: Provider Dependency Capability Split Summary

**Provider metadata and backend diagnostics now keep interface requirements, provider identity, and host capabilities separate**

## Performance

- **Duration:** 31 min
- **Started:** 2026-05-17T21:40:00Z
- **Completed:** 2026-05-17T22:11:00Z
- **Tasks:** 3
- **Files modified:** 5

## Accomplishments

- Extended backend provider graph records with declared interface/provider metadata and explicit required/optional capabilities.
- Added `missing_capability` backend lifecycle status for contract-required capabilities absent from a selected provider.
- Added non-audio package and shell tests proving provider routing remains manifest-driven.

## Task Commits

1. **Task 1: Extend provider records without merging concepts** - `6bfcd64`
2. **Task 2: Add distinct provider/dependency/capability diagnostics** - `a0afc64`
3. **Task 3: Prove service-specific Rust branches are unnecessary** - `715cc8d`

## Files Created/Modified

- `crates/core/extension/module/src/package/installed_graph.rs` - Added provider metadata fields.
- `crates/core/extension/module/src/package/tests.rs` - Added provider capability and generic provider-routing tests.
- `crates/core/shell/src/shell/backend/mod.rs` - Added backend lifecycle `missing_capability` status.
- `crates/core/shell/src/shell/backend/candidates.rs` - Added contract capability comparison against selected provider metadata.
- `crates/core/shell/src/shell/tests.rs` - Added shell diagnostics tests for missing and satisfied provider capabilities.

## Decisions Made

Capability compatibility is checked only when an interface contract explicitly declares required capabilities. Provider identity and frontend dependency requirements still do not grant or imply host permissions.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Added `BackendRuntimeStatus::MissingCapability`**
- **Found during:** Task 2
- **Issue:** A separate diagnostic status was required, but the status enum did not include one.
- **Fix:** Added `MissingCapability` and updated status string mapping/tests.
- **Files modified:** `crates/core/shell/src/shell/backend/mod.rs`, `crates/core/shell/src/shell/tests.rs`
- **Verification:** `cargo test -p mesh-core-shell backend_lifecycle` and `cargo test -p mesh-core-shell backend` passed.
- **Committed in:** `a0afc64`

---

**Total deviations:** 1 auto-fixed missing status.
**Impact on plan:** Required to satisfy the distinct diagnostics requirement.

## Issues Encountered

The first shell compile missed an import for `BackendProviderNode`; it was fixed before committing task 2.

## User Setup Required

None - no external service configuration required.

## Verification

- `cargo test -p mesh-core-module provider_capability` passed.
- `cargo test -p mesh-core-shell backend` passed.
- `rg -n "mesh\\.audio|audio" crates/core/shell/src/shell/backend/candidates.rs` returned no matches.

## Self-Check: PASSED

The plan success criteria are met for EXT-02 and EXT-04.

## Next Phase Readiness

Wave 3 can use the enriched provider metadata when converting contribution records into source-rich typed registries.

---
*Phase: 39-contribution-and-interface-extension-index*
*Completed: 2026-05-17*
