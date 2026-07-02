# 05 — Icons

> Part of the [MESH Specification](README.md).

Icons are **semantic names first**. Templates write logical names
(`<icon name="audio-volume-high"/>`); **icon-pack modules** map those names to
assets already installed on the system (XDG themes, font glyphs, files). MESH
never ships, installs, or fetches icon assets — discovery is purely local.

This part unifies the previous `icon-system.md` and `theming/icons.md` and is
the single icon contract. **`config/icons.toml` is deleted** — pack manifests
plus the settings store replace it.

## 1. Three layers

```
System asset             Icon-pack module            Frontend module
(user/distro installs)   (mapping only, no assets)   (logical names only)

Material Symbols font  → @mesh/icons-material      → <icon name="audio-volume-high"/>
~/.local/share/icons/    maps "audio-volume-high"
  Papirus/…               → material-symbols/volume_up
```

- **Layer 1 — system assets**: XDG icon themes (`~/.icons`,
  `$XDG_DATA_HOME/icons`, `$XDG_DATA_DIRS/icons`, `/usr/share/pixmaps`),
  fonts via fontconfig, or plain SVG/PNG files.
- **Layer 2 — icon-pack modules**: translate the logical vocabulary into
  asset-specific names. Multiple packs stay active at once as an ordered
  fallback chain (the deliberate divergence from single-active themes).
- **Layer 3 — frontends**: declare requirements, render by logical name,
  never pick the user's visual style.

## 2. Logical names & the vocabulary index

**Status: shipped in outline** (vocabulary/requirement diagnostics exist);
the fd.o seeding rule below is the contract.

All template usage is kebab-case logical names — never codepoints, paths, or
pack-specific ids. Expected names come from three owners, composed into a
local **vocabulary index** (data from the installed graph; no central
registry):

1. **Shell vocabulary** — core names (`settings`, `close`, `search`,
   `warning`, `missing-icon`). **Seeded from the freedesktop Icon Naming
   Specification**: where fd.o defines a name (`audio-volume-high`,
   `network-wireless`, `battery-full`), MESH uses it verbatim. Invented names
   are reserved for concepts fd.o lacks. This makes every installed XDG theme
   a near-zero-effort pack (§4.1).
2. **Interface vocabularies** — domain names owned by interface modules
   (`@mesh/audio-interface` owns `audio-volume-*`), co-locating the icon
   contract with the service contract.
3. **Module vocabularies** — frontend-declared names for module-specific
   concepts (`weather-rain-heavy`), qualified in diagnostics as
   `@community/weather:weather-rain-heavy`.

## 3. Manifest shapes (canonical)

One shape; the older `dependencies.icon_packs` / `contributes.icon_vocabulary`
spellings are deleted.

**Frontend (consumer):**

```json
"mesh": {
  "uses": {
    "resources": { "icons": ["@mesh/icons-material", "@mesh/icons-lucide"] },
    "iconRequirements": {
      "required": ["settings", "audio-volume-high", "audio-volume-muted"],
      "optional": ["audio-device-headphones"]
    }
  },
  "icons": { "overrides": { "settings": "lucide/settings" } }
}
```

- `uses.resources.icons` — ordered pack chain the module prefers; first pack
  defining a name wins, later packs are fallbacks.
- `uses.iconRequirements.required` — names that must resolve; misses render
  the built-in missing glyph and report `missing_required_icon`.
- `uses.iconRequirements.optional` — used when available; misses report
  `missing_optional_icon` at lower severity.
- `icons.overrides` — author escape hatch pinning one name to a
  pack-qualified target ("this brand mark is always this glyph").
- Names declared beyond interface vocabularies are implicitly the module's
  own vocabulary; no separate declaration block.

**Icon pack:**

```json
{
  "name": "@mesh/icons-material",
  "version": "1.0.0",
  "mesh": { "apiVersion": "0.1", "kind": "icon-pack" },
  "icon_pack": {
    "id": "material",
    "kind": "font",
    "covers": { "@mesh/audio-interface": ">=1.0", "mesh.shell": ">=1.0" },
    "requires": { "fonts": [{ "family": "Material Symbols Rounded", "version": ">=4.0" }] },
    "axes": { "fill": true, "weight": true, "grade": true, "optical_size": true },
    "mappings": {
      "settings":           "material-symbols/settings_rounded",
      "audio-volume-high":  "material-symbols/volume_up",
      "audio-volume-muted": "material-symbols/volume_off"
    },
    "vocabularies": {
      "@community/weather": { "weather-clear": "material-symbols/sunny" }
    }
  }
}
```

- `id` — short alias for pack-qualified syntax (`material/settings`);
  collisions resolve by chain order.
- `kind` — the asset-source kind behind this pack: `xdg`, `font`, `file`.
  **Each kind is one implementation of a single Rust `IconSource` trait** in
  `mesh-core-icon`; new source kinds are new trait impls, never new manifest
  authorities. *(Status: target.)*
- `covers` — advisory vocabulary coverage; validation warns on gaps, never
  blocks startup.
- `requires` — expected system assets (fontconfig families, XDG theme names).
  Soft: absence logs a warning; resolve-time presence is the truth.
