# Icons

Icons in MESH are **FreeDesktop/XDG icon themes first**. Markup references
semantic icon names, and MESH resolves those names through XDG-compatible icon
themes (`index.theme`, theme inheritance, size directories, and the standard
base-directory search order) before falling back to bundled compatibility
icons.

This keeps MESH compatible with distro-installed icon themes such as hicolor,
Adwaita, Breeze, Papirus, or a user's custom `~/.icons` theme. Theme tokens
still control rendered size and color, but icon discovery follows the
FreeDesktop Icon Theme Specification rather than a MESH-only pack format.

## Model

1. **Icons are referenced by name.** Markup never points at a file:

   ```xml
   <icon name="network-wifi"/>
   ```

2. **Icon packs are XDG icon themes.** MESH resource-pack modules contribute
   or select FreeDesktop-compatible theme directories, not custom asset maps.

3. **Rendering parameters are tokens.** Colour, size, and the variable axes
   live in the theme, not in the icon call site.

4. **Color is inherited.** An `<icon>` picks up `color` from its ancestor
   (like text does with `currentColor`). Tinting a whole area tints the
   icons inside it for free — no per-state assets.

5. **Icon resolver aliasing is profile data.** MESH may map its stable UI
   names to several XDG names, but the resolved files still come from XDG
   themes. Icon resolver aliases are resource lookup rules, not vocabulary compatibility aliases.

## XDG Theme Contract

An icon-pack module is valid when it exposes a FreeDesktop icon theme. The
theme directory must contain `index.theme`, and every listed directory must
have a matching section in that file.

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

MESH relies on the standard lookup behavior: `$HOME/.icons`,
`$XDG_DATA_HOME/icons`, `$XDG_DATA_DIRS/icons`, additional module-provided base
directories, and `/usr/share/pixmaps`, with `hicolor` as the required fallback
theme.

## Resolution rules

### Unqualified Names

`<icon name="network-wireless"/>` resolves by walking the configured XDG theme
chain. The current MVP config uses semantic aliases so a Mesh-facing name can
try several standard names:

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

### Icon resolver aliasing

Icon resolver aliasing is MESH profile data, not custom icon-pack behavior and
not old-name vocabulary compatibility. It translates a stable MESH UI name to
one or more XDG icon names, then the normal XDG resolver does the rest.

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

Modules that use only common XDG names do not need to declare a specific pack;
the user's selected XDG theme and hicolor fallback are the contract.

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
- Icon packs are XDG-compatible icon themes, not a MESH-only asset format.
- Theme lookup follows FreeDesktop base directories, `index.theme`,
  inheritance, and hicolor fallback.
- MESH semantic aliases map stable UI names onto standard XDG icon names.
- Rendering parameters are theme tokens; color is inherited from context.
- Modules declare required/optional packs in their manifest.
- Multicolor icons opt out of tinting per asset.
