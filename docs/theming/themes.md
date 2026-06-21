# Themes

A theme is a **module with metadata plus a CSS theme file**. Scalar design
values such as colour, typography, spacing, radius, elevation, animation, and
icon-style are authored as CSS custom properties in `theme.css`. Default styles
for primitives such as `button` are authored as semantic CSS rules. The runtime
compiles that CSS into the internal token and component-default model used by
the renderer.

The theme system is extensible through the same contract-and-registry model
used for services (see [`../extensibility.md`](../extensibility.md)). Nothing
in the core assumes which themes exist.

## Model

1. **CSS variables compile to theme tokens.** Themes author variables such as
   `--color-primary`; components reference the same semantic variables with
   `var(--color-primary)`.
2. **One theme active at a time.** Unlike icon packs, themes are
   winner-takes-all — mixed token sets would produce incoherent UI.
3. **Themes are modules.** `mesh.kind = "theme"` in `module.json`; the theme
   metadata points at the CSS entry file.
4. **Component defaults inherit from `node`.** Every rendered element starts
   from `ComputedStyle::default()`, then inherits the theme's `node` rule, then
   its tag-specific defaults such as `button`.
5. **Mode variants live inside a theme.** Dark / light / high-contrast are
   modes, not separate themes. The user picks the mode; the theme supplies
   the token set for that mode.
6. **Frontend module theme contributions are explicitly scoped.** Module-owned
   variables and defaults live under explicit module scopes so they do not leak
   into the whole shell.

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

Frontends can consume raw tokens through this proxy. In `<style>` blocks,
authors should use `var(--...)`; the style resolver maps missing local
custom properties through the active `mesh.theme` implementation.

## Theme Sources

MESH keeps three distinct theme sources:

1. **Base shell defaults file**
   This is the canonical template for new themes and recovery. It contains the
   stock root variables and semantic default rules.
2. **Authored active theme package**
   This is the theme the user actually edits and selects. It starts from the
   base defaults template, then carries user changes in `theme.css`.
3. **Installed frontend module manifests**
   Frontend modules may declare `mesh.theme`, and Mesh installs those
   contributions under explicit module scopes.

Theme modules are normal modules with `mesh.kind = "theme"`. Their metadata
lives in `module.json`; their visual contract lives in `theme.css`.

Minimal theme package metadata:

```json
{
  "name": "@mesh/default-theme",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "theme",
    "theme": {
      "id": "mesh-default",
      "label": "MESH Default",
      "entry": "theme.css",
      "defaultMode": "dark",
      "modes": ["dark", "light"]
    }
  }
}
```

### Theme File Format

Theme packages use this layout:

```text
theme/
  module.json
  theme.css
```

`theme.css` keeps root tokens in `:root` and semantic component defaults in
element rules:

```css
:root {
  --color-primary: #7A5AF8;
  --color-on-primary: #FFFFFF;
  --color-surface: #121217;
  --color-on-surface: #E7E5EA;
  --spacing-md: 8;
  --radius-md: 8;
  --typography-size-md: 14;
  --animation-duration-fast: 90;
  --animation-curves-bezier-standard: cubic-bezier(0.2, 0.0, 0, 1.0);
}

node {
  color: var(--color-on-surface);
  transition: color var(--animation-duration-fast) var(--animation-curves-bezier-standard);
}

button {
  background: var(--color-primary);
  border-radius: var(--radius-md);
}

button:hover {
  background: var(--color-primary-container);
}
```

CSS custom property names map to token names through the known theme groups.
For example:

```css
--color-on-primary         /* var(--color-on-primary) */
--typography-size-md       /* var(--typography-size-md) */
```

The authored active theme package is the source of truth the shell reads at
runtime. Legacy JSON theme files are still loadable as a compatibility path, but
new themes should use the package format.

### Groups

Groups are the prefixes (`color`, `spacing`, `typography`, `radius`,
`elevation`, `animation`, `shadow`, `icon`). A theme may introduce new groups;
consumers asking for an unknown token get `nil` and are expected to handle
it. The core reserves no group names — theme authors can extend freely.

Within the `animation` group, primitive values and default recipes stay
separate. Primitive tokens such as `animation.duration.fast` and
`animation.curves.bezier.standard` provide reusable numbers and easing curves.
Recipe tokens such as `animation.default.border-radius` bundle a full property
animation contract and should use explicit `var(--...)` references in the value
so authors can see which primitives the recipe composes.

### Component Defaults

`node` is the inheritance root for every element. Tag defaults such as `button`
layer on top of `node`.

