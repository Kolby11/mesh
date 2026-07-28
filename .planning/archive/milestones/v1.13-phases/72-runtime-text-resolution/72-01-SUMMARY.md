---
phase: 72-runtime-text-resolution
plan: 01
subsystem: shell-runtime
tags: [i18n, runtime, keybinds, debug]

requires:
  - phase: 71-contribution-propagation
    provides: LocalizedText preservation for contribution metadata
provides:
  - Runtime keybind descriptors resolve localized manifest text
  - Debug keybind metadata exposes resolved text and source translation keys
  - Missing manifest translations degrade diagnostics without blocking render

requirements-completed: [MRES-01, MRES-02, MRES-03, MRES-04]

duration: 15min
completed: 2026-05-24
---

# Phase 72: Runtime Text Resolution Summary

**Shell runtime metadata now resolves localized manifest keybind text against the active locale and keeps source keys for debugging.**

## Performance

- **Duration:** 15 min
- **Started:** 2026-05-24T07:27:00Z
- **Completed:** 2026-05-24T07:42:55Z
- **Tasks:** 3
- **Files modified:** 6

## Accomplishments

- Replaced raw keybind `LocalizedText` exposure in `this.keybinds` with resolved strings.
- Added additive `label_key`, `label_fallback`, `description_key`, `description_fallback`, `category_key`, and `category_fallback` metadata for translation-backed manifest text.
- Added non-fatal degraded diagnostics for missing localized manifest text, including module id, field path, translation key, and fallback.
- Extended debug keybind metadata with resolved label, description, category, and source key fields.
- Added regression tests for descriptor resolution, missing translation fallback diagnostics, and debug keybind metadata.

## Task Commits

1. **Tasks 1-2: Runtime and debug text resolution** - `ac391d9` (feat)

## Files Created/Modified

- `crates/core/shell/src/shell/component/runtime.rs` - Resolves manifest text while building runtime `this` metadata.
- `crates/core/shell/src/shell/component/input/keyboard.rs` - Resolves manifest text for debug keybind entries.
- `crates/core/foundation/debug/src/lib.rs` - Adds resolved text and source-key fields to `DebugKeybindEntry`.
- `crates/core/shell/src/shell/runtime/debug.rs` - Serializes the expanded debug keybind metadata.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` - Adds runtime and debug i18n regression coverage.
- `crates/core/shell/src/shell/tests.rs` - Updates debug snapshot JSON expectations.

## Decisions Made

Runtime descriptors expose resolved user-facing strings as the primary `label`, `description`, and `category` values. Translation source details are additive metadata fields so existing consumers that read labels do not need to understand manifest localization objects.

Missing translations are diagnostics, not fatal errors. The runtime falls back to the manifest's required fallback text so surfaces remain renderable while still reporting the missing key with enough context to fix the catalog.

## Deviations from Plan

None.

## Issues Encountered

The descriptor resolution test initially observed fallback text because the test helper mounts and creates the root runtime before the test changes locale. The test now reloads the root runtime after setting the locale, matching the production `locale_changed` path.

## User Setup Required

None.

## Next Phase Readiness

Phase 73 can migrate bundled manifests to explicit localized text objects and publish the author-facing contract. The shell runtime now has descriptor, debug, fallback, and diagnostic behavior in place.

---
*Phase: 72-runtime-text-resolution*
*Completed: 2026-05-24*
