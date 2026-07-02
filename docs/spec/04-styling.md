# 04 — Styling & Theming

> Part of the [MESH Specification](README.md).

A theme is a **module with metadata plus CSS**. Scalar design values (color,
typography, spacing, radius, elevation, animation, icon style) are CSS custom
properties in `theme.css`; default styles for primitives are semantic CSS
rules. The runtime compiles that CSS into the token and component-default
model the renderer uses.

## 1. Model

1. **CSS variables are the tokens.** Themes author `--color-primary`;
   components reference `var(--color-primary)`. Token groups are prefixes
   (`color`, `spacing`, `typography`, `radius`, `elevation`, `animation`,
   `shadow`, `icon`, `font`); themes may introduce new groups freely — the
   core reserves no names, and unknown-token reads return `nil`.
2. **One theme active at a time** (winner-takes-all — mixed token sets
   produce incoherent UI). Modes (dark / light / high-contrast) live *inside*
   a theme and are user-switchable at runtime. This is the deliberate
   opposite of icon/font/language packs, which are ordered multi-active
   chains.
3. **Themes are modules** (`mesh.kind: "theme"`), hot-swappable, with no
   privileged default.
4. **`node` is the inheritance root.** Every element starts from
   `ComputedStyle::default()`, inherits the theme's `node` rule, then its
   tag rule (`button`), then module-scoped defaults, then local styles.
5. **Module theme contributions are scoped.** A frontend's tokens/defaults
   apply only inside its own rendered subtree.

## 2. Theme pack shape

**Status: shipped** (CSS theme loading, modes, tokens); manifest fields per
[01 §3](01-module-system.md).

```json
{
  "name": "@alice/theme",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "theme",
    "provides": {
      "themes": [{
        "id": "alice", "label": "Alice", "default_mode": "dark",
        "modes": { "dark": "themes/dark/theme.css", "light": "themes/light/theme.css" }
      }]
    }
  }
}
```

```css
/* theme.css — tokens in :root, defaults in semantic rules */
:root {
  --color-primary: #7A5AF8;
  --color-surface: #121217;
  --spacing-md: 8;
  --radius-md: 8;
  --animation-duration-fast: 90;
  --animation-curves-bezier-standard: cubic-bezier(0.2, 0.0, 0, 1.0);
}

node   { color: var(--color-on-surface);
         transition: color var(--animation-duration-fast) var(--animation-curves-bezier-standard); }
button { background: var(--color-primary); border-radius: var(--radius-md); }
button:hover { background: var(--color-primary-container); }
```

Theme CSS supports a restricted selector surface: `:root`, `node`, semantic
element selectors, and supported pseudo-states. Module-specific selectors
belong in module scopes (§4), not global theme rules. Within `animation`,
primitive tokens (durations, curves) and recipe tokens (full transition
contracts composed via explicit `var(--…)`) stay separate.

## 3. The load-time cascade

**Status: target** (this replaces the old write-on-install model — Mesh no
longer writes module contributions into the user's theme file; any code doing
so is deleted).

The **effective theme is composed in memory at load** from three layers:

```
1. active theme pack CSS  (per active mode)
2. module mesh.theme contributions  (module-scoped, from installed manifests)
3. user token overrides  (from the settings store, 08)
```

- No compiled cache artifact; recomposition happens on theme/mode switch,
  module (un)install, and settings change, then `ThemeChanged` fires and
  surfaces re-resolve `var(--…)` on the next frame. No module code runs
  during a swap.
- Uninstalling a module simply drops its layer; remaining references to its
  tokens become unresolved-token warnings.
- The base shell defaults pack is the template and recovery point: creating a
  theme = copy base, edit `theme.css`, select in settings.

Per-node resolution order (unchanged, shipped):

```
ComputedStyle::default()
→ base shell defaults → active theme node → active theme tag rule
→ module-scope node → module-scope tag rule
→ local stylesheet rules → pseudo-state rules
```

## 4. Module theme contributions (`mesh.theme`)

**Status: shipped shape; storage moves to the load-time cascade (§3).**

Frontend modules may declare module-owned tokens and subtree-scoped component
defaults:

```json
"theme": {
  "tokens": { "weather.color.sunny": "#F6B73C" },
  "defaults": {
    "components": {
      "base":         { "transition": "background-color var(--animation-duration-fast) var(--animation-curves-bezier-standard)" },
      "button":       { "border-radius": "var(--radius-md)" },
      "weather-chip": { "background": "var(--weather-color-sunny)" }
    }
  }
}
```

- `base` maps to the module-scoped `node` rule; tag keys override core
  primitives inside that module's subtree only; custom keys are module-local
  component defaults.
- Invalid token names, invalid properties, or unresolved `var(--…)`
  references are install/load diagnostics.
- Contributions are not theme-variant-specific in v1.
- Module tokens are referenced with their prefixed name
  (`var(--weather-color-sunny)`); root tokens stay unqualified.

## 5. User customization

Through the settings store ([08](08-settings.md)), generated UI, and CLI:

```json
"shell": {
  "theme": {
    "active": "@alice/theme",
    "mode": "dark",
    "tokens": { "color-primary": "#FF6B00" }
  }
}
```

`tokens` is the sparse user override layer of the cascade (§3). Per-component
overrides stay in component styles (`.my-widget { --color-primary:
var(--color-tertiary); }`) and cascade like CSS variables; the registry-level
token read still returns the theme value.

## 6. Consuming tokens

- **In `<style>`**: `var(--…)` — the only styling path components should use
  for design values.
- **From script**: the `mesh.theme` interface —
  `token(name)`, `tokens(group)`, `modes()`, `active_mode()`,
  `set_mode(name)`; events `ThemeChanged(theme_id, mode)`,
  `TokenChanged(name, value)`.
- **Props**: a `token`-typed prop exposes a controlled, theme-aware knob
  ([03 §3.2](03-components.md)).
- Icon style (axes, sizes) and font roles are theme tokens too
  (`--icon-*`, `--font-*`) — see [05 §7](05-icons.md), [06 §3](06-fonts.md).

## 7. Theme coherence enforcement

**Status: target.** Token-based theming only works ecosystem-wide if modules
actually use tokens. The installed-graph source scan flags color literals
outside `var(--…)`/`prop(…)` in component `<style>` blocks as
`hardcoded_color_in_component_style` (warn severity, LSP + `mesh doctor`).
Escape hatch: a `/* mesh-allow-literal */` trailing comment for genuinely
fixed colors (brand marks). The inverse guard — unresolved token references —
already exists.

## 8. Tooling

```
mesh themes list                  # installed theme packs + modes
mesh themes active                # current theme + mode
mesh themes set <id> [--mode m]
mesh themes tokens [group]        # dump effective (composed) tokens
mesh themes which <token-name>    # which cascade layer supplied the value
```
