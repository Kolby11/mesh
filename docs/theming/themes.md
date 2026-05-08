# Themes

A theme is a **token set plus component defaults**. Scalar design values such
as colour, typography, spacing, radius, elevation, animation, and icon-style
live in `tokens`. Default styles for primitives such as `button` live under
`defaults.components`. Installed frontend modules may also contribute their own
theme-owned token and component-default subtrees under `modules`.

The theme system is extensible through the same contract-and-registry model
used for services (see [`../extensibility.md`](../extensibility.md)). Nothing
in the core assumes which themes exist.

## Model

1. **Tokens, not variables.** Components reference named tokens
   (`color.primary`, `spacing.md`); the active theme supplies the values.
2. **One theme active at a time.** Unlike icon packs, themes are
   winner-takes-all — mixed token sets would produce incoherent UI.
3. **Themes are modules.** `mesh.kind = "theme"` in `package.json`, with
   selectable token modes contributed through `mesh.contributes.themes`.
4. **Component defaults inherit from `base`.** Every rendered element starts
   from `ComputedStyle::default()`, then inherits `defaults.components.base`,
   then its tag-specific defaults such as `defaults.components.button`.
5. **Mode variants live inside a theme.** Dark / light / high-contrast are
   modes, not separate themes. The user picks the mode; the theme supplies
   the token set for that mode.
6. **Frontend module theme contributions are stored in the active theme.**
   When a frontend module declares `mesh.theme`, Mesh validates it and writes
   it under `modules.<module-id>` in the authored theme file.

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

## Theme Sources

MESH keeps three distinct theme sources:

1. **Base shell defaults file**
   This is the canonical template for new themes and recovery. It contains the
   stock root `tokens` and `defaults.components` tree.
2. **Authored active theme file**
   This is the theme the user actually edits and selects. It starts from the
   base defaults template, then carries user changes plus installed frontend
   module subtrees under `modules`.
3. **Installed frontend module manifests**
   Frontend modules may declare `mesh.theme`, and Mesh writes those
   contributions into the authored active theme file under their module ID.

Theme modules still exist as packages with `mesh.kind = "theme"`, but the
runtime contract is the authored active theme JSON that the shell loads and
mutates.

Minimal theme package metadata:

```json
{
  "name": "@mesh/default-theme",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "theme",
    "contributes": {
      "themes": [
        {
          "id": "@mesh/default-theme",
          "label": "MESH Default",
          "default_mode": "dark"
        }
      ]
    }
  }
}
```

### Theme File Format

Theme files keep root tokens under `tokens`, root component defaults under
`defaults.components`, and installed frontend-module contributions under
`modules`:

```json
{
  "id": "my-theme",
  "name": "My Theme",
  "tokens": {
    "color.primary": "#7A5AF8",
    "color.on-primary": "#FFFFFF",
    "color.surface": "#121217",
    "color.on-surface": "#E7E5EA",
    "color.on-surface-variant": "#A8A5AE",
    "color.error": "#F7375A",

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

    "animation.duration.fast": 90.0,
    "animation.curves.bezier.standard": "cubic-bezier(0.2, 0.0, 0, 1.0)",
    "animation.default.border-radius": "border-radius token(animation.duration.fast) token(animation.curves.bezier.standard)",

    "icon.size.md": "16px",
    "icon.weight": 400,
    "icon.style": "rounded"
  },
  "defaults": {
    "components": {
      "base": {
        "color": "token(color.on-primary)",
        "transition": "color token(animation.duration.fast) token(animation.curves.bezier.standard)"
      },
      "button": {
        "background": "token(color.primary)",
        "border-radius": "token(radius.md)"
      }
    }
  },
  "modules": {
    "@mesh/weather": {
      "tokens": {
        "weather.color.sunny": "#F6B73C"
      },
      "defaults": {
        "components": {
          "base": {
            "transition": "background-color token(animation.duration.fast) token(animation.curves.bezier.standard)"
          },
          "button": {
            "border-width": "token(border.width.medium)"
          },
          "weather-chip": {
            "background": "token(@mesh/weather.weather.color.sunny)"
          }
        }
      }
    }
  }
}
```

The authored active theme file is the source of truth the shell reads at
runtime. When a frontend module is installed, Mesh validates its `mesh.theme`
block and writes that data into the active theme file under `modules`.

### Groups

Groups are the prefixes (`color`, `spacing`, `typography`, `radius`,
`elevation`, `animation`, `shadow`, `icon`). A theme may introduce new groups;
consumers asking for an unknown token get `nil` and are expected to handle
it. The core reserves no group names — theme authors can extend freely.