Transitions are authored as normal CSS declarations inside semantic rules.
There is no separate `transition-color` or shell-only animation namespace:

```css
node {
  transition: color var(--animation-duration-fast) var(--animation-curves-bezier-standard),
              border-radius var(--animation-duration-medium) var(--animation-curves-bezier-standard);
}
```

If a component default declares its own `transition`, it replaces the inherited
transition string entirely.

Theme CSS intentionally supports a restricted selector surface. Root theme
rules should use `:root`, `node`, semantic element selectors such as `button`,
`label`, `slider`, and supported pseudo states such as `button:hover` and
`button:focus`. Theme authors should not target arbitrary implementation
classes globally; module-specific selectors belong in explicit module scopes.

### Resolution Order

For any rendered node, defaults resolve in this order:

1. `ComputedStyle::default()`
2. base shell defaults package
3. active theme `node`
4. active theme semantic tag rule, such as `button`
5. current module subtree `node`
6. current module subtree semantic tag rule
7. local stylesheet rules
8. pseudo-state rules such as `:hover`, `:focus`, `:active`

Module component defaults are subtree-scoped. A frontend module's `base` and
tag-specific defaults affect only that module's rendered subtree, not the whole
shell.

### Module Contributions

Frontend modules may contribute:

- module-owned variables under an explicit module scope
- subtree-scoped component defaults under that module scope

For now, module contributions are not theme-variant-specific. A frontend
module writes one theme contribution block, and Mesh installs that block into
the authored active theme package.

Example module scope:

```css
@module "@mesh/weather" {
  :root {
    --weather-color-sunny: #F6B73C;
  }

  button {
    border-width: var(--border-width-medium);
  }

  weather-chip {
    background: var(--weather-color-sunny);
  }
}
```

Module-scoped theme values use module-owned variables:

```css
color: var(--weather-color-sunny);
```

Root theme tokens remain unqualified:

```css
color: var(--color-primary);
```

If a module is uninstalled and another module still references one of its
tokens, Mesh emits an unresolved token warning.

## Creating A Theme

New themes should be created from the base shell defaults package, then edited
as authored active theme packages.

The intended workflow is:

1. copy the base shell defaults package
2. change root variables in `theme.css`
3. change semantic default rules in `theme.css`
4. select the new theme in settings
5. let Mesh install frontend module contributions into explicit module scopes

This gives users a stable recovery path: if a theme becomes inconsistent, they
can create a fresh theme from the base defaults template and reapply changes.

## Mode switching

`set_mode(name)` changes the active mode for the current theme without
changing the theme itself. Typical use: tying mode to a system dark/light
preference, or scheduling dark mode at night.

In v1, frontend module theme contributions are not mode-specific. The same
module-scoped contribution remains installed regardless of the selected theme
mode.

Mode is part of `~/.mesh/settings.json` (see [`../settings/README.md`](../settings/README.md)):

```json
{
  "theme": {
    "active": "@mesh/default-theme",
    "mode":   "dark"
  }
}
```

Switching theme is the same key. `"active"` points at a different authored
theme package and emits `ThemeChanged`. Switching mode emits `ThemeChanged`
with the same theme ID and a new mode.

## Per-Component Overrides

A module may request component-scoped overrides in its `<style>` block
without editing the global theme:

```css
.my-widget {
  --color-primary: var(--color-tertiary);   /* override just inside this class */
  color: var(--color-on-primary);
}
```

Overrides cascade through the component tree like CSS variables. The
registry-level `mesh.theme.token(name)` call still returns the theme's value;
only the style scope is affected.

## Theme Storage

User-authored active theme packages live under `~/.mesh/themes/`. The repo
fallback for development remains `config/themes/`.

MESH also keeps a separate base shell defaults package that acts as the template
for new themes and as the stable recovery point if a user wants to reset a
broken theme to known-good defaults.

## Hot-swap

Changing theme is hot. The active implementation is replaced in the
registry, `ThemeChanged` fires, every surface re-resolves its `var(--...)`
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
- The authored active theme package is the runtime source of truth.
- A separate base shell defaults package is the template and recovery point.
- Root token values live in `:root`; root component defaults live under
  semantic selectors such as `node` and `button`.
- Frontend module theme contributions are stored under explicit module scopes.
- Modes (dark/light/high-contrast) are inside a single theme, selectable at runtime.
- `node` is the inheritance root for all elements.
- Components reference `var(--name)`; cross-module token reads use explicit
  module syntax such as `var(--weather-color-sunny)`.
- Icon style and icon size tokens live in the same namespace, so themes can
  tune the icon look without shipping a new icon pack.
