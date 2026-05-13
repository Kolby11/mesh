# Research: v1.6 Localized Keybind Management - Features

## Table Stakes

### Module-Declarations

- Frontend modules declare keybind actions with stable ids.
- Each declaration includes purpose text or an i18n label key, a handler, an optional target control reference, and default triggers.
- The schema distinguishes shortcuts/accelerators from localized access keys and focused-widget bindings.

### Runtime Resolution

- The shell resolves the active trigger set from module defaults, active locale, and user overrides.
- User overrides win over locale defaults, which win over generic module defaults.
- Disabled or empty overrides can intentionally remove a module default.
- Resolution is deterministic and testable without launching Wayland.

### Localized Access Keys

- Modules can define locale-specific access keys for action labels.
- Access keys can follow translated action purpose, such as `Accept -> A` in English and `Prijat -> P` in Slovak.
- The resolver validates that access keys are usable single alphanumeric keys where possible and avoids duplicate keys in the same scope.
- If a locale does not define an access key, the resolver falls back to the generic default.

### Script Dispatch

- Scripts receive a keybind event with action id, trigger type, key, modifiers, locale, target metadata, and resolved label.
- Keybinds can activate functions, buttons, popovers, and service commands through existing named handler calls.
- Existing focused-control behavior for Enter, Space, arrows, Tab, Escape, text input, and Ctrl+C remains intact.

### Diagnostics and Discoverability

- Duplicate bindings in the same surface/scope are visible through diagnostics.
- Resolved shortcut/access-key metadata is attached to accessibility data.
- Shipped surfaces can display or expose shortcut text where appropriate.

## Differentiators

- Locale-aware keybind declarations are first-class module authoring data, not just shell user configuration.
- The same action id can drive handler dispatch, accessibility, diagnostics, and later settings UI.
- The model leaves room for future compositor-global registration while keeping the first milestone focused and reliable.

## Deferred Features

- Full keybind settings UI.
- Compositor-global shortcuts through XDG Desktop Portal GlobalShortcuts.
- Automatic translation or automatic access-key generation from translated labels.
- Complex multi-key Office-style access-key sequences beyond the first stable access-key model.

## Feature Risks

- Overloading "keybind" to mean shortcut, mnemonic, and widget key binding will create ambiguity. The implementation should name these concepts explicitly.
- Localized access keys must be scoped; otherwise duplicate letters will make common surfaces unreliable.
- User overrides must remain stable when module labels or locale files change.
