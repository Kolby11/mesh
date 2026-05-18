---
phase: 40-migration-diagnostics-and-author-docs
plan: 03
subsystem: module-system
tags: [module-manifest, keybinds, migration, shell-keyboard, docs]
requires:
  - phase: 39-contribution-and-interface-extension-index
    provides: Contribution index and installed graph records for module resources
provides:
  - Typed installed-graph keybind contribution records with default and localized triggers
  - Manifest-first shell shortcut resolution coverage with legacy settings fallback preserved
  - Author documentation for keybind migration and keyboard override boundaries
affects: [module-system, shell-keyboard, settings, author-docs]
tech-stack:
  added: []
  patterns: [manifest-first keybind declarations, user override fallback boundary]
key-files:
  created: []
  modified:
    - crates/core/extension/module/src/manifest/model.rs
    - crates/core/extension/module/src/package/installed_graph.rs
    - crates/core/extension/module/src/package/tests.rs
    - crates/core/shell/src/shell/component/tests/interaction/navigation.rs
    - docs/module-system.md
    - docs/settings/README.md
key-decisions:
  - "Keybind trigger data belongs in installed graph contribution records, not only raw manifests."
  - "settings.keyboard.surface_shortcuts remains user override data, with legacy settings-derived declarations used only when a manifest action id is absent."
patterns-established:
  - "Installed graph keybind records preserve default and localized triggers for later dispatch, conflict, and accessibility phases."
  - "Shell shortcut tests prove manifest declarations win over legacy settings declarations with the same action id."
requirements-completed: [MIGR-02]
duration: 1h 51min
completed: 2026-05-18
---

# Phase 40: Keybind Migration Continuity Summary

**Manifest keybind declarations now flow into installed graph records with default and localized trigger data, while shell shortcut tests preserve manifest-first resolution and settings-only user overrides.**

## Performance

- **Duration:** 1h 51min
- **Started:** 2026-05-18T08:52:01Z
- **Completed:** 2026-05-18T10:43:01Z
- **Tasks:** 3
- **Files modified:** 6

## Accomplishments

- Extended `ContributedKeybindAction` to carry `trigger` and `localized_triggers` cloned from canonical manifest keybind actions.
- Added package and shell navigation tests covering trigger preservation, manifest-over-legacy precedence, user overrides, localized triggers, and legacy fallback behavior.
- Documented `mesh.keybinds`, installed graph keybind contributions, and the boundary between manifest declarations and `keyboard.surface_shortcuts` user overrides.

## Task Commits

Each task was committed atomically:

1. **Task 1: Carry trigger data in typed keybind contribution records** - `9b5c618` (test)
2. **Task 2: Protect manifest-first shortcut resolution behavior** - `ac20660` (test)
3. **Task 3: Document keybind migration and override boundaries** - `a172d22` (docs)

**Plan metadata:** `e3709fe` (docs: align verification wording)

## Files Created/Modified

- `crates/core/extension/module/src/manifest/model.rs` - Derived `PartialEq` and `Eq` for `KeybindTrigger` so typed contribution records can compare trigger data in tests.
- `crates/core/extension/module/src/package/installed_graph.rs` - Added default and localized trigger data to `ContributedKeybindAction`.
- `crates/core/extension/module/src/package/tests.rs` - Asserted installed graph keybind records preserve default `m` and localized `sk` trigger `s`.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` - Added shortcut resolution coverage proving manifest-first behavior and legacy fallback preservation.
- `docs/module-system.md` - Added `### Keybind Contributions` author guidance for `mesh.keybinds` and installed graph preservation.
- `docs/settings/README.md` - Clarified `keyboard.surface_shortcuts` as user override data and legacy settings declarations as fallback input only.

## Decisions Made

- Kept production shell shortcut resolution unchanged because existing behavior already used manifest declarations before legacy settings fallback.
- Treated `KeybindTrigger` equality derives as support for typed graph testing, not a behavior change.
- Preserved the existing `surface_shortcuts` JSON example while clarifying its role as override data.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Reflowed documentation wording for deterministic verification**
- **Found during:** Task 3 (Document keybind migration and override boundaries)
- **Issue:** The documentation contained the intended phrase but split and capitalized it such that the plan's exact `rg` verification did not match.
- **Fix:** Reflowed the sentence to include `installed graph keybind contributions` as an exact phrase.
- **Files modified:** `docs/module-system.md`
- **Verification:** `rg -n "Keybind Contributions|mesh.keybinds|localizedTriggers|installed graph keybind contributions|surface_shortcuts|Legacy settings-derived shortcut declarations are fallback input only when a manifest action id is absent" docs/module-system.md docs/settings/README.md`
- **Committed in:** `e3709fe`

---

**Total deviations:** 1 auto-fixed (1 blocking verification issue)
**Impact on plan:** Verification wording became deterministic. No scope creep or behavior change.

## Issues Encountered

- The plan's package test command used two Cargo filters (`package::tests keybind`), which Cargo rejects. Verification was rerun with the focused test name `contribution_index_exposes_frontend_keybind_resource_interface_and_provider_records`.
- The shell navigation test requires native `xkbcommon` pkg-config metadata; running outside the Nix dev shell failed before test execution. The same focused test passed under `nix develop -c`.

## Verification

- `cargo test -p mesh-core-module contribution_index_exposes_frontend_keybind_resource_interface_and_provider_records` - passed.
- `nix develop -c cargo test -p mesh-core-shell shell::component::tests::interaction::navigation` - passed, 22 tests.
- `rg -n "Keybind Contributions|mesh.keybinds|localizedTriggers|installed graph keybind contributions|surface_shortcuts|Legacy settings-derived shortcut declarations are fallback input only when a manifest action id is absent" docs/module-system.md docs/settings/README.md` - passed.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

The canonical manifest keybind model is now represented in installed graph records and protected by shell shortcut tests. Later dispatch, conflict detection, and accessibility phases can consume typed keybind contribution data without re-reading raw manifests, while user settings remain scoped to overrides and legacy fallback input.

## Self-Check: PASSED

- Required key files exist and contain the planned keybind contribution fields and documentation phrases.
- Plan commits exist for `40-03`.
- Verification commands passed after using the valid Cargo filter and Nix shell for native dependencies.

---
*Phase: 40-migration-diagnostics-and-author-docs*
*Completed: 2026-05-18*
