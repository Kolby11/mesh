# Phase 33 Research: Locale-Aware Keybind Resolution

## Research Complete

Phase 33 builds directly on Phase 32. The code already has typed `keybinds.actions`, stable action ids, a shell-side surface shortcut bridge, a locale engine, and user overrides keyed by `surface_id + action_id`. The remaining work is to extend the declaration model with typed localized trigger defaults and route shell shortcut resolution through deterministic locale-aware fallback.

## Existing Implementation Surface

### Manifest Model

- `crates/core/extension/module/src/manifest/model.rs` defines:
  - `KeybindsSection { actions: HashMap<String, KeybindAction> }`
  - `KeybindAction { handler, target_ref, scope, label, label_i18n_key, trigger }`
  - `KeybindTrigger { kind, key, modifiers }`
  - `KeybindScope::{Surface, Access}`
  - `KeybindTriggerKind::{Shortcut, AccessKey}`
- JSON, TOML, and package-style manifests already pass keybind declarations into normalized `Manifest`.
- Existing validation rejects malformed generic action data. For Phase 33, localized trigger entries should be typed but not fatal when blank or incomplete, per context decision D-06.

### Shell Runtime

- `crates/core/shell/src/shell/component/input/keyboard.rs` owns current shortcut resolution.
- `ResolvedSurfaceShortcut` currently carries `action_id`, `key`, `handler`, and `target_ref`.
- `resolved_surface_shortcuts` currently applies user overrides from `KeyboardSettings.surface_shortcuts[self.surface_id()][action_id].key` before dispatch.
- Shell-global shortcuts remain outside this component path and already run earlier.

### Locale Runtime

- `mesh-core-locale` exposes `LocaleEngine::current()` and `fallback_chain()`.
- Shell locale changes call `FrontendSurfaceComponent::locale_changed`, which updates the component's `LocaleEngine`.
- Module settings can also set `settings_json.i18n.default_locale`, and `reload_module_settings` applies that locale to the component.

## Decisions From CONTEXT.md

- `localized_triggers` lives inside each action.
- Localized entries override trigger only.
- Resolver precedence is:
  1. User override by `surface_id + action_id`
  2. Exact active locale trigger
  3. Parent locale trigger
  4. Generic action trigger
  5. No binding
- Missing, incomplete, or blank localized entries fall back silently in Phase 33.
- Phase 33 localizes access keys only; regular shortcut localization is out of scope.

## Recommended Implementation Shape

### 1. Extend `KeybindAction`

Add:

```rust
#[serde(default)]
pub localized_triggers: HashMap<String, KeybindTrigger>,
```

Use the existing `KeybindTrigger` type so localized entries preserve `kind`, `key`, and `modifiers`. Do not call the existing strict `KeybindTrigger::validate` on localized entries in Phase 33, because blank or incomplete localized entries must fall back silently. Manifest tests should prove parse and round-trip behavior.

### 2. Add Parent Locale Expansion

Add a small helper with deterministic output:

```rust
fn keybind_locale_candidates(active_locale: &str) -> Vec<String>
```

Examples:

- `sk-SK` -> `["sk-SK", "sk"]`
- `sk` -> `["sk"]`
- `""` -> `[]`

This can live near the resolver in shell keyboard code for Phase 33. A later phase can move it into `mesh-core-locale` if more consumers need it.

### 3. Add Resolver Source Metadata

Internal resolution should know where a binding came from. Suggested enum:

```rust
enum KeybindResolutionSource {
    UserOverride,
    LocaleDefault { locale: String },
    ModuleDefault,
}
```

Phase 33 does not need to expose this to script dispatch yet, but tests should assert precedence. Phase 34 can reuse it for enriched dispatch payloads.

### 4. Restrict Localized Defaults To Access Keys

When resolving an action:

- If user override key is present and nonblank, it wins for both shortcut and access-key actions.
- If the generic trigger kind is `AccessKey`, check `localized_triggers` by exact locale then parent locale.
- If the generic trigger kind is `Shortcut`, skip localized triggers and use the generic trigger.
- If a localized trigger is missing, blank, or uses a non-`AccessKey` kind, ignore it and continue fallback.
- Generic trigger remains the final fallback for valid actions.

This preserves Phase 32 shortcut behavior and matches the user decision to localize access keys only.

### 5. Integrate In Existing Shortcut Bridge

Update `FrontendSurfaceComponent::manifest_surface_shortcut_declarations` or its immediate caller so the declaration includes:

- `action_id`
- `default_trigger`
- `localized_triggers`
- `handler`
- `target_ref`

Then resolve the effective key with:

- keyboard settings override
- `self.locale.current()`
- action localized trigger map
- generic trigger

Legacy `settings_json.keyboard.shortcuts` declarations do not have typed `localized_triggers`; they should keep current generic behavior.

## Validation Architecture

Phase 33 should include tests at two levels:

1. **Manifest parse tests** in `mesh-core-module`:
   - `parses_module_json_localized_keybind_triggers`
   - asserts `localized_triggers["sk"].key == "p"` and `kind == AccessKey`
   - proves missing `localized_triggers` keeps existing keybind tests passing

2. **Shell resolver tests** in `mesh-core-shell`:
   - user override beats locale and generic
   - exact locale beats parent and generic
   - parent locale beats generic
   - blank localized trigger falls back to generic
   - shortcut actions ignore localized triggers and keep generic shortcut
   - stable action id remains the override key

Use `nix develop` for shell tests because native Wayland dependencies are not available in the raw host shell.

## Risks And Guardrails

- **Risk:** Accidentally validating localized blank triggers as manifest errors.  
  **Guardrail:** Localized trigger validation should be permissive in Phase 33; diagnostics are Phase 35.

- **Risk:** Localizing shortcuts changes muscle-memory bindings.  
  **Guardrail:** Localized defaults apply only to `AccessKey` triggers.

- **Risk:** Resolver duplicates Phase 34 dispatch expansion.  
  **Guardrail:** Phase 33 should keep dispatch behavior unchanged and only resolve effective binding keys.

- **Risk:** Existing shortcut behavior regresses.  
  **Guardrail:** Keep current `keyboard_shortcuts` and `navigation_bar_keyboard_shortcut` tests, and add focused resolver tests.

## Files Likely To Change

- `crates/core/extension/module/src/manifest/model.rs`
- `crates/core/extension/module/src/manifest/tests.rs`
- `crates/core/shell/src/shell/component/input/keyboard.rs`
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs`

Optional, only if planner chooses extraction:

- `crates/core/shell/src/shell/component/input/keybind_resolver.rs`
- `crates/core/shell/src/shell/component/input/mod.rs`

## Recommended Verification Commands

- `nix develop -c cargo fmt --check`
- `nix develop -c cargo test -p mesh-core-module localized_keybind`
- `nix develop -c cargo test -p mesh-core-shell keybind_locale`
- `nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts`
