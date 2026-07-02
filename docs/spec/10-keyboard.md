# 10 — Keyboard

> Part of the [MESH Specification](README.md).

Keyboard behavior has three layers, each with one owner: **traversal and
activation** (shell-owned, uniform everywhere), **keybind actions**
(module-declared contributions, user-remappable), and **focus policy per
surface** (`keyboard_mode` + runtime focus transfers, shell-owned).

## 1. Focus traversal & activation

**Status: shipped.**

- `Tab` / `Shift+Tab` move through the final rendered visual order.
  `tabindex` overrides: `0` = normal order, positive sorts earlier, `-1` =
  pointer/script-focusable but skipped by Tab.
- Menus and menu items support roving `ArrowUp`/`ArrowDown` between
  siblings, Tab transfer, `Escape`, and return focus to the trigger — the
  same traversal works across promoted popover surfaces
  ([03 §1.1](03-components.md)).
- `onkeydown` / `onkeyup` fire only on the focused element (never
  surface-global); handlers receive `event.key` + `event.modifiers`.
- Activation keys are shell props with user overrides in the settings store
  (`shell.keyboard.*`): `button_activation_keys` (Enter, Space),
  `toggle_activation_keys`, `slider_decrement_keys` /
  `slider_increment_keys`.
- `Ctrl+C` copies the current selection on `selectable` text while the
  surface owns keyboard input.
- Shell-owned traversal, activation, cancel, and selection-copy behavior
  cannot be stolen: user overrides that would shadow them are ignored with a
  diagnostic.

## 2. Keybind actions (`mesh.keybinds`)

**Status: shipped.**

Focused-surface keybinds are **semantic actions**, not global hotkeys.
Modules declare actions; controls subscribe; users remap.

```json
"keybinds": {
  "mute": {
    "label":       { "t": "keybind.mute.label", "fallback": "Mute audio" },
    "description": { "t": "keybind.mute.description", "fallback": "Toggle audio mute" },
    "category":    { "t": "keybind.category.audio", "fallback": "Audio" },
    "trigger":     { "kind": "shortcut", "key": "m" },
    "localizedTriggers": { "sk": { "kind": "access_key", "key": "u" } }
  }
}
```

- Identity = declaring module + action id. Labels/descriptions/categories
  are field-local localized text ([07 §6](07-i18n.md)).
- `trigger` is the default; `localizedTriggers` supply locale-specific
  defaults (applied to `access_key` declarations; `shortcut` declarations
  keep their generic key unless the user overrides).
- A control subscribes by setting `keybind="mute"` (or
  `{this.keybinds.mute.id}`) with an `onkeybind` handler.

Effective binding resolution, first match wins:

```
user override (settings store, shell.keyboard.surface_shortcuts)
→ exact-locale access key → parent-locale access key → generic trigger
```

User overrides remap existing action ids only — they cannot create actions;
unknown ids are ignored with a diagnostic.

Diagnostics cover: invalid declarations, duplicate effective bindings,
unresolved/unsafe overrides, template subscriptions without a matching
declaration (`undeclared_keybind_subscription`), declarations without a
runtime handler (`keybind_subscription_missing_handler`), and trigger
conflicts across the installed graph. Resolved bindings surface as
accessibility shortcut metadata ([09 §1](09-accessibility.md)) and in
`mesh.debug.keybinds`.

## 3. Surface keyboard policy (`keyboard_mode`)

**Status: shipped.**

`mesh.surface.keyboard_mode` declares the module-default interactivity
policy; the user may override per module; runtime focus transfers (an open
popover owning focus) override temporarily. The manifest/settings contract
is *policy*; the shell owns actual focus state.

| Mode | Meaning |
| ---- | ------- |
| `none` | Surface never takes keyboard focus (passive chrome). |
| `on_demand` | Focus only when the user engages the surface — preferred for keyboardable shell chrome. |
| `exclusive` | Dedicated keyboard sink while focused (launcher-style). |

Popover focus ownership: activation may transfer focus into the promoted
popover (`options.focus`), the shell records return focus as
`(trigger_surface, trigger_key)`, closes on focus leave, and restores focus
on dismiss. Modules never maintain durable focus state themselves.

## 4. Settings shapes

All keyboard user configuration lives in the settings store
([08 §1](08-settings.md)) under `shell.keyboard`:

```json
"shell": {
  "keyboard": {
    "button_activation_keys": ["Enter", "Space"],
    "slider_increment_keys": ["ArrowRight", "ArrowUp"],
    "surface_shortcuts": {
      "@mesh/navigation-bar": { "mute": { "key": "u" } }
    }
  }
}
```

The generated settings UI lists every module's declared actions (label,
category, current effective binding, conflicts) and writes
`surface_shortcuts` entries.

## 5. Global hotkeys

**Status: deferred non-goal for now.** MESH surfaces are layer-shell
clients; compositor-global hotkeys belong to the compositor. Keybinds here
are focused-surface scoped by design. If a global-hotkey story arrives, it
will be a separate capability-gated contract, not an extension of
`mesh.keybinds` semantics.