- `axes` — variable-font axes the asset supports; gates the `--icon-*`
  properties (§7). Ignored for non-font targets.
- `mappings` — flat logical name → `<asset-namespace>/<asset-name>`. No
  fallback chains *inside* a pack; cross-pack fallback is the chain's job.
  A mapping may flag `multicolor: true` targets (flags, logos) to opt out of
  tinting and axes.
- `vocabularies` — namespaced sections for module-owned names.

### 4.1 XDG packs

A pack targeting XDG themes maps 1:1 almost for free because MESH names *are*
fd.o names (§2). When a target resolves through XDG, standard lookup applies:
base-directory order, `index.theme`, theme `Inherits` chains, size
directories, `hicolor` required fallback. A pack may also point `root` at a
module-bundled XDG-layout theme directory; the files on disk are still a
normal XDG theme.

## 4. User configuration

**Status: target shape** (via the settings store, [08](08-settings.md)).

```json
{
  "shell": {
    "icons": { "packs": ["@mesh/user-icons", "@mesh/icons-material"] }
  },
  "@mesh/navigation-bar": {
    "icons": {
      "use_packs": ["@mesh/icons-lucide"],
      "overrides": {
        "audio-volume-high": "material/audio-volume-high",
        "settings": "~/icons/settings.svg"
      }
    }
  }
}
```

- `shell.icons.packs` — the user's shell-wide pack chain, prepended to every
  frontend's declared chain. A frontend may opt out with
  `icons.ignore_shell_default: true`.
- `<module>.icons.use_packs` — replaces the module's declared chain for that
  module only.
- `<module>.icons.overrides` — strongest user knob; per-icon, prepended ahead
  of everything.

### 4.2 The user icon-pack module

**Status: target.** User icon customization *is* an auto-managed icon-pack
module at `~/.local/share/mesh/user-icons/`, always first in the shell chain.
When the settings UI's icon picker assigns a custom file or glyph, the shell
writes a mapping into that pack — no special-case override machinery beyond
the normal chain, and the user's customizations are a portable, inspectable
module.

## 5. Resolution order

For `<icon name="X"/>` inside module `<id>` (steps 1–6 shipped in outline;
step 5 is **target**):

1. **User per-module override** — `<id>.icons.overrides.X`.
2. **Author override** — manifest `icons.overrides.X`.
3. **Pack-qualified name** — `X` = `pack/name` resolves directly through that
   pack (bypasses chains; use sparingly).
4. **Effective chain** — user shell chain (unless suppressed) + module chain
   (or `use_packs`), in order; first pack whose mapping resolves to a real
   asset wins.
5. **Dash-segment generalization** — freedesktop fallback semantics: strip
   trailing `-segment`s and retry the whole chain with each shorter name
   (`network-wireless-signal-weak` → `network-wireless-signal` →
   `network-wireless` → `network`). A pack covering only base names still
   renders something semantically close for long-tail requests.
6. **System hicolor fallback** — bare-name XDG lookup.
7. **Built-in missing-icon glyph** — embedded SVG; always renders; one
   warning per `(module, name)`.

Rendered size is always the layout box; the `size` attribute is only an XDG
resolution hint.

## 6. Color

Icons inherit `color` from their style context like text (`currentColor`
semantics): monochrome assets are recolored; `multicolor` targets skip
recoloring and axis application. There is no `icon.color` token — tint the
parent.

## 7. Variable axes & style tokens

Theme tokens control icon look without shipping new packs
([04 §6](04-styling.md)):

| CSS property | Axis | Range |
| ------------ | ---- | ----- |
| `--icon-fill` | `FILL` | 0.0–1.0 |
| `--icon-weight` | `wght` | 100–700 |
| `--icon-grade` | `GRAD` | −25–200 |
| `--icon-optical-size` | `opsz` | 20–48 |

```css
icon { --icon-fill: 0; transition: --icon-fill 150ms ease-out; }
icon:active { --icon-fill: 1; }
```

The pack's `axes` declaration gates effect; unsupported axes are silent
no-ops (no fake animation). Size tokens (`--icon-size-sm/md/lg`) are normal
theme tokens.

## 8. Caching

Resolver caches: resolution per `(module, name, chain)`; decoded raster per
file path; glyph raster per `(font, codepoint, size, color, axes)`. Flushed
on pack load/reload/remove, icon settings change, and theme change.

## 9. Tooling

```
mesh icons vocabulary list / show <owner>   # expected names per vocabulary owner
mesh icons validate-pack <id>               # covers/mappings vs claimed vocabularies
mesh icons missing <module>                 # unresolved required/optional names
mesh icons resolve <name> --module <id>     # print the full resolution chain
mesh icons scaffold-pack --from-xdg-theme <name> | --from-font <family>
```

**`scaffold-pack` (target)** attacks the coverage problem at its root —
authoring cost: it generates a pack manifest with best-guess mappings
(fuzzy-matching the vocabulary index against the asset's glyph/icon names)
plus a TODO list of unmapped required names. The diagnostics panel surfaces
the same data: active chain, per-module miss counts, which pack serves each
name.
