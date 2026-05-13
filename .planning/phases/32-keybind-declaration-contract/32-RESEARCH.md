# Phase 32 Research: Keybind Declaration Contract

**Phase:** 32 - Keybind Declaration Contract
**Date:** 2026-05-13
**Status:** Complete

## Objective

Research what Phase 32 needs to plan well: adding a typed module keybind declaration model while preserving the existing navigation-bar shortcut behavior.

## Phase Boundary

Phase 32 covers only the declaration contract and compatibility bridge. It should not implement locale-aware resolution, conflict diagnostics, or full script event enrichment beyond what is needed to preserve current behavior.

Requirements covered:

- `KEYB-01`: Frontend modules can declare semantic keybind actions with stable action ids.
- `KEYB-02`: Each keybind action can define handler, target control reference, scope, label/i18n key, and default trigger metadata.
- `KEYB-03`: Manifest/settings parsing validates keybind declarations into typed Rust structures instead of relying on ad hoc JSON at dispatch time.

## Current Code Evidence

- `crates/core/extension/module/src/manifest/model.rs` defines the normalized `Manifest` shape and is the right source of truth for typed module declarations.
- `crates/core/extension/module/src/manifest/json.rs` converts current `module.json` files into normalized `Manifest` records.
- `crates/core/extension/module/src/manifest/toml.rs` converts legacy TOML manifests into the same normalized `Manifest` shape.
- `crates/core/extension/module/src/package/module_manifest.rs` converts package-style manifests into normalized runtime manifests.
- `crates/core/shell/src/shell/component/input/keyboard.rs` currently defines `ResolvedSurfaceShortcut` and parses `settings_json.keyboard.shortcuts` at dispatch time.
- `modules/frontend/navigation-bar/config/settings.json` currently declares `keyboard.shortcuts.mute` with `key`, `handler`, and `target_ref`.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` has existing shortcut tests that must continue passing.

## Existing Behavior to Preserve

The current navigation-bar shortcut path:

1. Reads `settings_json.keyboard.shortcuts`.
2. Uses shortcut id as the stable override key.
3. Reads `key`, `handler`, and optional `target_ref`.
4. Applies `KeyboardSettings.surface_shortcuts[surface_id][shortcut_id].key` override.
5. Calls the named handler.
6. Annotates `accessibility.keyboard_shortcut` on the referenced node.

Phase 32 must preserve this behavior while moving parsing toward typed declaration records.

## Recommended Contract Shape

Add typed structures in `manifest/model.rs`:

- `KeybindsSection`
- `KeybindAction`
- `KeybindTrigger`
- `KeybindScope`
- `KeybindTriggerKind`

Required fields for Phase 32:

- action id from the map key
- `handler`
- optional `target_ref`
- optional `scope`, default `surface`
- optional `label`
- optional `label_i18n_key`
- default `trigger` with `key`, optional `modifiers`, and kind defaulting to `shortcut`

Keep locale-specific trigger maps for Phase 33 or parse them as inert optional data without using them in dispatch yet.

## Compatibility Strategy

Phase 32 should support both:

1. New module-level declarations, preferably under a top-level `keybinds` section in normalized manifests.
2. Legacy settings JSON shortcuts under `keyboard.shortcuts`.

The shell can expose a helper that converts legacy `settings_json.keyboard.shortcuts` values into the same declaration struct. This lets execution preserve current runtime behavior while later phases move resolution to manifest-owned declarations.

## Validation Strategy

Validation should be narrow and non-fatal in Phase 32:

- Empty action ids are impossible in JSON maps but should be rejected if constructed manually in tests.
- Missing `handler` invalidates that action.
- Missing trigger key invalidates that action.
- Unknown scope/kind should fail deserialization or fall back only if explicitly designed.
- Valid sibling actions must remain usable when one action is invalid.

## Test Targets

- `crates/core/extension/module/src/manifest/tests.rs`
  - Parse `module.json` with top-level `keybinds`.
  - Assert handler, target ref, scope, label/i18n key, trigger kind/key/modifiers.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs`
  - Existing surface shortcut test still passes.
  - Add a test that the shell component converts settings shortcuts to typed declarations before dispatch.
- `modules/frontend/navigation-bar/module.json`
  - Add schema/documentation for the new keybind declaration shape only if implementation chooses manifest-level declarations in this phase.

## Landmines

- Do not break existing user override lookup by shortcut id.
- Do not make Phase 33 locale resolution decisions inside Phase 32.
- Do not route single-letter access keys before text input behavior.
- Do not implement XDG portal/global shortcuts in this phase.
- Do not move current `settings_json` runtime data wholesale out of the component before checking how settings reload uses it.

## Suggested Plan Shape

One plan is enough:

1. Add typed declaration structs and parsing coverage.
2. Add shell-side compatibility bridge and preserve existing dispatch/annotation behavior.
3. Add manifest/settings proof on navigation bar and verification commands.

## Validation Architecture

Phase 32 can be validated with Rust unit/integration tests:

- Manifest parsing tests in `mesh-core-extension-module`.
- Shell component interaction tests in `mesh-core-shell`.
- Real navigation-bar shortcut regression tests.
- `cargo fmt --check`.