Within the `animation` group, primitive values and default recipes stay
separate. Primitive tokens such as `animation.duration.fast` and
`animation.curves.bezier.standard` provide reusable numbers and easing curves.
Recipe tokens such as `animation.default.border-radius` bundle a full property
animation contract and must keep explicit `token(...)` references in the value
so authors can see which primitives the recipe composes.

### Component Defaults

`defaults.components.base` is the inheritance root for every element. Tag
defaults such as `defaults.components.button` layer on top of `base`.

Transitions are authored as normal style declarations inside the component
defaults. There is no separate `transition-color` or shell-only animation
namespace:

```json
{
  "defaults": {
    "components": {
      "base": {
        "transition": "color token(animation.duration.fast) token(animation.curves.bezier.standard), border-radius token(animation.duration.medium) token(animation.curves.bezier.standard)"
      }
    }
  }
}
```

If a component default declares its own `transition`, it replaces the inherited
transition string entirely.

### Resolution Order

For any rendered node, defaults resolve in this order:

1. `ComputedStyle::default()`
2. base shell defaults file
3. active theme `defaults.components.base`
4. active theme `defaults.components.<tag>`
5. current module subtree `defaults.components.base`
6. current module subtree `defaults.components.<tag>`
7. local stylesheet rules
8. pseudo-state rules such as `:hover`, `:focus`, `:active`

Module component defaults are subtree-scoped. A frontend module's `base` and
tag-specific defaults affect only that module's rendered subtree, not the whole
shell.

### Module Contributions

Frontend modules may contribute:

- module-owned tokens under `modules.<module-id>.tokens`
- subtree-scoped component defaults under
  `modules.<module-id>.defaults.components`

For now, module contributions are not theme-variant-specific. A frontend
module writes one theme contribution block, and Mesh installs that block into
the authored active theme file.

Cross-module token reads must be explicit:

```css
color: token(@mesh/weather.weather.color.sunny);
```

Root theme tokens remain unqualified:

```css
color: token(color.primary);
```

If a module is uninstalled and another module still references one of its
tokens, Mesh emits an unresolved token warning.

## Creating A Theme

New themes should be created from the base shell defaults file, then edited as
authored active themes.

The intended workflow is:

1. copy the base shell defaults template
2. change root `tokens`
3. change root `defaults.components`
4. select the new theme in settings
5. let Mesh install frontend module contributions into `modules`

This gives users a stable recovery path: if a theme becomes inconsistent, they
can create a fresh theme from the base defaults template and reapply changes.

## Mode switching

`set_mode(name)` changes the active mode for the current theme without
changing the theme itself. Typical use: tying mode to a system dark/light
preference, or scheduling dark mode at night.

In v1, frontend module theme contributions are not mode-specific. The same
`modules.<module-id>` subtree remains installed regardless of the selected
theme mode.

Mode is part of `~/.mesh/settings.json` (see [`../settings/README.md`](../settings/README.md)):

```json
{
  "theme": {
    "active": "@mesh/default-theme",
    "mode":   "dark"
  }
}
```

Switching theme is the same key — `"active"` points at a different authored
theme file — and emits `ThemeChanged`. Switching mode emits `ThemeChanged`
with the same theme ID and a new mode.

## Per-Component Overrides

A module may request component-scoped overrides in its `<style>` block
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

## Theme Storage

User-authored active theme JSON lives under `~/.mesh/themes/`. The repo
fallback for development remains `config/themes/`.

MESH also keeps a separate base shell defaults file that acts as the template
for new themes and as the stable recovery point if a user wants to reset a
broken theme to known-good defaults.

## Hot-swap

Changing theme is hot. The active implementation is replaced in the
registry, `ThemeChanged` fires, every surface re-resolves its `token(...)`
references on the next frame. No module code runs during the swap; the core
owns it.

## Auto-detect

When no theme is pinned, auto-detection picks the highest-priority installed
theme template and activates its authored copy. In practice this is
`@mesh/default-theme` for a stock install, or a distro-provided theme on distro
installs.

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
- The authored active theme JSON is the runtime source of truth.
- A separate base shell defaults file is the template and recovery point.
- Root token values live under `tokens`; root component defaults live under
  `defaults.components`.
- Frontend module theme contributions are stored under `modules.<module-id>`.
- Modes (dark/light/high-contrast) are inside a single theme, selectable at runtime.
- `defaults.components.base` is the inheritance root for all elements.
- Components reference `token(name)`; cross-module token reads use explicit
  module syntax such as `token(@mesh/weather.weather.color.sunny)`.
- Icon style and icon size tokens live in the same namespace, so themes can
  tune the icon look without shipping a new icon pack.
