# Icons

Icons in MESH are **extensible the same way everything else is**: they are
resolved by semantic name against an interface, implementations (icon packs)
are ordinary plugins, and rendering parameters come from theme tokens. Nothing
about the icon system is baked into the core.

This design follows Material 3's icon model: icons are symbolic vectors keyed
by name, rendered with variable axes (weight, fill, grade, optical size), and
colored by the surrounding role context rather than per-asset color.

## Model

1. **Icons are referenced by name.** Markup never points at a file:

   ```xml
   <icon name="network-wifi"/>
   ```

2. **Icon packs are plugins** with `type = "icon-pack"`. Each pack resolves a
   set of names to vectors. Multiple packs can be installed.

3. **Rendering parameters are tokens.** Colour, size, and the variable axes
   live in the theme, not in the icon call site.

4. **Color is inherited.** An `<icon>` picks up `color` from its ancestor
   (like text does with `currentColor`). Tinting a whole area tints the
   icons inside it for free — no per-state assets.

5. **The icon resolver is an interface contract.** Surfaces consume icons
   via the registry, and icon packs implement the same contract — so icons
   slot into the extensibility story described in
   [`../extensibility.md`](../extensibility.md).

## The icon contract

Icon packs implement `@mesh/icon-contract`:

```
interface: mesh.icons
version:   1.0
methods:
  resolve(name: string, variant: Variant) -> Vector?
  has(name: string) -> bool
  list() -> [string]
events:
  PackReloaded()
types:
  Variant { weight: int, fill: float, grade: int, optical: int, style: string }
  Vector  { kind: "svg" | "font-glyph", payload: bytes, multicolor: bool }
```

A pack declares itself with:

```toml
[package]
id   = "@lucide/icons"
type = "icon-pack"

[service]
provides = "mesh.icons"
priority = 50          # fallback chain ordering, lower = later
```

Multiple icon packs are **active simultaneously** — this is the one place the
registry diverges from the single-active-backend rule used elsewhere. The
registry treats `mesh.icons` providers as an ordered chain rather than a
winner-takes-all.

## Resolution rules

### Unqualified names — fallback chain

`<icon name="wifi"/>` walks the active chain. The chain is composed from:

1. Packs pinned by the user in `~/.config/mesh/config.toml`:

   ```toml
   [icons]
   chain = ["@user/symbols", "@mesh/symbols", "@lucide/icons"]
   ```

2. Otherwise the packs registered as `mesh.icons` providers, ordered by
   descending `priority`.

First pack whose `has(name)` returns true wins. Miss → placeholder glyph +
diagnostic.

### Qualified names — explicit pack

A `@scope/pack:name` prefix bypasses the chain:

```xml
<icon name="@mesh/symbols:wifi"/>
<icon name="@lucide/icons:wifi-high"/>
<icon name="@flags/country:sk"/>
```

If the named pack is not installed, or does not have the name, the icon
fails visibly (placeholder + diagnostic) rather than falling through. This
keeps intentional choices intentional.

### Scoped default

For plugins that mostly use one non-default pack, a subtree can set a
default:

```xml
<row icon-pack="@lucide/icons">
  <icon name="wifi-high"/>            <!-- resolves against @lucide/icons -->
  <icon name="battery-80"/>
  <icon name="@flags/country:sk"/>    <!-- explicit prefix still wins -->
</row>
```

Precedence (highest first):

1. Explicit `@scope/pack:` prefix on the call site
2. Nearest ancestor `icon-pack="…"` attribute
3. User's configured fallback chain

### Aliases

Packs may declare aliases (`audio-muted → volume-off`) so UIs written against
one pack keep working on another. Aliases are **pack-local**: they resolve
inside the pack that owns them, never across packs. Predictability over
cleverness.

## Declaring icon-pack dependencies

A plugin that uses specific packs lists them in its manifest so the installer
can guarantee they are present:

```toml
[icon-packs]
required = ["@mesh/symbols", "@flags/country"]
optional = ["@lucide/icons"]
```

