# Icon System

MESH uses a binding-based icon system. Frontends write semantic logical
names; **icon-pack modules** map those names to assets installed on the
system. Asset installation is deliberately outside MESH's scope (XDG icon
themes, fonts via fontconfig, etc. — handled by the user, distro, or a
setup script), so MESH never ships icons itself.

This document is the design contract; implementation follows it.

---

## Goals

- Single semantic API in templates: `<icon name="volume-high"/>`
- A clean three-layer model: system asset → MESH binding (icon-pack) → frontend
- Frontends are portable: swap the icon-pack and the look changes, no
  template edits
- Pack-qualified escape hatch when the author or user wants a specific
  source for one icon
- Per-frontend user overrides without rebuilding the icon-pack
- CSS-driven animations on packs whose underlying asset supports them
  (variable-font axes for font-based packs)
- No duplicate installation paths — assets live in standard system
  locations only

## Non-goals

- MESH does not ship, install, or manage icon assets. That is the user's
  job (or a separate install script).
- No automatic asset downloading, registry, or update mechanism.
- Hard version gating against installed assets — version constraints in
  packs are advisory only (logged warnings, never blockers).

---

## The three layers

```
System asset            MESH icon-pack module        Frontend module
(installed by user)     (mapping only, no assets)    (uses logical names)

material-symbols.ttf  → @mesh/icons-material-r1   → @mesh/navigation-bar
~/.local/share/icons    pack: maps "home" →           <icon name="home"/>
  /Adwaita/...           material-symbols/home_round
```

### Layer 1 — System asset

Files on disk in standard locations:

- XDG icon themes under `~/.local/share/icons/`, `/usr/share/icons/`
- Fonts under `~/.local/share/fonts/`, `/usr/share/fonts/` (discovered
  via fontconfig)
- Single SVG/PNG files in arbitrary locations the icon-pack points to

Assets are installed **outside MESH** — `apt`, `pacman`, AUR helpers, a
bash setup script, manual download into XDG paths. MESH discovers what
is already installed; it never installs.

### Layer 2 — Icon-pack module

A MESH module of kind `icon-pack`. Contains **only** a mapping table
plus metadata about the system assets it expects to find. Ships **no**
icons.

The job of the icon-pack is to translate between two name vocabularies:
the logical names a frontend wants (`home`, `volume-high`) and the
asset-specific names the system has (`home_rounded`, `audio-volume-high`,
codepoint ``, etc.).

Multiple icon-pack modules can wrap the same underlying assets with
different conventions — e.g. `@mesh/icons-material-rounded` and
`@mesh/icons-material-flat` both map to Material Symbols glyphs but
expose different sets of logical names or pick different glyph variants.

### Layer 3 — Frontend module

Declares which icon-pack(s) it depends on. Writes `<icon name="..."/>`
with logical names only. Optionally declares per-icon overrides for
cases where the author wants to deviate from the active icon-pack's
chosen glyph for one specific icon.

---

## Logical names

All template usage is by **kebab-case logical name**:

```xml
<icon name="volume-high"/>
<icon name="wifi-off"/>
```

Logical names are never raw codepoints, file paths, or pack-specific
identifiers in the common case.

The canonical vocabulary for a given service is owned by the
**interface module** for that service. The audio interface declares
names like `volume-high`, `volume-mute`, `volume-medium`; any frontend
consuming the audio interface inherits that vocabulary. This
co-locates the icon contract with the service contract.

Modules are free to use additional names beyond the interface
vocabulary, but should prefer interface-declared names where they
exist.

### Pack-qualified escape hatch

For the cases where you, the author, want to pin one specific glyph
from a specific pack regardless of the active mapping, use the
`<pack-id>/<logical-name>` form:

```xml
<icon name="lucide/home"/>
<icon name="material-rounded/settings"/>
```

The slash separates the icon-pack module id (or its short alias) from
the logical name resolved through that pack. This bypasses the active
default and the dependency chain — it always resolves through the
named pack.

Use sparingly. The whole point of the binding system is that
templates don't bake in pack choices. Pack-qualified names are an
escape hatch for the rare cases where one specific glyph really does
matter (e.g. a brand mark, a custom logo).

---

## Icon-pack module shape

Pack manifest (`package.json` with `mesh.kind = "icon-pack"`):

```json
{
  "name": "@mesh/icons-material-rounded",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "icon-pack"
  },
  "icon_pack": {
    "id": "material-rounded",
    "requires": {
      "fonts": [
        { "family": "Material Symbols Rounded", "version": ">=4.0" }
      ]
    },
    "axes": {
      "fill": true,
      "weight": true,
      "grade": true,
      "optical_size": true
    },
    "mappings": {
      "home":         "material-symbols/home_rounded",
      "settings":     "material-symbols/settings_rounded",
      "volume-high":  "material-symbols/volume_up",
      "volume-mute":  "material-symbols/volume_off"
    }
  }
}
```

