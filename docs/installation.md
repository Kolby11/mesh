# Plugin Installation

Installation is the process of taking a plugin package, resolving everything
it depends on, and landing it on disk in a state where the shell can load it.
The core manages the whole flow — users never hand-wire dependencies.

This document covers the manifest format (`plugin.json`), the dependency
kinds, the resolution algorithm, and how the installer handles the conflicts
that come up in a multi-backend ecosystem.

## `plugin.json`

Every plugin has a single `plugin.json` at its package root. It is the
authoritative manifest: identity, dependencies, capabilities, entrypoints,
settings schema, and defaults all live here.

> **Migration note.** `plugin.json` replaces the earlier `mesh.toml`.
> Existing plugin source in `plugins/` still uses the TOML form during
> transition; the installer accepts both but `plugin.json` is the target
> format.

### Shape

```json
{
  "id":          "@mesh/panel",
  "version":     "0.1.0",
  "type":        "surface",
  "api_version": "0.1",
  "description": "Top panel shell surface.",
  "authors":     ["MESH Project"],
  "license":     "MIT",

  "compatibility": {
    "mesh":        ">=0.1.0",
    "compositors": ["wlr-layer-shell-v1"]
  },

  "capabilities": {
    "required": ["shell.surface", "theme.read", "locale.read"],
    "optional": []
  },

  "dependencies": {
    "plugins": {
      "@mesh/audio-contract":   ">=1.0.0, <2.0.0",
      "@mesh/power-contract":   ">=1.0.0"
    },
    "interfaces": [
      { "name": "mesh.audio",   "version": ">=1.0", "required": false },
      { "name": "mesh.power",   "version": ">=1.0", "required": false },
      { "name": "mesh.network", "version": ">=1.0", "required": false }
    ],
    "icon_packs":     { "required": ["@mesh/symbols"], "optional": [] },
    "language_packs": { "optional": ["@mesh/core-translations"] },
    "themes":         { "optional": ["@mesh/default-theme"] },
    "native_libs":    [],
    "binaries":       [],
    "fonts":          []
  },

  "entrypoints": {
    "main":         "src/main.mesh",
    "settings_ui":  null
  },

  "provides_slots": {
    "left":   { "accepts": "widget", "layout": "row", "max": 4 },
    "center": { "accepts": "widget", "layout": "row", "max": 1 },
    "right":  { "accepts": "widget", "layout": "row", "max": 8 }
  },

  "slot_contributions": {},

  "settings": {
    "namespace": "@mesh/panel",
    "schema": {
      "clock_format": {
        "type": "enum", "values": ["12h", "24h"], "default": "24h",
        "description": "Clock display format."
      },
      "show_seconds":        { "type": "boolean", "default": false },
      "show_battery_percent":{ "type": "boolean", "default": true }
    }
  },

  "i18n": {
    "default_locale": "en",
    "bundled":        "config/i18n/"
  },

  "assets": {
    "icons": "assets/icons/"
  }
}
```

### Variant: backend plugin

Backends add a `provides` block (and typically declare system dependencies):

```json
{
  "id":      "@mesh/pipewire-audio",
  "version": "0.1.0",
  "type":    "backend",

  "provides": [
    {
      "interface":    "mesh.audio",
      "version":      "1.0",
      "backend_name": "PipeWire",
      "priority":     100,
      "optional_capabilities": ["set_app_volume", "per_app_mute"]
    }
  ],

  "extensions": [
    {
      "interface": "@mesh/pipewire-audio.extensions",
      "version":   "0.1"
    }
  ],

  "dependencies": {
    "plugins": { "@mesh/audio-contract": ">=1.0.0, <2.0.0" },
    "native_libs": [
      { "name": "libdbus-1.so.3", "reason": "System D-Bus access." }
    ],
    "binaries": [
      {
        "name":    "pw-cli",
        "version": ">=0.3",
        "reason":  "PipeWire control.",
        "packages": {
          "debian": "pipewire",
          "arch":   "pipewire",
          "fedora": "pipewire"
        }
      }
    ]
  },

  "entrypoints": { "main": "src/main.luau" }
}
```

### Variant: contract package

Contracts carry the interface declaration and the shared settings schema:

```json
{
  "id":      "@mesh/audio-contract",
  "version": "1.0.0",
  "type":    "interface",

  "interface": {
    "name":    "mesh.audio",
    "version": "1.0",
    "file":    "interface.toml"
  },

  "settings": {
    "namespace": "mesh.audio",
    "schema": {
      "default_output_priority": {
        "type": "enum", "values": ["speakers","headphones","hdmi"],
        "default": "speakers"
      },
      "auto_resume_on_device_change": { "type": "boolean", "default": true }
    }
  }
}
```

