---
phase: 02-backend-lifecycle-foundation
plan: 01
subsystem: backend-lifecycle
tags: [rust, package-graph, backend-runtime, provider-selection]
requires:
  - phase: 01-plugin-package-manifest-foundation
    provides: installed module graph and active provider choices
provides:
  - graph-driven backend launch candidate resolution
  - pre-launch provider manifest and entrypoint validation
  - explicit unmet backend requirement statuses
affects: [backend lifecycle, provider selection, shell startup]
tech-stack:
  added: []
  patterns: [installed module graph as backend startup source of truth]
key-files:
  created: []
  modified:
    - crates/core/shell/src/shell/mod.rs
    - crates/core/shell/src/shell/component.rs
key-decisions:
  - "Backend startup now prefers explicit installed-module graph active providers over legacy priority fallback."
  - "Legacy priority discovery remains only as a compatibility fallback when the installed package graph cannot load."
patterns-established:
  - "Graph resolver: derive backend launch candidates from InstalledModuleGraph and validate before creating command channels."
requirements-completed: [BPLUG-01, BPLUG-02]
duration: 12 min
completed: 2026-05-03
---

# Phase 02 Plan 01: Graph-Driven Backend Launch Summary

**Installed-module graph provider choices now drive backend launch candidates before runtime channels are created**

## Performance

- **Duration:** 12 min
- **Started:** 2026-05-03T17:32:40Z
- **Completed:** 2026-05-03T17:44:05Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments

- Added graph-derived backend launch candidate resolution using `InstalledModuleGraph::active_provider`.
- Added pre-launch status handling for `invalid_manifest`, `missing_entrypoint`, `missing_binary`, `no_active_provider`, and `unmet_backend_requirement`.
- Removed the obsolete priority-only `BackendServiceCandidate` type from shell component code.
- Added focused backend lifecycle tests for active provider selection, disabled providers, missing entrypoints, and unmet frontend backend requirements.

## Task Commits

1. **Tasks 1-3: Graph resolver, validation, and unmet requirement statuses** - `54096f4` (feat)

**Plan metadata:** this SUMMARY commit

## Files Created/Modified

- `crates/core/shell/src/shell/mod.rs` - Adds graph-driven backend candidate resolution and startup bridging to backend lifecycle events.
- `crates/core/shell/src/shell/component.rs` - Removes the obsolete backend priority candidate struct.

## Decisions Made

- Kept legacy priority-based backend discovery only as a compatibility fallback when `config/package.json` cannot be loaded.
- Used legacy plugin manifests for runtime script entrypoint fallback because current repo-local module `package.json` fixtures declare providers but not script entrypoints.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Plan 03 can build runtime slot ownership on top of graph-derived candidates and lifecycle event routing.

## Self-Check: PASSED

- `nix develop -c cargo test -p mesh-core-shell backend_lifecycle` passed.
- `nix develop -c cargo test -p mesh-core-plugin installed_module_graph` passed.
- `grep -n "fallback_provider(" crates/core/shell/src/shell/mod.rs` found no graph-driven fallback provider use.

---
*Phase: 02-backend-lifecycle-foundation*
*Completed: 2026-05-03*