### Field reference

- **`id`** — short alias used in pack-qualified template syntax
  (`material-rounded/home`). Should be globally unique; collisions are
  resolved by frontend dependency declaration order.
- **`requires`** — declares system assets the pack expects. Currently
  supports `fonts` (matched via fontconfig) and `themes` (matched
  against installed XDG theme names). Version strings are **soft**:
  failure to find or a too-old version logs a warning, never blocks
  loading. The actual presence of a referenced asset at icon-resolve
  time is the source of truth.
- **`axes`** — variable-font axes the underlying asset supports. Used
  by the painter to gate CSS `--icon-*` custom properties; unsupported
  axes silently no-op. Ignored entirely for non-font assets.
- **`mappings`** — flat 1:1 map from logical name → asset reference.
  The right-hand side is `<asset-pack>/<asset-name>` where
  `asset-pack` is an XDG theme name, a font-family alias, or a free
  asset namespace; `asset-name` is the icon name within that
  namespace (XDG name, font glyph name, etc.). No fallback chains
  inside a single pack — cross-pack fallback is handled by the
  frontend declaring multiple icon-pack dependencies.

A single icon-pack can wrap multiple system assets — `mappings`
entries can target different `<asset-pack>` prefixes freely.

---

## Frontend usage

Frontend manifest:

```json
{
  "name": "@mesh/navigation-bar",
  "mesh": {
    "kind": "frontend",
    "dependencies": {
      "icon_packs": [
        "@mesh/icons-material-rounded",
        "@mesh/icons-lucide"
      ]
    }
  },
  "icons": {
    "overrides": {
      "settings": "@mesh/icons-lucide/settings"
    }
  }
}
```

- **`dependencies.icon_packs`** — ordered list. Resolution prefers the
  first pack that defines the requested logical name; subsequent packs
  are fallbacks for names the first one doesn't define.
- **`icons.overrides`** — author-side per-icon escape hatch. Format
  matches pack-qualified template syntax. This is **not** the same as
  user-side overrides (those live in shell `settings.json`); think of
  this as the frontend author saying "I always want this specific
  glyph for this name regardless of which pack is otherwise active."

### Implicit shell-default pack

The user's chosen shell-default icon-pack is **prepended** to every
frontend's `dependencies.icon_packs` list at resolution time. This
makes the default pack the highest-priority source for any logical
name without each frontend having to opt in.

A frontend that explicitly does NOT want the shell default can declare
`icons.ignore_shell_default: true`.

---

## Shell configuration

User-side configuration in shell `settings.json`:

```json
{
  "icons": {
    "default_pack": "@mesh/icons-material-rounded"
  },
  "modules": {
    "navigation-bar": {
      "icons": {
        "use_packs": ["@mesh/icons-lucide"],
        "overrides": {
          "home": "custom/my-home.svg"
        }
      }
    }
  }
}
```

- **`icons.default_pack`** — the shell-wide preferred icon-pack,
  implicitly prepended to every frontend's dependencies (see above).
- **`modules.<id>.icons.use_packs`** — replaces the frontend's
  declared `dependencies.icon_packs` list for *this module only*.
  Useful when the user wants a different visual style for one panel
  without forking the module.
- **`modules.<id>.icons.overrides`** — per-icon override chain
  prepended in front of every other resolution path for matching
  logical names. The strongest user-side knob; use it to swap one
  icon in one place.

---

## Resolution order

For any `<icon name="X"/>` rendered inside frontend `<id>`:

1. **User override chain** — `modules.<id>.icons.overrides.X`, if
   present.
2. **Frontend author override chain** — frontend manifest's
   `icons.overrides.X`, if present.
3. **Pack-qualified template name** — if `X` is `pack/name`, resolve
   directly through `pack` and skip the dependency chain.
4. **Effective dependency chain** — the user's shell-default pack
   (unless suppressed) followed by the frontend's
   `dependencies.icon_packs` (or `modules.<id>.icons.use_packs` if
   overridden), tried in order. First pack whose `mappings` defines
   `X` and whose target resolves to a real asset wins.
5. **System hicolor fallback** — bare-name lookup in the installed
   `hicolor` XDG theme, as a last-resort cross-app convention.
6. **Built-in missing-icon glyph** — embedded SVG, always renders
   something. One-time warning per `(module_id, logical_name)` is
   logged so the dev knows.

---

## Pack-qualified target syntax

The right-hand side of a mapping entry, the override value, and the
template `<pack>/<name>` form all use the same target syntax:

```
<pack-id>/<asset-name>
```

Where `pack-id` may refer to:

- An XDG icon theme installed on the system (`hicolor`, `Adwaita`,
  `Papirus`, ...) — `asset-name` is the freedesktop icon name.
- A font family installed on the system, addressed by alias declared
  in the icon-pack's `requires.fonts` — `asset-name` is the font
  glyph's text name (looked up against the family's codepoints).
- An absolute or `~`-relative file path — `asset-name` may be empty
  for a single-file reference (`override: "/abs/path/to/icon.svg"`).
- Another icon-pack module (in user-overrides only) — `asset-name`
  is a logical name within that pack.

The renderer dispatches on what the target type turns out to be:
file targets go through the SVG/PNG raster path; font glyph targets
go through the font glyph rasterizer with optional variable-axis
settings.

---

## Animations (variable-font axes)

CSS custom properties drive variable-font axes for font-based
mappings:

- `--icon-fill` — `FILL` axis (0.0–1.0)
- `--icon-weight` — `wght` axis (typically 100–700)
- `--icon-grade` — `GRAD` axis (typically -25 to 200)
- `--icon-optical-size` — `opsz` axis (typically 20–48)

Example:

```css
icon { --icon-fill: 0; transition: --icon-fill 150ms ease-out; }
icon:active { --icon-fill: 1; }
```

The icon-pack's `axes` declaration gates which custom properties
have any effect. Unsupported axes are silent no-ops; no fake
animation is synthesized for non-supporting packs. File-based
targets (SVG/PNG) ignore axis properties entirely.

---

## Color

Icons inherit `color` from their style context (which resolves
through theme tokens). Monochrome assets (single-color SVG, font
glyphs) are recolored to the resolved color. Multicolor assets keep
their source colors and skip recoloring; mapping entries can mark
their target as `multicolor: true` if the pack knows in advance.

---

## Discovery

At shell startup MESH discovers:

1. **Installed icon-pack modules** — found via the standard module
   discovery paths (workspace, `~/.local/share/mesh/modules/`, etc.).
   Each pack's `requires` block is matched against installed system
   assets; mismatches log soft warnings.
2. **System icon themes** — XDG directories scanned to know what
   theme names are available for `hicolor:foo` style targets.
3. **System fonts** — fontconfig query for families referenced in any
   loaded icon-pack's `requires.fonts`.

There is no "MESH icon registry" or central server. Discovery is
purely local.

---

## Caching

The resolver caches:

- Resolution results per `(module_id, logical_name, active_pack_chain)`
- Decoded SVG/PNG raster per file path
- Font glyph raster per `(font, codepoint, size, color, axes)`

All three are flushed when:

- An icon-pack module is loaded, reloaded, or removed
- Shell `icons` settings change
- The active theme changes (icons may inherit different colors)

---

## Migration from in-frontend mappings

Earlier MESH allowed frontends to declare mappings inline in their
own `package.json`. That layer is **deprecated** and being removed:

- Frontend `package.json` `icons.mappings` — **drop**. Move the
  mapping entries into a dedicated icon-pack module the frontend
  depends on.
- Frontend `package.json` `icons.pack` — **drop**. The icon-pack
  module the frontend depends on is the source of truth.
- Frontend `package.json` `icons.overrides` — **keep**. Renamed for
  clarity and used as the author-side per-icon escape hatch.

User-side shell `settings.json` `modules.<id>.icons.{pack,overrides}`
from the earlier design becomes
`modules.<id>.icons.{use_packs,overrides}` (note: `use_packs` is now
a list — the user can declare a full pack chain, not just a single
preferred pack).

---

## Implementation surface

The crate that owns the resolver is `mesh-core-icon`. Touch points
for implementation:

- `mesh-core-icon`
  - Pack manifest parsing (`icon_pack` section)
  - Pack chain resolution (user override → author override →
    pack-qualified → dependency chain → hicolor → missing)
  - Resolution cache, glyph cache invalidation hooks
  - Built-in missing-icon SVG
- `mesh-core-render/src/surface/icon.rs`
  - Branch on resolved target kind (file vs. font glyph)
  - Variable-axis pass-through to glyph rasterizer
- `mesh-core-render/src/surface/glyph.rs`
  - swash-based glyph rasterizer
- `mesh-core-elements/src/style/`
  - Parse `--icon-*` custom properties; expose on `ComputedStyle`
  - Future: animatable axis values through the StyleAnimation engine
- `mesh-core-module/src/manifest/`
  - `icon-pack` kind handling, `icon_pack` section parsing
  - Frontend `dependencies.icon_packs` and `icons.overrides`
- `mesh-core-config`
  - `icons.default_pack`, `modules.<id>.icons.{use_packs,overrides}`
- `mesh-core-shell`
  - Discover icon-pack modules; register their mappings + axis
    metadata in the shared registry
  - Compose effective dependency chain per frontend at module load
