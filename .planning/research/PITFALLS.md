# Research: v1.6 Localized Keybind Management - Pitfalls

## Pitfall: Collapsing Shortcuts and Access Keys

Microsoft, GNOME, and GTK all distinguish broader shortcuts/accelerators from access keys/mnemonics and focused widget key bindings. If MESH uses one untyped "keybind" shape for everything, authors will not know when a key should be scoped, localized, or focus-local.

Prevention: encode trigger kind explicitly and test dispatch order for each kind.

## Pitfall: Localized Keys Without Scope

Localized access keys naturally collide because translated labels often share first letters. Microsoft recommends scopes, and GNOME warns that collisions can appear only after translation.

Prevention: validate collisions per surface/scope and expose diagnostics. Do not make duplicates fatal across unrelated scopes.

## Pitfall: Breaking Text Input

Single-letter localized access keys can conflict with typing if they are handled as raw key presses in text fields.

Prevention: reserve access-key activation for an explicit access-key mode/modifier or scoped command context. Preserve text-input and focused-control behavior before module shortcuts.

## Pitfall: User Overrides Drift When Labels Change

If overrides are keyed by localized label text, changing a translation breaks user settings.

Prevention: key all overrides by stable `module_id + action_id`, never by display text.

## Pitfall: Wayland Global Shortcut Scope Creep

Global shortcuts on Wayland are permissioned and compositor-mediated. XDG Desktop Portal GlobalShortcuts requires sessions, binding/configuration flows, and activation signals.

Prevention: defer compositor-global registration. Define the action model so a later milestone can export eligible actions to a portal backend.

## Pitfall: Locale Auto-Guessing

Generating keys automatically from translated labels can produce unusable keys, duplicate keys, or keys not present on the user's keyboard.

Prevention: modules/localizers provide explicit locale key hints; MESH validates and falls back rather than guessing silently.

## Pitfall: Diagnostics Hidden From Authors

Keybind bugs are easy to miss if only runtime dispatch fails.

Prevention: publish diagnostics during module load and runtime resolution, and add tests with malformed bindings, duplicate bindings, missing handlers, and missing target refs.

## Pitfall: Accessibility Metadata Lags Effective Binding

If accessibility metadata uses default keys while dispatch uses overrides, assistive technology and visible hints become misleading.

Prevention: accessibility annotation must use the same resolved keybind records as dispatch.