- **Required** — installed (or pulled in) before the plugin loads. If
  unavailable, the plugin refuses to load with a clear diagnostic.
- **Optional** — used when present, absent gracefully. The plugin is
  expected to degrade (e.g. fall back to text, or use an unqualified name
  that another pack can satisfy).

Plugins that use only unqualified names don't need to declare anything — the
user's chain is the contract.

## Rendering tokens

Icon appearance is token-driven. These sit alongside the existing theme
tokens (`color.*`, `spacing.*`, `typography.*`, …):

| Token | Type | Purpose |
|-------|------|---------|
| `icon.size.sm` | pixel | Small icons (status indicators) |
| `icon.size.md` | pixel | Default size (buttons, list items) |
| `icon.size.lg` | pixel | Large icons (hero affordances) |
| `icon.weight` | int 100..700 | Stroke weight for variable fonts |
| `icon.fill` | float 0..1 | Outlined (0) vs filled (1) |
| `icon.grade` | int -25..200 | Emphasis tuning (M3 grade axis) |
| `icon.optical` | int 20..48 | Optical size; `auto` by default (derived from `size`) |
| `icon.style` | enum | `"outlined" \| "rounded" \| "sharp"` |

Call sites override per-instance when needed:

```xml
<icon name="star" style="fill: 1; size: token(icon.size.lg)"/>
```

### Color

There is no `icon.color` token. Icons inherit `color` from their ancestor,
which itself comes from the surrounding role (`color.on-surface`,
`color.primary`, `color.error`, …). To recolor an icon, set the color on the
parent — the same way you would for text.

```css
.danger {
  color: token(color.error);
}
```

```xml
<row class="danger">
  <icon name="warning"/>      <!-- picks up color.error -->
  <text>Disk almost full</text>
</row>
```

This means the same pack asset serves every color context. No per-state
duplicates.

## Multicolor icons

Some icons are intentionally multicolored — flags, emoji, app logos. These
must opt out of tinting so the inherited `color` doesn't flatten them.

The `Vector` returned from `resolve(...)` carries a `multicolor: bool` flag.
When true, the renderer ignores `color` inheritance. Pack authors set this
per-icon in the pack's manifest:

```toml
[[icons]]
name        = "sk"
source      = "flags/sk.svg"
multicolor  = true
```

Variable-axis tokens (`weight`, `fill`, `grade`) are also ignored for
multicolor icons — they have no meaningful effect on a flag.

## Variable-axis support is optional

Packs that ship a variable font (or axis-aware SVGs) honour `weight`, `fill`,
`grade`, and `optical`. Simpler packs (single-glyph SVGs) just render the
asset and ignore the axis tokens. The core must tolerate both.

A pack advertises what it supports in its manifest:

```toml
[icon-pack]
axes = ["weight", "fill"]   # omitted axes are ignored silently
styles = ["outlined", "rounded"]
```

Requesting a style the pack does not have falls back to the pack's default
style rather than failing.

## Tooling

```
mesh icons list                       # every icon available across the active chain
mesh icons which wifi                 # show which pack resolved the name
mesh icons resolve @lucide/icons:wifi # dump the vector payload
mesh icons missing <plugin>           # report names a plugin requests but no pack has
```

The diagnostics panel surfaces the same info: active chain, miss counts per
plugin, which pack is currently serving each name.

## Defaults

MESH ships `@mesh/symbols` as the default pack at priority 100. It is an
ordinary plugin — users can replace it, disable it, or re-order the chain
without touching the core.

## Summary

- Icons are named, not pathed.
- Icon packs are ordinary plugins implementing `mesh.icons`.
- Multiple packs coexist via an ordered fallback chain (the only
  multi-active interface in MESH).
- Names can be explicit (`@pack:name`), scoped by ancestor, or unqualified.
- Rendering parameters are theme tokens; color is inherited from context.
- Plugins declare required/optional packs in their manifest.
- Multicolor icons opt out of tinting per asset.
