# Icons

Icons in MESH are **semantic names first**. Markup references stable MESH
names, and icon-pack modules map those names to real assets from XDG themes,
font glyphs, or files. XDG-compatible icon themes remain the preferred system
asset source (`index.theme`, theme inheritance, size directories, and the
standard base-directory search order), but pack mappings are the compatibility
layer that lets Material Symbols, Lucide, Adwaita, Papirus, or a custom theme
serve the same frontend markup.

This keeps MESH compatible with distro-installed icon themes such as hicolor,
Adwaita, Breeze, Papirus, or a user's custom `~/.icons` theme. Theme tokens
still control rendered size and color, but icon discovery follows the
FreeDesktop Icon Theme Specification whenever the selected mapping targets an
XDG theme.

## Model

1. **Icons are referenced by name.** Markup never points at a file:

   ```xml
   <icon name="network-wifi"/>
   ```

2. **Icon packs map semantic names.** MESH resource-pack modules translate
   shell/interface/module icon names to assets. Those assets may be XDG theme
   entries, font glyphs, or files.

3. **Rendering parameters are tokens.** Colour, size, and the variable axes
   live in the theme, not in the icon call site.

4. **Color is inherited.** An `<icon>` picks up `color` from its ancestor
   (like text does with `currentColor`). Tinting a whole area tints the
   icons inside it for free — no per-state assets.

5. **Icon vocabulary is contract data.** Shared names live in the shell and
   interface modules; frontend modules may declare module-owned names for
   special concepts. Icon-pack mappings target those declared names.

For the full vocabulary, mapping, settings, and resolution contract, see
[`../icon-system.md`](../icon-system.md).

## XDG Theme Contract

An icon-pack module may expose or target a FreeDesktop icon theme. When it
does, the theme directory must contain `index.theme`, and every listed
directory must have a matching section in that file.

```text
@mesh/symbols/
  module.json
  icons/
    mesh-symbols/
      index.theme
      scalable/
        actions/
          settings.svg
        status/
          audio-volume-high.svg
          network-wireless.svg
```

Minimal `index.theme`:

```ini
[Icon Theme]
Name=MESH Symbols
Comment=MESH fallback symbols
Inherits=hicolor
Directories=scalable/actions,scalable/status

[scalable/actions]
Size=16
Type=Scalable
MinSize=1
MaxSize=256
Context=Actions

[scalable/status]
Size=16
Type=Scalable
MinSize=1
MaxSize=256
Context=Status
```

When a mapping target resolves through XDG, MESH relies on the standard lookup
behavior: `$HOME/.icons`,
`$XDG_DATA_HOME/icons`, `$XDG_DATA_DIRS/icons`, additional module-provided base
directories, and `/usr/share/pixmaps`, with `hicolor` as the required fallback
theme.

## Resolution rules

### Unqualified Names

`<icon name="network-wireless"/>` resolves through the active icon-pack chain.
An XDG-backed pack can map that semantic name to one or more standard XDG
names:

```toml
active_profile = "material"

[[packs]]
id = "system"
theme = "hicolor"

[[packs]]
id = "mesh-symbols"
root = "/usr/share/mesh/icons"
theme = "mesh-symbols"

[profiles.material.icons]
network-wireless = [
  "system:network-wireless",
  "system:network-wireless-symbolic",
  "mesh-symbols:network-wireless"
]
```

The `system` pack has no `root`, so it searches standard XDG icon locations.
Module-provided packs set `root` to a base directory containing one or more
theme directories.

### Qualified Names

A `pack-id:icon-name` candidate inside `config/icons.toml` chooses a specific
configured XDG theme. Template markup should still use unqualified semantic
names unless a surface intentionally requires a particular icon theme.

Misses produce a visible placeholder and a diagnostic listing every attempted
XDG name.

### Icon resolver mapping

Icon resolver mapping translates a stable MESH UI name to one or more concrete
asset names, then the target resolver does the rest. When the target is XDG,
the normal XDG resolver handles theme inheritance and hicolor fallback.

## Declaring icon-pack dependencies

A module that truly needs a bundled XDG theme lists the icon-pack module in its
manifest so the installer can guarantee it is present:

```toml
[icon-packs]
required = ["@mesh/symbols", "@flags/country"]
optional = ["@lucide/icons"]
```

- **Required** — installed (or pulled in) before the module loads. If
  unavailable, the module refuses to load with a clear diagnostic.
- **Optional** — used when present, absent gracefully. The module is
  expected to degrade (e.g. fall back to text, or use an unqualified name
  that another pack can satisfy).

Modules should declare the semantic icon names they use through `uses.icons`
even when those names are common. They do not need to require a specific pack
unless the UI depends on assets outside the shared shell/interface
vocabularies.

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

MESH tracks a `multicolor` flag on configured icon candidates. When true, the
renderer ignores color inheritance. Pack authors should prefer normal XDG
categories such as `apps`, `places`, or `mimetypes`; MESH-specific metadata is
only for renderer hints that the XDG spec does not define.

```toml
[profiles.material.icons]
country-sk = ["system:flag-sk?multicolor", "mesh-flags:sk?multicolor"]
```

Variable-axis tokens (`weight`, `fill`, `grade`) are also ignored for
multicolor icons — they have no meaningful effect on a flag.

## Variable-axis support is optional

Packs that ship a variable font (or axis-aware SVGs) honour `weight`, `fill`,
`grade`, and `optical`. Simpler packs (single-glyph SVGs) just render the
asset and ignore the axis tokens. The core must tolerate both.

A pack may advertise optional renderer hints in its manifest, but the files on
disk are still an XDG icon theme:

```toml
[icon-pack]
axes = ["weight", "fill"]   # omitted axes are ignored silently
styles = ["outlined", "rounded"]
```

Requesting a style the pack does not have falls back to the pack's default
style rather than failing.

## Tooling

```
mesh icons list                     # every icon available through configured themes
mesh icons which network-wireless   # show which XDG theme resolved the name
mesh icons resolve network-wireless # print resolved path and candidate chain
mesh icons missing <module>         # report names a module requests but no theme has
```

The diagnostics panel surfaces the same info: active chain, miss counts per
module, which pack is currently serving each name.

## Defaults

MESH first searches the configured system XDG theme. It ships a small bundled
fallback theme for core shell icons so the shell remains usable on minimal
systems. That fallback should also be laid out as an XDG icon theme.

## Summary

- Icons are named, not pathed.
- Icon packs map MESH semantic names to concrete assets.
- XDG-backed targets follow FreeDesktop base directories, `index.theme`,
  inheritance, and hicolor fallback.
- Shell and interface modules define shared expected names; frontend modules
  may declare module-owned special names.
- Rendering parameters are theme tokens; color is inherited from context.
- Modules declare required/optional packs in their manifest.
- Multicolor icons opt out of tinting per asset.
