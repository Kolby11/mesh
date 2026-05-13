# Phase 33 Pattern Map: Locale-Aware Keybind Resolution

## Closest Analogs

### Manifest Model And Tests

- `crates/core/extension/module/src/manifest/model.rs`
  - Existing `KeybindAction`, `KeybindTrigger`, `KeybindScope`, and `KeybindTriggerKind` are the extension point.
  - Pattern: add serde-defaulted typed fields directly to the normalized manifest model.

- `crates/core/extension/module/src/manifest/tests.rs`
  - Existing tests `parses_module_json_keybind_declarations`, `module_json_without_keybinds_has_empty_keybinds`, and `package_json_keybinds_round_trip_to_runtime_manifest` are the test style to copy.
  - Pattern: parse inline JSON into `JsonManifest`, convert to normalized `Manifest`, then assert typed fields.

### Shell Runtime Resolution

- `crates/core/shell/src/shell/component/input/keyboard.rs`
  - Existing `ResolvedSurfaceShortcut` and `SurfaceShortcutDeclaration` form the bridge from declarations/settings to dispatch.
  - Pattern: convert manifest/settings data into typed declarations, apply `KeyboardSettings.surface_shortcuts` overrides, skip blank effective keys, then dispatch by `key_matches_binding`.

- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs`
  - Existing `keyboard_shortcuts_manifest_keybind_action_resolves_user_override_by_action_id` proves stable override identity.
  - Pattern: construct a test component, mutate `component.compiled.manifest.keybinds.actions`, call `resolved_surface_shortcuts`, and assert effective key/action/handler metadata.

### Locale Support

- `crates/core/foundation/locale/src/lib.rs`
  - Existing `LocaleEngine::current()` and `fallback_chain()` expose active locale state.
  - Pattern: keep locale logic deterministic and simple; translation fallback is separate from keybind fallback.

- `crates/core/shell/src/shell/component/shell_component.rs`
  - `locale_changed` updates component locale and rebuilds runtime state.
  - Pattern: resolver should read current component locale at resolution time so locale changes affect resolved bindings without a separate cache.

## Executor Notes

- Do not add Phase 35 diagnostics yet. Blank/incomplete localized entries should be ignored during resolution.
- Do not expand script event payloads yet. Phase 34 owns action id, locale, source, and resolved label payload details.
- Shell tests should run through `nix develop` because the direct host environment can miss native Wayland dependencies.
