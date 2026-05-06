---
phase: 11-keyboard-navigation-and-shortcuts
plan: 03
subsystem: config-and-routing
tags: [rust, config, keyboard, shortcuts, navigation-bar]

requires:
  - phase: 11-02
    provides: Shell-owned focused keyboard dispatch and activation defaults

provides:
  - Shell keyboard settings schema with remappable defaults
  - Focused-surface shortcut defaults from module settings plus shell override slots
  - Shortcut metadata annotation on the configured host control
  - Navigation-bar mute shortcut proof wired through settings

affects:
  - phase-11-proof-surface
  - phase-13-navigation-bar-proof

tech-stack:
  added: []
  patterns:
    - "Module settings define shortcut actions while shell settings own per-surface key remaps"
    - "Shortcut metadata is derived from the same effective binding used for runtime dispatch"

key-files:
  created:
    - .planning/phases/11-keyboard-navigation-and-shortcuts/11-03-SUMMARY.md
  modified:
    - crates/core/foundation/config/src/lib.rs
    - config/settings-default.json
    - crates/core/shell/src/shell/component/input.rs
    - crates/core/shell/src/shell/mod.rs
    - crates/core/shell/src/shell/component/tests.rs
    - modules/frontend/navigation-bar/module.json
    - modules/frontend/navigation-bar/config/settings.json
    - modules/frontend/navigation-bar/src/main.mesh
    - modules/frontend/navigation-bar/src/components/volume-button.mesh

key-decisions:
  - "Shell settings expose default activation bindings and per-surface shortcut override maps so the runtime no longer depends on hardcoded activation keys."
  - "Focused-surface shortcuts are declared in module settings by action id and merged with shell overrides by surface id."
  - "The navigation bar runs in `keyboard_mode: on_demand` and advertises the effective mute shortcut on the volume control metadata."

requirements-completed: [KEY-03, KEY-04]

duration: 1 session
completed: 2026-05-06
---

# Phase 11 Plan 03: Surface Shortcuts And Keyboard Settings Overrides

**Keyboard defaults are now settings-driven, and the navigation bar ships a focused-surface mute shortcut whose metadata stays synchronized with the effective binding.**

## Accomplishments

- Added a `keyboard` section to `ShellSettings` with default button, toggle, and slider bindings plus per-surface shortcut override slots.
- Wired focused-surface shortcut resolution through module settings and shell settings, keeping shell-global shortcut precedence untouched.
- Annotated the shortcut host node with the effective keyboard shortcut string so accessibility/debug metadata stays aligned with runtime routing.
- Updated the shipped navigation-bar module to expose a concrete `m` mute shortcut and request the audio control capability it now uses.

## Task Commits

Not created in this workspace run. The implementation is validated but currently uncommitted.

## Verification

- `nix develop -c cargo test -p mesh-core-config keyboard_settings`
- `nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts`

## Deviations From Plan

None. The module-default plus shell-override model shipped as planned.

## Self-Check: PASSED

- Summary file exists.
- Targeted validation commands passed.
- Keyboard settings merge and shortcut metadata/routing are covered by automated tests.

---
*Phase: 11-keyboard-navigation-and-shortcuts*
*Completed: 2026-05-06*
