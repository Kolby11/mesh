# 06 — Fonts

> Part of the [MESH Specification](README.md).

The font system is deliberately the icon system's twin: semantic **roles**
instead of logical icon names, **font-pack modules** instead of icon packs,
same chain/override/settings idioms. One mental model, two resources. MESH
ships no fonts; fontconfig is the authority for what is installed.

**Status: target** — the v1 scope is deliberately minimal (§8): font packs
register files/families with the text renderer at load, roles surface as
`--font-*` tokens, one settings knob picks the default UI family. Fallback
chains per script are deferred.

## 1. Three layers

```
System font              Font-pack module             Theme / frontend
(fontconfig)             (mapping only)               (logical roles)

Inter, JetBrains Mono →  @mesh/fonts-default        → font-family: var(--font-body)
                         maps body → Inter,
                              mono → JetBrains Mono
```

## 2. Logical roles

Material-3-style role vocabulary; packs and themes may add roles but should
prefer the standard set:

| Role | Use |
| ---- | --- |
| `display` | Hero / large headings |
| `headline` | Section headings |
| `title` | Card / dialog titles |
| `body` | Default running text |
| `label` | UI labels, form fields |
| `caption` | Secondary / fine print |
| `mono` | Code, fixed-width |

## 3. Consumption — roles are theme tokens

Roles surface to styles as **`--font-<role>` theme tokens** (composed into
the theme cascade, [04 §3](04-styling.md)), so components consume fonts the
same way they consume every other design value:

```css
.title  { font-family: var(--font-title); }
.term   { font-family: var(--font-mono); }
.brand  { font-family: "inter/display"; }   /* pack-qualified escape hatch */
```

- A `font-family` value containing `/` is a pack-qualified reference
  (`<pack-id>/<role>`), bypassing the chain — use sparingly.
- A value matching an installed family name verbatim (`font-family: Inter`)
  bypasses the binding system entirely (historical CSS behavior, kept).

## 4. Font-pack module shape

```json
{
  "name": "@mesh/fonts-default",
  "version": "1.0.0",
  "mesh": { "apiVersion": "0.1", "kind": "font-pack" },
  "font_pack": {
    "id": "default",
    "requires": {
      "fonts": [
        { "family": "Inter", "version": ">=4.0" },
        { "family": "JetBrains Mono", "version": ">=2.0" }
      ]
    },
    "mappings": {
      "display": "Inter", "headline": "Inter", "title": "Inter",
      "body": "Inter", "label": "Inter", "caption": "Inter",
      "mono": "JetBrains Mono"
    }
  }
}
```

- `id` — short alias for pack-qualified syntax.
- `requires.fonts` — expected fontconfig families; soft (warn, never block).
- `mappings` — flat role → exact fontconfig family name. No in-pack fallback
  chains; cross-pack fallback is the chain.
- A pack may additionally bundle-register font *files* it ships in-module
  with the text renderer at load (the one case where a pack carries assets,
  for shells on minimal systems). Files register into cosmic-text; they are
  not installed system-wide.

Consumers (themes, frontends) declare packs via
`mesh.uses.resources.fonts: ["@mesh/fonts-default", …]` — same bucket rules
as icons ([05 §3](05-icons.md)).

## 5. User configuration

Via the settings store ([08](08-settings.md)):

```json
{
  "shell": { "fonts": { "packs": ["@mesh/fonts-default"], "ui_family": "body" } },
  "@mesh/navigation-bar": {
    "fonts": { "use_packs": ["@mesh/fonts-compact"],
               "overrides": { "label": "default/caption" } }
  }
}
```

`shell.fonts.packs` prepends to every consumer's chain; `use_packs` replaces
per module; `overrides` is the per-role user knob. `ui_family` is the single
v1 "default UI font" setting.

## 6. Resolution order

For a `font-family` value `X` in module `<id>`:

1. User per-module override (`<id>.fonts.overrides.X`)
2. Author override (manifest `fonts.overrides.X`)
3. Pack-qualified (`pack/role`)
4. Verbatim installed family name
5. Effective chain (user shell packs + declared chain / `use_packs`)
6. System default sans-serif via fontconfig (one warning per `(module, role)`)

## 7. Rendering & caching

The binding layer resolves role → family name and hands it to the existing
text path; cosmic-text + swash keep owning loading, shaping, rasterization,
and per-script glyph fallback. Resolution results cache per
`(module, role, chain)` and flush on pack changes, font settings changes, and
theme changes.

## 8. Deferred

Per-script font stacks, role-level fallback chains inside packs, weight/width
sub-role mapping, and hard version gating are all deferred until the minimal
model proves out. RTL/bidi and missing-script fallback remain renderer
responsibilities ([07 §7](07-i18n.md)).
