---
phase: 70-localized-text-manifest-model
plan: 01
subsystem: manifest
tags: [i18n, module-json, keybinds, diagnostics]

requires: []
provides:
  - Reusable LocalizedText manifest model
  - Keybind display field localized text parsing
  - Canonical module.json diagnostics for raw dotted key-like literals
affects: [contribution-propagation, runtime-text-resolution, shipped-manifest-i18n-proof]

tech-stack:
  added: []
  patterns: [serde untagged compatibility enum, non-fatal manifest diagnostics]

key-files:
  created: []
  modified:
    - crates/core/extension/module/src/manifest/model.rs
    - crates/core/extension/module/src/manifest/tests.rs
    - crates/core/extension/module/src/package/installed_graph.rs
    - crates/core/extension/module/src/package/module_manifest.rs
    - crates/core/extension/module/src/package/tests.rs
    - crates/core/extension/module/src/lib.rs

key-decisions:
  - "Raw strings remain literal text for compatibility."
  - "Localized manifest text uses the structured { \"t\": \"...\", \"fallback\": \"...\" } object."
  - "Phase 70 keeps installed graph compatibility consumers on fallback strings."

patterns-established:
  - "LocalizedText::validate receives exact manifest field paths for actionable validation errors."
  - "Suspicious raw i18n-key-shaped literals produce non-fatal ModuleManifestDiagnostic warnings."

requirements-completed: [MI18N-01, MI18N-02, MI18N-03, MI18N-04]

duration: 15min
completed: 2026-05-24
---

# Phase 70: Localized Text Manifest Model Summary

**Reusable localized text manifest values for keybind display metadata, with fallback compatibility and canonical loader migration diagnostics**

## Performance

- **Duration:** 15 min
- **Started:** 2026-05-24T06:21:00Z
- **Completed:** 2026-05-24T06:36:10Z
- **Tasks:** 3
- **Files modified:** 7

## Accomplishments

- Added public `LocalizedText` with literal string and `{ "t": "...", "fallback": "..." }` translation forms.
- Moved keybind label, description, and category fields to `Option<LocalizedText>` while preserving fallback string output for existing contribution consumers.
- Added canonical `module.json` warning diagnostics for raw dotted key-like keybind text fields.

## Task Commits

1. **Tasks 1-3: Localized text schema, keybind field adoption, and diagnostics** - `eceb8a3` (feat)

## Files Created/Modified

- `crates/core/extension/module/src/manifest/model.rs` - Added `LocalizedText`, validation helpers, suspicious-key heuristic, and keybind field type changes.
- `crates/core/extension/module/src/manifest/tests.rs` - Covered literal parsing, translation object parsing, validation failures, and keybind display parsing.
- `crates/core/extension/module/src/package/module_manifest.rs` - Added localized text diagnostics collection for canonical module manifests.
- `crates/core/extension/module/src/package/installed_graph.rs` - Added fallback conversion for string contribution records and loader diagnostics wiring.
- `crates/core/extension/module/src/package/tests.rs` - Covered raw dotted warning diagnostics and literal no-warning behavior.
- `crates/core/extension/module/src/lib.rs` - Re-exported `LocalizedText`.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` - Updated one direct `KeybindAction` fixture to construct `LocalizedText::Literal`.

## Decisions Made

Raw manifest strings are literal text, including dotted strings, because compatibility is more important than implicit localization. Dotted raw strings are still diagnosed in canonical manifests so authors get a concrete migration path without breaking existing modules.

Phase 70 intentionally flattens localized keybind metadata to fallback strings at the existing `ContributedKeybindAction` boundary. Rich installed-graph propagation remains Phase 71 scope.

## Deviations from Plan

One shell test fixture needed a direct `LocalizedText::Literal(...)` update because it constructs `mesh_core_module::KeybindAction` manually. The change was compatibility-only and did not expand runtime localization scope.

## Issues Encountered

`cargo check -p mesh-core-shell` could not complete in this environment because `smithay-client-toolkit` requires the system `xkbcommon.pc` pkg-config file, which is not installed. Module crate verification passed.

## User Setup Required

None.

## Next Phase Readiness

Phase 71 can now preserve `LocalizedText` metadata in installed-graph contribution records instead of relying on fallback strings.

---
*Phase: 70-localized-text-manifest-model*
*Completed: 2026-05-24*
