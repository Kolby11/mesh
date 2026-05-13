# Research: v1.6 Localized Keybind Management - Stack

## Scope

This milestone should build on MESH's existing keyboard and localization stack rather than introduce a new shortcut subsystem from scratch.

## Existing MESH Stack

- `crates/core/shell/src/shell/component/input/keyboard.rs` already resolves surface shortcuts from `settings_json.keyboard.shortcuts`, applies user overrides from `mesh_core_config::KeyboardSettings`, dispatches named handlers, and annotates `accessibility.keyboard_shortcut`.
- `crates/core/foundation/config/src/lib.rs` already has `KeyboardSettings` with standard button/toggle/slider keys and `surface_shortcuts` overrides keyed by surface id and shortcut id.
- `crates/core/shell/src/shell/runtime/wayland.rs` routes keyboard events to the focused surface and lets shell-global debug shortcuts win before component handling.
- `modules/frontend/navigation-bar/config/settings.json` already declares a `keyboard.shortcuts.mute` default bound to `m`.
- `modules/frontend/navigation-bar/config/i18n/en.json` and `sk.json` prove frontend modules already ship locale resources.

## External Stack Guidance

- Microsoft separates access keys from shortcut keys. Access keys are Alt-style key sequences tied to labelled controls, while shortcut keys/accelerators invoke common actions without navigating the UI.
- Microsoft recommends localizing shortcut keys when action names are localized, while preserving common conventions per language.
- Microsoft and GNOME both recommend scoping access keys to avoid cognitive load and collisions.
- GTK models accelerators, mnemonics, and widget key bindings as distinct concepts; that maps cleanly to MESH's current global shell shortcut, surface shortcut, and focused-widget input paths.
- XDG Desktop Portal GlobalShortcuts is the relevant later stack for compositor-wide shortcuts on Wayland, but it has session, binding, permission, and configuration flows that are larger than this milestone.

## Recommended Stack Additions

- Add typed keybind manifest/config models in `mesh-core-extension-module` and `mesh-core-foundation-config` instead of reading arbitrary JSON at dispatch time.
- Add a resolver module in shell/component input that produces `ResolvedKeybind` records from module defaults, locale defaults, and user overrides.
- Add manifest validation diagnostics for malformed triggers, missing handlers, missing i18n labels, and duplicate action ids.
- Extend accessibility metadata with resolved shortcut/access-key text and a stable action id.
- Keep key capture and dispatch inside the existing shell/component keyboard pipeline.

## Sources

- Microsoft keyboard shortcuts and localization: https://learn.microsoft.com/en-us/globalization/input/hotkeys-accelerators
- Microsoft access keys: https://learn.microsoft.com/en-us/windows/apps/develop/input/access-keys
- Microsoft keyboard accelerators: https://learn.microsoft.com/en-us/windows/apps/develop/input/keyboard-accelerators
- GNOME keyboard HIG: https://developer.gnome.org/hig/guidelines/keyboard.html
- GTK input handling: https://gnome.pages.gitlab.gnome.org/gtk/gtk4/input-handling.html
- XDG Desktop Portal GlobalShortcuts: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html
