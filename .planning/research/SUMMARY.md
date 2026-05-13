# Research Summary: v1.6 Localized Keybind Management

## Recommendation

Build a typed, locale-aware module keybind system on top of MESH's existing surface shortcut path. Keep compositor-global shortcuts out of v1.6.

## Key Findings

- MESH already has a narrow surface shortcut mechanism: `settings_json.keyboard.shortcuts`, shell user overrides, named handler dispatch, and accessibility shortcut annotation.
- Microsoft, GNOME, and GTK all separate three concepts that MESH should model explicitly:
  - shortcuts/accelerators for command activation
  - access keys/mnemonics for localized labelled controls
  - focused-widget key bindings for controls and text input
- Microsoft recommends localizing shortcut keys when action names are localized, while keeping common command conventions consistent per language.
- Localized access keys need scoped collision detection. Duplicates in the same scope should be diagnosed; duplicates in unrelated scopes can be acceptable.
- XDG Desktop Portal GlobalShortcuts is the right future path for compositor-wide shortcuts on Wayland, but it is a separate permission/session problem and should not block module-scoped keybind management.

## Implementation Direction

1. Add typed keybind declarations to module manifest/settings parsing.
2. Add a resolver that merges module defaults, locale defaults, and user overrides.
3. Preserve dispatch order: shell-global shortcuts, protected text/focused-control behavior, module keybinds, then focused handlers.
4. Dispatch script events with action id, trigger metadata, locale, target metadata, and resolved label.
5. Drive accessibility annotation and diagnostics from the same resolved keybind records.
6. Prove the system on navigation bar/audio popover with English and Slovak bindings.

## Suggested Requirement Categories

- Declaration Contract
- Locale Resolution
- Runtime Dispatch
- Diagnostics and Conflict Handling
- Accessibility and Proof Surfaces

## Out of Scope

- Full user-facing keybind settings UI.
- Compositor-global shortcuts through XDG Desktop Portal.
- Automatic translation or automatic access-key generation.
- Replacing existing keyboard focus traversal and control activation behavior.

## Sources

- Microsoft keyboard shortcuts and localization: https://learn.microsoft.com/en-us/globalization/input/hotkeys-accelerators
- Microsoft access keys: https://learn.microsoft.com/en-us/windows/apps/develop/input/access-keys
- Microsoft keyboard accelerators: https://learn.microsoft.com/en-us/windows/apps/develop/input/keyboard-accelerators
- GNOME keyboard HIG: https://developer.gnome.org/hig/guidelines/keyboard.html
- GTK input handling: https://gnome.pages.gitlab.gnome.org/gtk/gtk4/input-handling.html
- XDG Desktop Portal GlobalShortcuts: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html
