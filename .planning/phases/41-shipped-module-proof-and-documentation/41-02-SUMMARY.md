---
phase: 41-shipped-module-proof-and-documentation
plan: 02
subsystem: shell
tags: [installed-module-graph, shell-runtime, interfaces, navigation, nix]

requires:
  - phase: 41-shipped-module-proof-and-documentation
    provides: 41-01 shipped graph proof modules and package-level graph tests
provides:
  - Shell runtime proof that installed graph providers register generically
  - Shell frontend filtering proof for shipped navigation and disabled modules
  - Focused navigation behavior verification for interface/keybind-driven audio UI
affects: [shell-runtime, module-system, navigation-bar, backend-providers]

tech-stack:
  added: []
  patterns: [graph-derived interface registration, shipped-module shell tests]

key-files:
  created:
    - .planning/phases/41-shipped-module-proof-and-documentation/41-02-SUMMARY.md
  modified:
    - crates/core/shell/src/shell/tests.rs
    - .planning/phases/41-shipped-module-proof-and-documentation/41-02-PLAN.md
    - .planning/phases/41-shipped-module-proof-and-documentation/41-VALIDATION.md
    - .planning/phases/41-shipped-module-proof-and-documentation/41-RESEARCH.md

key-decisions:
  - "Kept production shell discovery generic; proof lives in shell tests over the real installed graph."
  - "Recorded existing navigation coverage with an empty commit because no Phase 41 code change was required."

patterns-established:
  - "Shell runtime proofs should load the shipped installed module graph instead of constructing toy fixtures."
  - "When Cargo package tests need narrowing, use one test filter after package selection."

requirements-completed: [PROOF-01]

duration: 36min
completed: 2026-05-18
---

# Phase 41-02: Shell Runtime Proof Summary

**Shell tests now prove the shipped installed graph drives provider registration, active backend selection, frontend filtering, and navigation behavior without service-specific production branches.**

## Performance

- **Duration:** 36 min
- **Started:** 2026-05-18T13:06:00+02:00
- **Completed:** 2026-05-18T13:42:15+02:00
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- Expanded `installed_module_graph_exposes_shell_package_choices` to prove the real graph registers `mesh.audio` providers through `Shell::register_interfaces_from_graph`.
- Verified active provider selection preserves `@mesh/pipewire-audio` while retaining `@mesh/pulseaudio-audio` as an alternate provider record.
- Proved frontend filtering keeps `@mesh/navigation-bar` and excludes disabled `@mesh/text-selection-proof`.
- Verified existing navigation tests cover the shipped volume button keybind path and interface-driven audio import without adding backend-specific frontend imports.

## Task Commits

1. **Task 1: Strengthen graph-driven shell provider and frontend tests** - `21bf236` (test)
2. **Task 2: Preserve shipped navigation interface/keybind behavior in focused tests** - `e2b5fb6` (test, empty verification commit)

## Files Created/Modified

- `.planning/phases/41-shipped-module-proof-and-documentation/41-02-SUMMARY.md` - Plan execution summary.
- `crates/core/shell/src/shell/tests.rs` - Shipped installed graph shell runtime assertions.
- `.planning/phases/41-shipped-module-proof-and-documentation/41-02-PLAN.md` - Corrected focused Cargo command.
- `.planning/phases/41-shipped-module-proof-and-documentation/41-VALIDATION.md` - Corrected focused Cargo command.
- `.planning/phases/41-shipped-module-proof-and-documentation/41-RESEARCH.md` - Corrected focused Cargo command.

## Decisions Made

No production code was changed in `crates/core/shell/src/shell/discovery.rs`; the existing generic graph registration path already supported the proof.

## Deviations from Plan

### Auto-fixed Issues

**1. Invalid focused Cargo command**
- **Found during:** Task 1 verification.
- **Issue:** The plan used two test filters for Cargo shell tests.
- **Fix:** Updated the focused command to `nix develop -c cargo test -p mesh-core-shell installed_module_graph`.
- **Files modified:** `.planning/phases/41-shipped-module-proof-and-documentation/41-02-PLAN.md`, `.planning/phases/41-shipped-module-proof-and-documentation/41-VALIDATION.md`, `.planning/phases/41-shipped-module-proof-and-documentation/41-RESEARCH.md`
- **Verification:** `nix develop -c cargo test -p mesh-core-shell installed_module_graph`
- **Committed in:** `21bf236`

**2. Navigation task needed no code changes**
- **Found during:** Task 2 inspection.
- **Issue:** Existing focused tests and component source already satisfied the task acceptance criteria.
- **Fix:** Left production and test code unchanged, verified behavior, and recorded an empty commit for traceability.
- **Files modified:** None.
- **Verification:** `nix develop -c cargo test -p mesh-core-shell shell::component::tests::interaction::navigation`
- **Committed in:** `e2b5fb6`

---

**Total deviations:** 2 auto-fixed/documented.
**Impact on plan:** No scope expansion; corrections made the intended proof executable and preserved unrelated dirty work.

## Issues Encountered

The grep review matched provider strings in tests and existing declarations. Production discovery behavior remained generic; no new service-specific Rust branch was added.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

41-03 can document the shipped author workflow against real graph behavior, shell runtime proof, provider selection, interface imports, settings schema, and diagnostics.

---
*Phase: 41-shipped-module-proof-and-documentation*
*Completed: 2026-05-18*
