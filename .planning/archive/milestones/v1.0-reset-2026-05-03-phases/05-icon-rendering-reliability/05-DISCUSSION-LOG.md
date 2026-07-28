# Phase 5: Icon Rendering Reliability - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md - this log preserves the alternatives considered.

**Date:** 2026-05-03T13:02:38Z
**Phase:** 05-Icon Rendering Reliability
**Areas discussed:** Lookup source of truth, Missing-icon behavior, Rendering fidelity, Proof surfaces

---

## Lookup Source of Truth

| Option | Description | Selected |
|--------|-------------|----------|
| XDG + bundled fallback | Fix the current XDG search path and bundled Material fallback only. | |
| Icon-pack contract slice | Start implementing icon-pack plugin/fallback-chain behavior now. | |
| Hybrid/user mapping | Semantic icon names resolve through user-selectable mappings and configured packs. | yes |

**User's choice:** Semantic icon contract with dedicated icon config, configured pack roots, named mapping profiles, and ordered fallback lists.
**Notes:** The user wants flexible icon mappings because different packs use different names. A user can switch from rounded to filled to lucide-style icons by changing the active mapping profile. Frontend plugins/components should declare their required semantic icons and relevant icon packs so missing assets produce diagnostics.

Follow-up decisions:

| Question | Selected answer |
|----------|-----------------|
| Where should mappings live? | Dedicated icon config files. |
| What does a mapping entry guarantee? | Ordered semantic fallback list; first available icon wins. |
| How strict are icon requirements? | Warn and fallback, do not block plugin load. |
| How are packs discovered? | Explicit configured pack IDs and roots. |
| Config shape? | Named profiles with one active profile. |
| Does config define pack roots too? | Yes, one dedicated config owns both pack roots and mappings. |

---

## Missing-Icon Behavior

| Option | Description | Selected |
|--------|-------------|----------|
| Neutral placeholder icon | Render a stable fallback glyph, diagnose, and keep layout stable. | yes |
| Empty space | Reserve the icon box but draw nothing. | |
| Text marker | Render a small question mark or label-style marker. | |

**User's choice:** Neutral placeholder icon with diagnostics and stable layout.
**Notes:** Diagnostics should deduplicate by plugin plus semantic icon name. Core should own a tiny built-in vector fallback so missing-icon rendering never depends on icon resolution working. Missing icons mark plugin health warning/degraded but keep the plugin active.

---

## Rendering Fidelity

| Option | Description | Selected |
|--------|-------------|----------|
| Inherit color with multicolor opt-out | Symbol icons tint like text; logos/flags preserve original colors when marked multicolor. | yes |
| Always tint | Every asset becomes an alpha mask. | |
| Preserve raster colors | Raster assets keep original colors. | |

**User's choice:** Inherit surrounding color by default, with multicolor opt-out.
**Notes:** Layout/CSS box size wins; `size` is only a lookup hint. Phase 5 needs practical monochrome SVG support, not full complex SVG fidelity. Raster icons also use alpha-mask tint unless marked multicolor.

---

## Proof Surfaces

| Option | Description | Selected |
|--------|-------------|----------|
| Panel + quick settings + navigation bar | Covers Phase 4 surfaces plus existing icon-heavy navigation components. | yes |
| Panel + quick settings only | Stays closest to current core-surface milestone. | |
| All bundled frontend plugins | Strongest coverage, but risks broad UI cleanup. | |

**User's choice:** Panel, quick settings, and navigation bar.
**Notes:** The proof must include SVG rendering, raster rendering, and missing-icon fallback. Config-level profile switching is enough; no visible UI switch is required. Shipped core surfaces should use semantic names only, with pack-specific filenames and filesystem paths confined to mappings/config.

---

## the agent's Discretion

- Choose exact dedicated icon config file format.
- Choose exact built-in vector fallback shape.
- Choose implementation details for diagnostic deduplication and cache invalidation.

## Deferred Ideas

- User-facing profile switcher UI.
- Full complex SVG fidelity.
- Automatic icon-pack registry/provider chains beyond explicit configured pack roots.