See [`extensibility.md`](./extensibility.md) for the interface model and
[`settings/README.md`](./settings/README.md) for how the inlined settings
schema feeds into the system-wide settings stack.

## Dependency kinds

The installer understands and resolves the following dependency kinds:

| Kind | What it is | How it's satisfied |
|------|------------|-------------------|
| `plugins` | Other MESH plugins (any type) | Registry fetch or local path |
| `interfaces` | Contract name + version the plugin consumes | Must have a provider; installer warns if none exists |
| `icon_packs` | `mesh.icons` providers the plugin expects | Registry fetch |
| `language_packs` | `mesh.locale.source` providers | Registry fetch |
| `themes` | Recommended/fallback themes | Registry fetch, optional-only |
| `native_libs` | System shared libraries | **Detected**, not installed |
| `binaries` | Executables on `$PATH` | **Detected**, not installed |
| `fonts` | Specific font families | **Detected**, not installed |

Kinds marked **detected** are the user's system package manager's
responsibility. The installer reports what's missing and how to get it —
never runs a package manager itself. This is a trust boundary: MESH does
not install system software.

## Resolution algorithm

Given a target plugin ID + version:

1. **Fetch target.** Read `plugin.json`. Verify signature against the trust
   tier (core / verified / community / local — see
   [`spec/pluggable-backend.md`](../spec/pluggable-backend.md#trust-tiers)).
2. **Expand the dependency graph.** Walk `dependencies.plugins`,
   `icon_packs.required`, `language_packs.required`, and any
   `interfaces[*].required == true` entries recursively. Treat `optional:
   true` deps as *offered*, not required — they are added to the graph only
   if the user accepts.
3. **Reconcile versions.** For each unique plugin ID in the graph, compute
   the intersection of all declared version ranges. Pick the highest
   version inside the intersection. If the intersection is empty, fail
   with a "no version satisfies all requirements" diagnostic listing the
   offenders.
4. **Deduplicate.** If a resolved version is already installed and
   compatible, mark it satisfied — don't redownload.
5. **Check capability diffs** for any plugin being **updated**. If the new
   version adds elevated/high capabilities, prompt the user to re-approve.
6. **Check interfaces.** For every `interfaces[*]` entry with `required:
   true`, confirm there is either an existing provider in the graph or one
   being pulled in. If not, emit a **ProviderMissing** warning listing the
   known plugins that provide this interface (the registry keeps an index).
7. **Check system deps.** For each `native_libs`, `binaries`, `fonts` entry
   on any plugin in the graph, probe the system:
   - `native_libs` → `ldconfig -p` lookup
   - `binaries`    → `$PATH` lookup + `--version` probe if `version` specified
   - `fonts`       → `fc-list` lookup
   Missing items produce warnings (not errors) annotated with the per-distro
   package hints the plugin declared.
8. **Stage.** Download every unsatisfied plugin into a staging directory.
   Verify signatures again.
9. **Resolve multi-provider conflicts.** For each interface that would have
   more than one installed provider after staging, consult the existing
   user pin (if any). If no pin and priorities differ, the higher priority
   implicitly wins; the installer notes it. If priorities tie or are
   marked "experimental", prompt the user to pin.
10. **Commit atomically.** Move staged plugins into place, update the
    lockfile (`~/.config/mesh/plugins.lock.json`), emit a
    `PluginInstalled` event per plugin.
11. **Rollback on failure.** If any step after staging fails, the staging
    directory is discarded and no plugins are moved.

### Lockfile

`plugins.lock.json` records the resolved state after a successful
transaction:

```json
{
  "version": 1,
  "plugins": {
    "@mesh/panel":           { "version": "0.1.0", "signature": "…", "source": "registry" },
    "@mesh/pipewire-audio":  { "version": "0.1.0", "signature": "…", "source": "registry" },
    "@mesh/audio-contract":  { "version": "1.0.0", "signature": "…", "source": "registry" }
  },
  "interfaces": {
    "mesh.audio":   { "pinned": "@mesh/pipewire-audio@0.1.0" },
    "mesh.network": { "pinned": "@mesh/networkmanager@0.1.0" }
  }
}
```

The lockfile makes `mesh install` reproducible across machines — the same
`plugin.json` + `plugins.lock.json` produces the same on-disk layout.

## Sources

A dependency can come from four places:

- **Registry** (default) — `@scope/name@version` resolves through the
  configured registry.
- **URL** — a `.mesh-pkg` archive fetched over HTTPS. Signature required.
- **Git** — a repository URL + ref. Useful for development forks.
- **Local path** — a directory on disk. No signature requirement; loaded at
  the `local` trust tier.

```json
"plugins": {
  "@community/weather-widget": ">=1.0.0",
  "@alice/my-audio":   { "git": "https://github.com/alice/my-audio", "ref": "v0.3.0" },
  "@me/local-test":    { "path": "../local-test" }
}
```

## Multiple providers for the same interface

Install-time policy: **allow multiple providers, warn, prompt only when
ambiguous**. The rationale is in [`extensibility.md`](./extensibility.md) —
being able to install a second backend and switch with one click is a
feature, not a bug.

When installing a plugin that provides an interface already provided by
something installed:

```
$ mesh install @mesh/pulseaudio-audio

→ @mesh/pulseaudio-audio@0.1.0 provides mesh.audio.
  mesh.audio is already provided by:
    - @mesh/pipewire-audio@0.1.0 (priority 100, active)

  @mesh/pulseaudio-audio will be installed at priority 50 and remain
  inactive until you pin it in settings.

Continue? [Y/n]
```

When priorities tie, the installer refuses to guess:

```
→ mesh.audio has two installed providers at priority 100:
    - @mesh/pipewire-audio@0.1.0
    - @alice/my-audio@0.3.0

  Pick the active one (this can be changed later):
    1) @mesh/pipewire-audio
    2) @alice/my-audio
```

The answer is written to `settings.json`:

```json
{ "interfaces": { "mesh.audio": { "pin": "@alice/my-audio" } } }
```

## Missing system dependencies

Install proceeds, plugin lands on disk, but the installer prints a clear
block *per plugin* that has unmet system deps:

```
⚠  @mesh/mpris-media has unmet system dependencies:
     Binary 'playerctl' (>=2.0) — not found on $PATH.
       Reason: MPRIS player control.
       Install with:
         Debian/Ubuntu: sudo apt install playerctl
         Arch:          sudo pacman -S playerctl
         Fedora:        sudo dnf install playerctl

The plugin is installed but will start in the 'unavailable' state until
the missing dependency is present. See `mesh doctor` for status.
```

The same check runs on every plugin load, producing the plugin's health
record — see [`health.md`](./health.md).

## CLI

```
mesh install <id>[@version]            # fetch, resolve, install
mesh install ./path/to/plugin          # local install
mesh uninstall <id>
mesh update [<id>]                     # update one or all
mesh list                              # installed plugins + versions
mesh pin <interface> <plugin-id>       # set the active provider
mesh unpin <interface>                 # clear pin, fall back to priority
mesh search <query>
mesh doctor                            # full health + dep check
mesh why <id>                          # show why a plugin is installed (who depends on it)
```

## Directories

```
/usr/share/mesh/plugins/               system-installed plugins
~/.local/share/mesh/plugins/           user-installed plugins
~/.local/share/mesh/dev-plugins/       `mesh dev` loaded
~/.cache/mesh/packages/                downloaded package archives
~/.config/mesh/settings.json           system-wide user settings
~/.config/mesh/plugins/<id>.json       per-plugin user overrides
~/.config/mesh/plugins.lock.json       resolved versions
```

User plugins override system plugins with the same ID. Core plugins at the
system path cannot be uninstalled but can be overridden by installing a
user plugin with the same ID.

## Conflict matrix

| Situation | Installer behaviour |
|-----------|---------------------|
| Same plugin + version already installed | Skip |
| Same plugin, different version | Reconcile to one version satisfying all ranges |
| No version satisfies all ranges | Fail; list the conflicting requirers |
| Two providers for a single-active interface | Allow; warn; prompt for pin only if priorities tie |
| Two providers for a multi-active interface (icons / locale) | Allow; order by priority; no prompt |
| Required native lib / binary missing | Warn; plugin installed; health set to `unavailable` at load |
| Update adds elevated capabilities | Prompt user to re-approve |
| Signature mismatch | Block unconditionally |
| Circular plugin dependency | Fail resolution; print the cycle |

## Summary

- `plugin.json` is the single source of truth (manifest + schema +
  defaults inline).
- Dependencies are kinded: MESH plugins (installed), interfaces
  (must have a provider), and system artefacts (detected, never
  installed).
- Resolution is plan-then-commit with a lockfile; failed transactions
  roll back.
- Multiple providers for the same interface are a feature, not a
  conflict — the installer warns and prompts only when ambiguous.
- System-dep gaps become plugin health states visible to frontends at
  runtime (see [`health.md`](./health.md)).
