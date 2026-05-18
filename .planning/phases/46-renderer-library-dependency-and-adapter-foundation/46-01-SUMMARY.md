---
phase: 46-renderer-library-dependency-and-adapter-foundation
plan: 01
subsystem: renderer
tags: [rust, cargo, renderer-libraries, taffy, parley, accesskit, anyrender, vello-encoding]

requires:
  - phase: 46-renderer-library-dependency-and-adapter-foundation
    provides: Phase 46 research and Rust 1.85-compatible renderer-library version selection
provides:
  - Workspace dependency pins for selected renderer-library candidates
  - Disabled-by-default mesh-core-render Cargo feature gates
  - Verified default and enabled renderer-library build paths
affects: [renderer, cargo, dependency-gates, v1.9]

tech-stack:
  added: [taffy 0.10.1, parley 0.7.0, accesskit 0.24.0, anyrender 0.10.0, vello_encoding 0.5.1]
  patterns: [workspace dependency source, optional render crate feature gates, empty default feature set]

key-files:
  created: [.planning/phases/46-renderer-library-dependency-and-adapter-foundation/46-01-SUMMARY.md]
  modified: [Cargo.toml, Cargo.lock, crates/core/frontend/render/Cargo.toml]

key-decisions:
  - "Renderer-library dependencies remain disabled by default through explicit mesh-core-render features."
  - "Full Vello, Blitz, Winit, Stylo, DOM/web-platform, and additional Skia expansion remain out of Phase 46 Plan 01."
  - "The aggregate renderer-libraries feature is a verification path, not a runtime behavior switch."

patterns-established:
  - "Optional renderer candidates are declared in workspace dependencies and consumed through dep: feature gates in mesh-core-render."
  - "Default renderer behavior is protected by an explicit default = [] feature table."

requirements-completed: [LIBS-01]

duration: 3min
completed: 2026-05-18
---

# Phase 46 Plan 01: Optional Renderer Library Cargo Features Summary

**Rust 1.85-compatible renderer-library candidates are now production manifest entries behind disabled-by-default mesh-core-render features.**

## Performance

- **Duration:** 3 min
- **Started:** 2026-05-18T16:02:43Z
- **Completed:** 2026-05-18T16:05:25Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- Added workspace dependency pins for `taffy 0.10.1`, `parley 0.7.0`, `accesskit 0.24.0`, `anyrender 0.10.0`, and `vello_encoding 0.5.1`.
- Added disabled-by-default `mesh-core-render` features: `renderer-taffy`, `renderer-parley`, `renderer-accesskit`, `renderer-anyrender`, `renderer-vello-encoding`, and aggregate `renderer-libraries`.
- Verified both default and explicit aggregate feature builds; no renderer behavior or author-facing `.mesh` contract changed.

## Task Commits

Each task was committed atomically:

1. **Task 1: Add workspace renderer-library dependency versions** - `54f041b` (feat)
2. **Task 2: Add disabled-by-default mesh-core-render features** - `8af37a8` (feat)
3. **Task 3: Verify enabled dependency feature path** - `e29bb8b` (chore, empty verification commit)

**Plan metadata:** final docs commit listed in the executor completion output

## Files Created/Modified

- `Cargo.toml` - Added selected renderer-library versions to `[workspace.dependencies]`.
- `Cargo.lock` - Locked the optional renderer-library transitive dependency graph after Cargo verification.
- `crates/core/frontend/render/Cargo.toml` - Added explicit feature gates and optional dependency entries.
- `.planning/phases/46-renderer-library-dependency-and-adapter-foundation/46-01-SUMMARY.md` - Recorded plan outcome and verification.

## Verification

- PASS: `rg -n "taffy = \\{ version = \"0\\.10\\.1\"|parley = \\{ version = \"0\\.7\\.0\"|accesskit = \\{ version = \"0\\.24\\.0\"|anyrender = \\{ version = \"0\\.10\\.0\"|vello_encoding = \\{ version = \"0\\.5\\.1\"" Cargo.toml`
- PASS: `rg -n "^vello =" Cargo.toml` returned no matches.
- PASS: `rg -n "default = \\[\\]|renderer-taffy = \\[\"dep:taffy\"\\]|renderer-parley = \\[\"dep:parley\"\\]|renderer-accesskit = \\[\"dep:accesskit\"\\]|renderer-anyrender = \\[\"dep:anyrender\"\\]|renderer-vello-encoding = \\[\"dep:vello_encoding\"\\]|renderer-libraries = \\[\"renderer-taffy\", \"renderer-parley\", \"renderer-accesskit\", \"renderer-anyrender\", \"renderer-vello-encoding\"\\]" crates/core/frontend/render/Cargo.toml`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo check -p mesh-core-render`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo check -p mesh-core-render --features renderer-libraries`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo tree -p mesh-core-render --features renderer-libraries`
- PASS: `rg -n "renderer-taffy|renderer-parley|renderer-accesskit|renderer-anyrender|renderer-vello-encoding|renderer-libraries" crates/core/frontend/render/Cargo.toml`

The Cargo checks completed with the existing `mesh-core-render` warning that `CachedGlyph::placement_top` is never read. No new compiler errors were introduced.

## Dependency Tree Summary

The enabled `renderer-libraries` feature adds the intended direct optional dependencies:

- `accesskit v0.24.0` with `uuid`
- `anyrender v0.10.0` with `kurbo v0.13.1`, `peniko v0.6.1`, and `raw-window-handle`
- `parley v0.7.0` with `fontique`, `harfrust v0.3.2`, `hashbrown`, `skrifa v0.37.0`, and existing text stack overlap through `swash`
- `taffy v0.10.1` with `arrayvec`, `grid`, and `slotmap`
- `vello_encoding v0.5.1` with `bytemuck`, `guillotiere`, `peniko v0.4.1`, and `skrifa v0.35.0`

Full `vello` and `wgpu` were not added.

## Decisions Made

Followed the plan-specified dependency versions and feature names. The only implementation detail was committing the `Cargo.lock` update with Task 2 because Cargo materialized the optional dependency graph during the default check.

## Deviations from Plan

None - plan executed exactly as written.

**Total deviations:** 0 auto-fixed.
**Impact on plan:** No scope change.

## Issues Encountered

- The first `cargo tree` attempt could not access the Nix daemon from the sandbox. The command was rerun with approved escalation and passed.
- Parallel/worktree execution was detected, and `.planning/STATE.md` already had an unrelated modification. Per the user instruction, shared tracking files were not updated.

## Known Stubs

None. The `default = []` feature entry is intentional Cargo behavior, not a UI/data stub.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Plan 02 can build the adapter status/bypass seam against the feature gates added here. The current renderer remains default, and the enabled dependency path compiles for future adapter work.

## Self-Check: PASSED

- Found expected files: `Cargo.toml`, `Cargo.lock`, `crates/core/frontend/render/Cargo.toml`, and this summary.
- Found task commits in git history: `54f041b`, `8af37a8`, and `e29bb8b`.
- Shared orchestrator artifacts were intentionally not updated because parallel/worktree execution was detected.

---
*Phase: 46-renderer-library-dependency-and-adapter-foundation*
*Completed: 2026-05-18*
