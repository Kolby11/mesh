# Research: v1.6 Localized Keybind Management - Architecture

## Existing Architecture Fit

MESH already has most of the event path:

1. Wayland keyboard events enter `Shell::dispatch_wayland`.
2. Shell-global debug shortcuts are checked first.
3. Events are routed to the keyboard-focus surface.
4. `FrontendSurfaceComponent::handle_input` processes focused navigation, clipboard, surface shortcuts, and focused handlers.
5. Surface shortcut dispatch calls a named script handler and can target a referenced node.

The milestone should insert a typed keybind resolver into step 4 and replace direct ad hoc JSON parsing with structured declarations.

## Proposed Data Model

### Module keybind declaration

Each frontend module can declare actions under a manifest/settings key such as:

```json
{
  "keybinds": {
    "accept": {
      "label": "action.accept",
      "handler": "onAccept",
      "target_ref": "accept-button",
      "scope": "surface",
      "shortcut": { "key": "Enter" },
      "access_key": {
        "default": "A",
        "locales": { "sk": "P" }
      }
    }
  }
}
```

Exact file placement should follow existing module settings patterns, but the implementation should parse it into typed Rust structs before runtime dispatch.

### Resolved keybind

The shell resolver should produce:

- `module_id`
- `surface_id`
- `action_id`
- `label`
- `handler`
- `target_ref`
- `scope`
- `trigger_kind` (`shortcut`, `access_key`)
- `key`
- `modifiers`
- `source` (`user_override`, `locale_default`, `module_default`)

## Resolution Precedence

1. User override in shell settings.
2. Locale-specific module default for the active locale.
3. Generic module default.
4. No binding if explicitly disabled.

## Dispatch Order

1. Shell-global shortcuts stay highest priority.
2. Text input and built-in focused-control behavior stays protected.
3. Surface/module keybinds run when their scope matches.
4. Focused node `keydown`/`keyup` handlers remain fallback behavior.

This preserves existing debug shortcut precedence and prevents keybind declarations from breaking text entry.

## Locale and Scope

- Locale resolution should use existing module i18n configuration and active shell locale.
- Access-key collision checks must run per scope, not globally across every surface.
- Duplicates across separate scopes can be valid, matching Microsoft and Office-style access key scoping.

## Diagnostics

Diagnostics should be non-fatal:

- malformed trigger
- unknown modifier/key name
- missing handler
- missing `target_ref`
- duplicate action id
- duplicate trigger in the same scope
- locale access key references a missing locale label

The shell should keep dispatching valid keybinds even when one declaration is invalid.

## Proof Surface

Navigation bar and audio popover are good proof surfaces because they already use:

- `keyboard.shortcuts.mute`
- localized navigation strings
- surface/popover activation handlers
- audio service command dispatch
- accessibility metadata

Use them to prove a localized English/Slovak binding and a user override.
