# Font System

MESH uses a binding-based font system that mirrors the icon system.
Frontends and themes use semantic logical font names (`display`,
`body`, `mono`); **font-pack modules** map those names to font
families installed on the system. Font installation is outside MESH's
scope — fonts go in `~/.local/share/fonts/` or `/usr/share/fonts/`,
discovered via fontconfig.

This document is the design contract; implementation follows it.

---

## Goals

- Single semantic API in styles: `font-family: body`
- Three-layer model identical to the icon system: system asset →
  MESH binding (font-pack) → frontend
- Themes and frontends are portable across font choices: swap the
  font-pack and the look changes without editing styles
- Pack-qualified escape hatch when an author or user wants a specific
  font for one element
- Per-frontend user overrides without rebuilding the font-pack
- No font files shipped by MESH; fontconfig is the source of truth
  for what is installed

## Non-goals

- MESH does not ship, install, or manage font files.
- No font fetching, registry, or update mechanism.
- Hard version gating against installed fonts — version requirements
  in font-packs are advisory only.

---

## The three layers

```
System font            MESH font-pack module        Frontend / theme
(installed by user)    (mapping only, no fonts)     (uses logical names)

Inter, JetBrains    →  @mesh/fonts-inter          → @mesh/navigation-bar
Mono, ...              maps "body" → Inter,         font-family: body;
~/.local/share/fonts          "mono" → JetBrains    or theme tokens
                              Mono
```

### Layer 1 — System font

Installed via the user's package manager, AUR, manual download into
`~/.local/share/fonts/`, or a setup script. Discovered by MESH
through fontconfig.

### Layer 2 — Font-pack module

A MESH module of kind `font-pack`. Contains **only** a mapping table
plus metadata about the system fonts it expects to find. Ships **no**
fonts.

The job is to translate between the logical font role names a theme
or frontend wants (`display`, `body`, `mono`, `headline`) and the
font family names installed on the system (`Inter`, `IBM Plex Sans`,
`JetBrains Mono`).

Multiple font-pack modules can wrap the same underlying fonts with
different role names — e.g. one pack uses Material 3's role
vocabulary, another uses Apple HIG's, both pointing at the same
installed Inter family.

### Layer 3 — Frontend / theme

Themes are the primary consumers (they bind logical roles to tokens
like `typography.body.family`). Frontends use the same logical names
in CSS via `font-family: body`. Both declare which font-pack(s) they
depend on.

---

## Logical names

Standard role-based vocabulary:

| Role        | Typical use                              |
|-------------|------------------------------------------|
| `display`   | Hero / large headings                     |
| `headline`  | Section headings                          |
| `title`     | Card / dialog titles                      |
| `body`      | Default running text                      |
| `label`     | UI labels, form fields                    |
| `caption`   | Secondary / fine print                    |
| `mono`      | Code, terminals, fixed-width              |

These mirror Material 3's typography roles. Frontends and themes are
free to define additional roles, but should prefer the standard set.

### Pack-qualified escape hatch

For pinning one specific font from a specific pack regardless of the
active mapping, use `<pack-id>/<logical-name>` syntax in
`font-family`:

```css
.code-block { font-family: "ibm-plex/mono"; }
.brand      { font-family: "inter/display"; }
```

The slash separates the font-pack module id (or its short alias)
from the logical role resolved through that pack. This bypasses the
active default and the dependency chain.

Use sparingly. The whole point of the binding system is that styles
don't bake in font choices. Pack-qualified names are an escape hatch
for the rare cases where a specific font really does matter (brand
typography, a code block that must be JetBrains Mono specifically,
etc.).

---

## Font-pack module shape

Pack manifest (`package.json` with `mesh.kind = "font-pack"`):

```json
{
  "name": "@mesh/fonts-default",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "font-pack"
  },
  "font_pack": {
    "id": "default",
    "requires": {
      "fonts": [
        { "family": "Inter",          "version": ">=4.0" },
        { "family": "JetBrains Mono", "version": ">=2.0" }
      ]
    },
    "mappings": {
      "display":  "Inter",
      "headline": "Inter",
      "title":    "Inter",
      "body":     "Inter",
      "label":    "Inter",
      "caption":  "Inter",
      "mono":     "JetBrains Mono"
    }
  }
}
```

### Field reference

- **`id`** — short alias used in pack-qualified `font-family`
  syntax (`default/body`). Should be globally unique.
- **`requires.fonts`** — declares system font families the pack
  expects, matched via fontconfig. Versions are **soft**: missing or
  older fonts log a warning, never block loading. Resolution time
  presence is the source of truth.
- **`mappings`** — flat 1:1 map from logical role → installed font
  family name (the right-hand side is the exact fontconfig family
  name). No fallback chains inside a single pack — cross-pack
  fallback is handled by depending on multiple font-packs.

A single font-pack can wrap multiple system fonts freely; mapping
entries can target different families.

---

## Frontend / theme usage

Frontend / theme manifest:

```json
{
  "name": "@mesh/material-theme",
  "mesh": {
    "kind": "theme",
    "dependencies": {
      "font_packs": [
        "@mesh/fonts-default",
        "@mesh/fonts-system-fallback"
      ]
    }
  },
  "fonts": {
    "overrides": {
      "mono": "@mesh/fonts-mono-extra/fira-code"
    }
  }
}
```

