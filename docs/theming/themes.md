# Themes

A theme is a **token set** — colour, typography, spacing, radius, elevation,
motion, and icon-style values — that the whole shell inherits from. Themes
are ordinary plugins: anyone can ship one, the user selects one, and the
switch is hot.

The theme system is extensible through the same contract-and-registry model
used for services (see [`../extensibility.md`](../extensibility.md)). Nothing
in the core assumes which themes exist.

## Model

1. **Tokens, not variables.** Components reference named tokens
   (`color.primary`, `spacing.md`); the active theme supplies the values.
2. **One theme active at a time.** Unlike icon packs, themes are
   winner-takes-all — mixed token sets would produce incoherent UI.
3. **Themes are plugins.** `type = "theme"`. They implement the `mesh.theme`
   interface.
4. **Inheritance.** A theme may `extends` another and override only the
   tokens it cares about.
5. **Mode variants live inside a theme.** Dark / light / high-contrast are
   modes, not separate themes. The user picks the mode; the theme supplies
   the token set for that mode.

## The `mesh.theme` contract

```
interface: mesh.theme
version:   1.0
methods:
  token(name: string) -> Value?
  tokens(group: string) -> map<string, Value>
  all() -> map<string, Value>
  modes() -> [string]
  active_mode() -> string
  set_mode(name: string) -> Result
events:
  ThemeChanged(theme_id: string, mode: string)
  TokenChanged(name: string, value: Value)
```

Frontends consume tokens through this proxy. The core's `token(...)`
helper inside `<style>` blocks resolves through the active `mesh.theme`
implementation.

## Theme packages

```
@mesh/default-theme/
  mesh.toml
  tokens/
    base.json
    modes/
      dark.json
      light.json
      high-contrast.json
```

```toml
[package]
id   = "@mesh/default-theme"
type = "theme"

[service]
provides = "mesh.theme"
priority = 100

[theme]
base   = "tokens/base.json"          # shared across modes
modes  = { dark = "tokens/modes/dark.json",
           light = "tokens/modes/light.json",
           high-contrast = "tokens/modes/high-contrast.json" }
default_mode = "dark"
```

### Token file format

Token files are flat JSON, one entry per fully-qualified token name:

```json
{
  "color.primary":          "#7A5AF8",
  "color.on-primary":       "#FFFFFF",
  "color.surface":          "#121217",
  "color.on-surface":       "#E7E5EA",
  "color.on-surface-variant": "#A8A5AE",
  "color.error":            "#F7375A",

  "spacing.xs": "2px",
  "spacing.sm": "4px",
  "spacing.md": "8px",
  "spacing.lg": "16px",

  "radius.sm": "4px",
  "radius.md": "8px",
  "radius.lg": "16px",

  "typography.size.sm": "12px",
  "typography.size.md": "14px",
  "typography.size.lg": "18px",

  "icon.size.md":  "16px",
  "icon.weight":   400,
  "icon.style":    "rounded"
}
```

A mode file overrides only the keys that differ. Unspecified keys fall
through to `base.json`.

### Groups

Groups are the prefixes (`color`, `spacing`, `typography`, `radius`,
`elevation`, `motion`, `shadow`, `icon`). A theme may introduce new groups;
consumers asking for an unknown token get `nil` and are expected to handle
it. The core reserves no group names — theme authors can extend freely.

## Extending an existing theme

A theme plugin may extend another and override selectively:

```toml
[theme]
extends  = "@mesh/default-theme@>=1.0"
base     = "tokens/accent.json"       # overrides on top of the base of the extended theme
modes    = { dark = "tokens/modes/dark.json" }
```

Resolution order for any token:

1. This theme's active-mode file
2. This theme's base file
3. Extended theme's active-mode file
4. Extended theme's base file
5. `nil` — consumer's fallback

Chains are allowed but must terminate. Circular `extends` is rejected at
load.

## Mode switching

`set_mode(name)` changes the active mode for the current theme without
changing the theme itself. Typical use: tying mode to a system dark/light
preference, or scheduling dark mode at night.

Mode is part of user settings (see [`../settings/README.md`](../settings/README.md)):

```json
{
  "theme": {
    "active": "@mesh/default-theme",
    "mode":   "dark"
  }
}
```

Switching theme is the same key — `"active"` points at a different plugin
ID — and emits `ThemeChanged`. Switching mode emits `ThemeChanged` with the
same ID and a new mode.

## Per-plugin overrides

A plugin may request component-scoped overrides in its `<style>` block
without editing the global theme:

```css
.my-widget {
  --color-primary: token(color.tertiary);   /* override just inside this class */
  color: token(color.on-primary);
}
```

Overrides cascade through the component tree like CSS variables. The
registry-level `token(name)` call still returns the theme's value; only the
style scope is affected.

## Hot-swap

Changing theme is hot. The active implementation is replaced in the
registry, `ThemeChanged` fires, every surface re-resolves its `token(...)`
references on the next frame. No plugin code runs during the swap; the core
owns it.

## Auto-detect

When no theme is pinned, auto-detection picks the highest-priority installed
theme that reports compatibility. In practice this is `@mesh/default-theme`
for a stock install, a distro-provided theme on distro installs.

## Tooling

```
mesh themes list                  # installed themes and their modes
mesh themes active                # current theme + mode
mesh themes set <id> [--mode m]   # switch
mesh themes tokens [group]        # dump effective tokens
mesh themes which <token-name>    # show which layer supplied a value
```

## Summary

- One active `mesh.theme`, user-switchable, hot-swappable.
- Token values live in JSON files shipped with the theme plugin.
- Modes (dark/light/high-contrast) are inside a single theme, selectable at runtime.
- Themes may `extends` another for partial overrides.
- Components reference `token(name)`; per-component overrides cascade like CSS variables.
- Icon style and icon size tokens live in the same namespace, so themes can
  tune the icon look without shipping a new icon pack.
