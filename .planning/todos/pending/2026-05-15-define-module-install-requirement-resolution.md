---
created: 2026-05-15T22:30:23.194Z
title: Define module install requirement resolution
area: planning
related_phases:
  - core-restructure
  - 38
  - 39
files:
  - .planning/ROADMAP.md
  - .planning/STATE.md
---

## Problem

The v1.7 module model and core restructure need to make install-time dependency and contribution resolution concrete, not just normalize manifest shape. This is core architecture work: the Rust core should own typed registries, graph resolution, provider/interface matching, diagnostics, and runtime lookup boundaries, while modules provide all policy/data/UI.

The user clarified that frontend modules should not depend directly on backend modules. A frontend should require an interface contract, and one or more user-installed backends should implement that interface. The open design question is how MESH handles contradictory backend implementations, provider conflicts, and missing resources without tying surfaces to implementation packages.

The same issue applies to resource packs. A module may reference icons, sounds, fonts, keybinds, languages, themes, and interface contracts, but the currently selected icon pack or font pack may not contain the requested symbolic resource. Each module package should declare what it needs and what it contributes so the installed graph can resolve or diagnose those relationships during module installation and graph rebuilds.

The capture also includes the broader configuration direction: every top-level resource section should be a typed partial object with the same schema available under `modules.<id>`, so shell-wide defaults and module-scoped overrides share one validation model. This applies to theme, i18n, icons, fonts, sounds, keybinds, and module settings, with module `settings` remaining schema-specific to the module manifest.

The user also clarified the intended `shell-settings.json` shape after module installation. Module `module.json` files declare their default configurable values and resource preferences, and installation materializes editable module entries into shell settings. The shell settings file is where the user configures module params, while the original defaults should still be stored or recoverable separately so reset-to-default and manifest upgrades remain deterministic.

## Solution

Design the core restructure and Phase 38/39 around explicit requirement and contribution declarations in the canonical package manifest:

- Rust core owns the installed graph, typed contribution registries, interface/provider compatibility checks, cascade resolver, resource lookup, and author/user diagnostics.
- Core should remain mechanism-only: no built-in user-visible categories, strings, theme values, icons, fonts, sounds, or UI. Those arrive through modules and resource packs.
- Core runtime APIs should expose resolved interfaces and resources by contract/id, not concrete package names, so frontend code binds to interface contracts and symbolic resources.
- Frontend/surface packages declare required interfaces by contract id and version range, never concrete backend packages.
- Backend/provider packages declare implemented interfaces, provider ids, capability needs, priority/default metadata, and whether multiple providers may coexist.
- The installed graph validates interface requirements against installed provider contributions, reports missing providers, and reports contradictory exclusive providers as install/load diagnostics that the user can resolve by choosing a provider.
- Resource-using modules declare required symbolic icon ids, sound ids, keybind action ids, supported locales, desired font families or typography tokens, and any direct fallback references they ship.
- Resource packs declare contributed icons, sounds, fonts, locales, aliases, and variants. Missing resources resolve to placeholders with diagnostics, but install-time validation should surface the gap early and optionally suggest packs or aliases that satisfy it.
- IconRef, SoundRef, and similar reference slots should support both qualified pack refs such as `@pack:name` and direct sources such as `{ src: "..." }`, using one schema slot and structural dispatch in the resolver.
- Module-level overrides use the same typed partial schemas as shell-level config: `theme`, `i18n`, `icons`, `fonts`, `sounds`, and `keybinds` under `modules.<id>`. Per-module overrides beat manifest overrides and shell-wide values.
- Token references such as `token(animation.duration.short)` resolve at render time in module context, so module overrides, shell overrides, active theme tokens, and fallbacks all participate in the same cascade.
- Installation should populate `shell-settings.json` with a top-level shell-wide section set plus `modules.<module-id>` entries. Each module entry may contain `settings` plus partial resource override sections such as `theme`, `i18n`, `icons`, `fonts`, `sounds`, and `keybinds`.
- Module defaults come from `module.json` and should be preserved separately from user-edited shell settings, either in the installed graph metadata, a defaults snapshot, or another deterministic source. The shell-facing file should represent the user's configurable/effective overrides, not erase the module-authored defaults.
- The intended settings shape is:

```json
{
  "theme": {
    "active": "@mesh/theme-material-dark",
    "tokens": {
      "color.primary": "#7B61FF"
    },
    "defaults": {
      "components": {
        "button": { "border-radius": "12px" }
      }
    }
  },

  "i18n": {
    "locale": "sk",
    "fallback_locale": "en"
  },

  "icons": {
    "default_pack": "@mesh/icons-material-symbols",
    "remap": {
      "settings": "@mesh/icons-feather:gear"
    }
  },

  "fonts": {
    "families": {
      "sans": "Inter",
      "mono": "JetBrains Mono",
      "display": "Inter"
    }
  },

  "sounds": {
    "default_pack": "@mesh/sounds-default",
    "muted": false
  },

  "keybinds": {
    "shell.toggle-overview": "Super+Space",
    "shell.lock": "Super+L"
  },

  "modules": {
    "@core/navigation": {
      "settings": { "show_workspaces": true, "max_items": 8 }
    },

    "@core/audio-widget": {
      "settings": { "show_per_app_sliders": true },
      "theme": { "tokens": { "color.primary": "#FF8800" } },
      "icons": { "remap": { "volume-mute": "@user/personal-icons:my-mute" } },
      "keybinds": { "mute": "XF86AudioMute" },
      "fonts": { "families": { "sans": "Roboto" } }
    },

    "@core/devtools": {
      "i18n": { "locale": "en" },
      "theme": { "active": "@mesh/theme-light" }
    }
  }
}
```

The resulting author model should be: a package declares what it provides, what it needs, and which fallback resources it can tolerate; installation resolves the graph; runtime uses the same cascade and distributed registries for all lookups.