- **`dependencies.font_packs`** — ordered list. Resolution prefers
  the first pack that defines the requested logical role; subsequent
  packs are fallbacks for roles the first one doesn't define.
- **`fonts.overrides`** — author-side per-role escape hatch. Format
  matches pack-qualified syntax. This is **not** the same as
  user-side overrides (which live in shell `settings.json`).

### Implicit shell-default pack

The user's chosen shell-default font-pack is **prepended** to every
frontend's `dependencies.font_packs` list at resolution time. This
makes the shell default the highest-priority source for any logical
role without each frontend opting in.

A frontend that explicitly does not want the shell default can
declare `fonts.ignore_shell_default: true`.

---

## CSS usage

In stylesheets, use logical roles directly in `font-family`:

```css
body         { font-family: body; }
h1, .display { font-family: display; }
code         { font-family: mono; }

.brand       { font-family: "inter/display"; }      /* pack-qualified */
.terminal    { font-family: "jetbrains/mono"; }
```

The CSS parser treats a `font-family` value containing `/` as a
pack-qualified reference; otherwise it's a logical role name
resolved through the active font-pack chain.

For compatibility, `font-family` values that match an actual
installed font family name verbatim (e.g. `font-family: Inter`)
still work — they bypass the binding system entirely. This is the
historical CSS behavior and is preserved for ad-hoc use.

---

## Shell configuration

User-side configuration in shell `settings.json`:

```json
{
  "fonts": {
    "default_pack": "@mesh/fonts-default"
  },
  "modules": {
    "navigation-bar": {
      "fonts": {
        "use_packs": ["@mesh/fonts-compact"],
        "overrides": {
          "label": "@mesh/fonts-default/caption"
        }
      }
    }
  }
}
```

- **`fonts.default_pack`** — shell-wide preferred font-pack,
  implicitly prepended to every frontend's dependencies.
- **`modules.<id>.fonts.use_packs`** — replaces the frontend's
  declared `dependencies.font_packs` for that module only. Useful
  for setting different typography in one panel.
- **`modules.<id>.fonts.overrides`** — per-role override prepended
  in front of every other resolution path.

---

## Resolution order

For any `font-family: X` style declaration inside frontend `<id>`:

1. **User override** — `modules.<id>.fonts.overrides.X`, if present.
2. **Frontend / theme author override** — manifest's
   `fonts.overrides.X`, if present.
3. **Pack-qualified value** — if `X` is `pack/role`, resolve
   directly through `pack`.
4. **Verbatim system family** — if `X` exactly matches an installed
   font family name, use it as-is (compatibility behavior).
5. **Effective dependency chain** — shell-default font-pack
   (unless suppressed) followed by manifest's
   `dependencies.font_packs` (or `modules.<id>.fonts.use_packs`),
   tried in order. First pack whose `mappings` defines `X` and
   whose target resolves to an installed font wins.
6. **System default sans-serif** — fontconfig-provided fallback if
   nothing else resolved. Logs a one-time warning per
   `(module_id, role_name)`.

---

## Discovery

At shell startup MESH:

1. Discovers installed `font-pack` modules via standard module
   discovery paths.
2. For each pack, queries fontconfig for declared `requires.fonts`
   entries; mismatches log soft warnings.
3. Builds the effective font-pack chain per frontend at module
   load (shell default prepended unless suppressed).

There is no MESH font registry or fetcher. Fontconfig is the
authority for what is installed.

---

## Caching

The resolver caches resolution results per `(module_id, role,
active_pack_chain)`. Cache is flushed when:

- A font-pack module is loaded, reloaded, or removed
- Shell `fonts` settings change
- The active theme changes (themes can re-bind role tokens)

cosmic-text's existing font system + swash glyph cache continue to
handle the actual font loading and rasterization; the binding layer
only resolves logical role → installed family name and hands that
to the existing text path.

---

## Implementation surface

- `mesh-core-font` (new crate, parallel to `mesh-core-icon`)
  - Font-pack manifest parsing (`font_pack` section)
  - Pack chain resolution (user override → author override →
    pack-qualified → verbatim → dependency chain → system default)
  - Resolution cache
- `mesh-core-elements/src/style/`
  - Parse `font-family` values: detect `pack/role` shape vs.
    verbatim family name vs. logical role
  - Surface the resolved family name for the text renderer
- `mesh-core-render/src/surface/text.rs`
  - Consume the resolved family name from `ComputedStyle`
  - No changes to glyph rasterization (cosmic-text already handles
    fontconfig family lookup)
- `mesh-core-module/src/manifest/`
  - `font-pack` kind handling, `font_pack` section parsing
  - Frontend / theme `dependencies.font_packs` and `fonts.overrides`
- `mesh-core-config`
  - `fonts.default_pack`, `modules.<id>.fonts.{use_packs,overrides}`
- `mesh-core-shell`
  - Discover font-pack modules; register their mappings in the
    shared font registry
  - Compose effective dependency chain per frontend at module load

---

## Relationship to the icon system

The font system is intentionally a clone of the icon system in
shape, configuration vocabulary, and resolution order. Anywhere
the icon docs say "icon-pack", "logical name", "shell default",
"author override", "user override" — the equivalents apply
verbatim here for fonts. This symmetry is deliberate so authors and
users only have one mental model to learn.

The two systems are wired separately in code (`mesh-core-icon` vs
`mesh-core-font`) but follow the same layering and idioms.
