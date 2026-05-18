---
phase: 41-shipped-module-proof-and-documentation
plan: 01
subsystem: module-graph
tags: [module-json, installed-graph, interface, icon-pack, diagnostics]

requires:
  - phase: 40-migration-diagnostics-and-author-docs
    provides: canonical manifest migration diagnostics and keybind contribution preservation
provides:
  - Canonical `@mesh/audio-interface` module installed in the root graph
  - Canonical `@mesh/icons-default` icon-pack module installed in the root graph
  - Shipped graph proof for navigation, audio providers, settings, keybinds, icons, and diagnostics
affects: [phase-41, module-system, shell-runtime, author-docs]

tech-stack:
  added: []
  patterns: [real-manifest graph proof, focused diagnostic fixture]

key-files:
  created:
    - modules/interfaces/audio/module.json
    - modules/interfaces/audio/interface.toml
  modified:
    - config/module.json
    - modules/icon-packs/default/module.json
    - crates/core/extension/module/src/package/tests.rs
    - .planning/phases/41-shipped-module-proof-and-documentation/41-01-PLAN.md
    - .planning/phases/41-shipped-module-proof-and-documentation/41-VALIDATION.md
    - .planning/phases/41-shipped-module-proof-and-documentation/41-RESEARCH.md

key-decisions:
  - "The proof path installs real canonical interface and icon-pack modules instead of accepting their absence as diagnostics."
  - "The missing-icon diagnostic proof mutates a loaded real navigation manifest inside the test, not checked-in shipped manifests."

patterns-established:
  - "Shipped module proof loads `config/module.json` and asserts typed graph records from real manifests."
  - "Diagnostic edge cases can use real loaded manifests with test-only mutation when checked-in invalid manifests would be brittle."

requirements-completed: [PROOF-01]

duration: 18min
completed: 2026-05-18
---

# Phase 41: Real Shipped Graph Proof Summary

**Canonical audio interface and default icon modules now participate in the real installed graph proof for the shipped navigation/audio path.**

## Performance

- **Duration:** 18 min
- **Started:** 2026-05-18T11:34:00Z
- **Completed:** 2026-05-18T11:52:00Z
- **Tasks:** 3
- **Files modified:** 7

## Accomplishments

- Added canonical `@mesh/audio-interface` module metadata and graph-local `interface.toml`.
- Converted `@mesh/icons-default` from legacy icon-pack manifest shape to canonical `module.json`.
- Expanded package tests to prove the real shipped graph exposes canonical manifest sources, `mesh.audio` interface declaration, active PipeWire provider, alternate PulseAudio provider, navigation layout/settings/keybind/icon contributions, and default icon-pack contribution.
- Added focused diagnostics coverage for a missing navigation semantic icon using the real navigation and icon-pack manifests with test-only mutation.

## Task Commits

1. **Task 1: Install canonical proof-path interface and icon modules** - `97a8272` (feat)
2. **Task 2: Broaden the real shipped graph package proof** - `33ef8ea` (test)
3. **Task 3: Add proof-path diagnostics coverage** - `0017503` (test)

## Files Created/Modified

- `config/module.json` - Installs `@mesh/audio-interface` and `@mesh/icons-default`.
- `modules/interfaces/audio/module.json` - Canonical interface module manifest for `mesh.audio`.
- `modules/interfaces/audio/interface.toml` - Graph-loadable audio interface contract.
- `modules/icon-packs/default/module.json` - Canonical icon-pack module manifest.
- `crates/core/extension/module/src/package/tests.rs` - Real shipped graph and diagnostics proof tests.
- `.planning/phases/41-shipped-module-proof-and-documentation/41-01-PLAN.md` - Corrected focused cargo filter syntax.
- `.planning/phases/41-shipped-module-proof-and-documentation/41-VALIDATION.md` - Corrected focused cargo filter syntax.
- `.planning/phases/41-shipped-module-proof-and-documentation/41-RESEARCH.md` - Corrected focused cargo filter syntax.

## Decisions Made

The checked-in proof path now resolves the interface and icon-pack modules directly rather than treating their absence as acceptable graph diagnostics. This better satisfies Phase 41's end-to-end proof requirement.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Corrected invalid focused cargo test command**
- **Found during:** Task 2 (Broaden the real shipped graph package proof)
- **Issue:** The plan used `cargo test -p mesh-core-module package::tests shipped_module`, but Cargo accepts only one test filter argument.
- **Fix:** Updated Phase 41 plan/research/validation commands to use `cargo test -p mesh-core-module shipped_module` and `cargo test -p mesh-core-module shipped_module_diagnostics`.
- **Files modified:** `.planning/phases/41-shipped-module-proof-and-documentation/41-01-PLAN.md`, `41-VALIDATION.md`, `41-RESEARCH.md`
- **Verification:** `nix develop -c cargo test -p mesh-core-module shipped_module` passed.
- **Committed in:** `33ef8ea`

**2. [Rule 3 - Blocking] Fixed frontend entrypoint assertion field**
- **Found during:** Task 2 (Broaden the real shipped graph package proof)
- **Issue:** The test initially asserted `ContributedFrontendEntrypoint.id`, but the contribution id lives at `source.local_id`.
- **Fix:** Updated the assertion to check `entrypoint.source.local_id == "main"`.
- **Files modified:** `crates/core/extension/module/src/package/tests.rs`
- **Verification:** `nix develop -c cargo test -p mesh-core-module shipped_module` passed.
- **Committed in:** `33ef8ea`

---

**Total deviations:** 2 auto-fixed (blocking execution/test issues).
**Impact on plan:** No scope change; fixes made the planned verification executable and aligned the assertion with the existing typed graph model.

## Issues Encountered

Nix emitted transient cache/file-lock messages during parallel focused test runs, but the tests completed successfully.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Wave 2 can build on the canonical root graph, interface declaration, icon-pack contribution, and shipped graph tests from this plan.

## Self-Check: PASSED

- `nix develop -c cargo test -p mesh-core-module shipped_module` passed.
- `nix develop -c cargo test -p mesh-core-module shipped_module_diagnostics` passed.
- `nix develop -c cargo test -p mesh-core-module package::tests` passed.

---
*Phase: 41-shipped-module-proof-and-documentation*
*Completed: 2026-05-18*
