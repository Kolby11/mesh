# Phase 32 Patterns: Keybind Declaration Contract

## Existing Analog Files

| Target Area | Closest Analog | Pattern to Reuse |
|-------------|----------------|------------------|
| Normalized manifest fields | `crates/core/extension/module/src/manifest/model.rs` | Add optional sections to `Manifest` with `#[serde(default)]`; define typed structs near related declaration models. |
| JSON manifest conversion | `crates/core/extension/module/src/manifest/json.rs` | Add fields to `JsonManifest`, import the normalized types, and pass through during `into_manifest`. |
| TOML manifest conversion | `crates/core/extension/module/src/manifest/toml.rs` | Add optional TOML-compatible fields and convert into normalized manifest fields. |
| Package manifest conversion | `crates/core/extension/module/src/package/module_manifest.rs` | Map package-style manifest data into normalized runtime manifest fields in `into_runtime_manifest`. |
| Existing shortcut dispatch | `crates/core/shell/src/shell/component/input/keyboard.rs` | Preserve `KeyboardSettings.surface_shortcuts` override lookup and handler dispatch path while replacing ad hoc JSON parsing with typed declarations. |
| Existing shortcut tests | `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` | Extend current shortcut tests instead of creating unrelated test modules. |

## Concrete Existing Shapes

Current settings shortcut shape:

```json
{
  "keyboard": {
    "shortcuts": {
      "mute": {
        "key": "m",
        "handler": "onMuteShortcut",
        "target_ref": "volume-button"
      }
    }
  }
}
```

Current user override path:

```rust
keyboard.surface_shortcuts["@mesh/navigation-bar"]["mute"].key
```

Current dispatch path:

```rust
self.dispatch_surface_shortcut(&tree, &key, modifiers, &keyboard_settings)?
```

## Recommended Names

- `KeybindsSection`
- `KeybindAction`
- `KeybindTrigger`
- `KeybindScope`
- `KeybindTriggerKind`
- `ResolvedSurfaceShortcut` can stay internal for now, but should be built from `KeybindAction`.

## File Ownership for Plan

- Manifest contract: `crates/core/extension/module/src/manifest/model.rs`, `json.rs`, `toml.rs`, `tests.rs`, `package/module_manifest.rs`
- Runtime bridge: `crates/core/shell/src/shell/component/input/keyboard.rs`
- Runtime tests: `crates/core/shell/src/shell/component/tests/interaction/navigation.rs`
- Proof manifest/settings: `modules/frontend/navigation-bar/module.json`, `modules/frontend/navigation-bar/config/settings.json`
