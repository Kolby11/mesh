# Module Installation

Installation is the process of taking a module package, resolving everything
it depends on, and landing it on disk in a state where the shell can load it.
The core manages the whole flow — users never hand-wire dependencies.

This document covers the manifest format (`module.json`), the dependency
kinds, the resolution algorithm, and how the installer handles the conflicts
that come up in a multi-backend ecosystem.

## `module.json`

Every module has a single `module.json` at its module root. It is the
authoritative manifest: identity, dependencies, capabilities, entrypoints,
settings schema, and defaults all live here.

For the default service stack, even the built-in interface contracts are
ordinary modules on disk. The shell core does not seed service APIs at
startup; it discovers interface and backend modules the same way it discovers
frontends.

`module.json` uses top-level `name` and `version` for package identity. MESH
runtime metadata lives under `mesh`, including `mesh.apiVersion`, `mesh.kind`,
`mesh.implements`, and `mesh.interface` where applicable.

> **Migration note.** `package.json`, legacy `module.json` fields, and
> `mesh.toml` are internal migration inputs. They warn when accepted and should
> be replaced with canonical `module.json`.

### Shape

```json
{
  "name": "@mesh/panel",
  "version": "0.1.0",
  "description": "Top panel shell surface.",
  "license": "MIT",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "capabilities": {
      "required": ["shell.surface", "theme.read", "locale.read"]
    },
    "dependencies": {
      "modules": {
        "@mesh/audio-interface": ">=1.0.0, <2.0.0"
      },
      "icons": {
        "@mesh/icons-default": "*"
      }
    },
    "entrypoints": {
      "main": "src/main.mesh"
    },
    "contributes": {
      "layout": [
        {
          "id": "main",
          "entrypoint": "src/main.mesh",
          "label": "Panel"
        }
      ],
      "settings": {
        "namespace": "@mesh/panel",
        "schema": {
          "clock_format": {
            "type": "enum",
            "values": ["12h", "24h"],
            "default": "24h",
            "description": "Clock display format."
          },
          "show_seconds": {
            "type": "boolean",
            "default": false
          }
        }
      },
      "i18n": {
        "defaultLocale": "en",
        "path": "config/i18n/"
      }
    },
    "surfaceLayout": {
      "size_policy": "fixed"
    }
  }
}
```

### Frontend composition

Frontend modules can embed other frontend modules in two complementary ways:

- `mesh.dependencies.modules` declares the frontend modules you want to consume
- `mesh.entrypoints` declares loadable `.mesh` surfaces or widgets
- `mesh.contributes.layout` exposes named layout entries that the installed
  graph can index

This lets a surface act as a host shell while keeping individual widgets
packaged, versioned, and replaceable on their own.

### Variant: backend module

Backends add an `implements` block (and typically declare system dependencies):

```json
{
  "name": "@mesh/pipewire-audio",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "backend",
    "capabilities": {
      "required": ["exec.wpctl"]
    },
    "implements": [
      {
        "interface": "mesh.audio",
        "version": "1.0",
        "provider": "pipewire",
        "priority": 100,
        "optional_capabilities": ["set_app_volume", "per_app_mute"]
      }
    ],
    "dependencies": {
      "modules": { "@mesh/audio-interface": ">=1.0.0, <2.0.0" },
      "binaries": [
        {
          "name": "wpctl",
          "reason": "PipeWire volume and mute control.",
          "packages": {
            "debian": "wireplumber",
            "arch": "wireplumber",
            "fedora": "wireplumber"
          }
        }
      ]
    },
    "entrypoints": { "main": "src/main.luau" }
  }
}
```

### Variant: interface module

Interface modules carry the interface declaration and the shared settings
schema:

```json
{
  "name": "@mesh/audio-interface",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "interface",
    "interface": {
      "name": "mesh.audio",
      "version": "1.0",
      "file": "interface.toml",
      "domain": "audio",
      "relationship": "base"
    },
    "contributes": {
      "settings": {
        "namespace": "mesh.audio",
        "schema": {
          "default_output_priority": {
            "type": "enum",
            "values": ["speakers", "headphones", "hdmi"],
            "default": "speakers"
          },
          "auto_resume_on_device_change": {
            "type": "boolean",
            "default": true
          }
        }
      }
    }
  }
}
```

See [`extensibility.md`](./extensibility.md) for the interface model and
[`settings/README.md`](./settings/README.md) for how the inlined settings
schema feeds into the system-wide settings stack.

## Dependency kinds

The installer understands and resolves the following dependency kinds:

| Kind             | What it is                                  | How it's satisfied                                   |
| ---------------- | ------------------------------------------- | ---------------------------------------------------- |
| `modules`        | Other MESH modules (any type)               | Registry fetch or local path                         |
| `interfaces`     | Contract name + version the module consumes | Must have a provider; installer warns if none exists |
| `icon_packs`     | `mesh.icons` providers the module expects   | Registry fetch                                       |
| `language_packs` | `mesh.locale.source` providers              | Registry fetch                                       |
| `themes`         | Recommended/fallback themes                 | Registry fetch, optional-only                        |
| `native_libs`    | System shared libraries                     | **Detected**, not installed                          |
| `binaries`       | Executables on `$PATH`                      | **Detected**, not installed                          |
| `fonts`          | Specific font families                      | **Detected**, not installed                          |

Kinds marked **detected** are the user's system package manager's
responsibility. The installer reports what's missing and how to get it —
never runs a package manager itself. This is a trust boundary: MESH does
not install system software.

## Resolution algorithm

Given a target module ID + version:

1. **Fetch target.** Read `module.json`. Verify signature against the trust
   tier (core / verified / community / local — see
   [`spec/pluggable-backend.md`](../spec/pluggable-backend.md#trust-tiers)).
2. **Expand the dependency graph.** Walk `mesh.dependencies.modules`,
   resource requirements, and interface dependencies recursively. Treat
   optional deps as *offered*, not required — they are added to the graph only
   if the user accepts.
3. **Reconcile versions.** For each unique module ID in the graph, compute
   the intersection of all declared version ranges. Pick the highest
   version inside the intersection. If the intersection is empty, fail
   with a "no version satisfies all requirements" diagnostic listing the
   offenders.
4. **Deduplicate.** If a resolved version is already installed and
   compatible, mark it satisfied — don't redownload.
5. **Check capability diffs** for any module being **updated**. If the new
   version adds elevated/high capabilities, prompt the user to re-approve.
6. **Check interfaces.** For every `interfaces[*]` entry with `required:
   true`, confirm there is either an existing provider in the graph or one
   being pulled in. If not, emit a **ProviderMissing** warning listing the
   known modules that provide this interface (the registry keeps an index).
7. **Check system deps.** For each `native_libs`, `binaries`, `fonts` entry
   on any module in the graph, probe the system:
   - `native_libs` → `ldconfig -p` lookup
   - `binaries`    → `$PATH` lookup + `--version` probe if `version` specified
   - `fonts`       → `fc-list` lookup
   Missing items produce warnings (not errors) annotated with the per-distro
   package hints the module declared.
8. **Stage.** Download every unsatisfied module into a staging directory.
   Verify signatures again.
9. **Resolve multi-provider conflicts.** For each interface that would have
   more than one installed provider after staging, consult the existing
   user pin (if any). If no pin and priorities differ, the higher priority
   implicitly wins; the installer notes it. If priorities tie or are
   marked "experimental", prompt the user to pin.
10. **Commit atomically.** Move staged modules into place, update the
    lockfile (`~/.config/mesh/modules.lock.json`), emit a
    `ModuleInstalled` event per module.
11. **Rollback on failure.** If any step after staging fails, the staging
    directory is discarded and no modules are moved.

### Lockfile

`modules.lock.json` records the resolved state after a successful
transaction:

```json
{
  "version": 1,
  "modules": {
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

The lockfile makes `mesh-shell install` reproducible across machines — the same
`module.json` + `modules.lock.json` produces the same on-disk layout.

## Sources

A dependency can come from four places:

- **Registry** (default) — `@scope/name@version` resolves through the
  configured registry.
- **URL** — a `.mesh-pkg` archive fetched over HTTPS. Signature required.
- **Git** — a repository URL + ref. Useful for development forks.
- **Local path** — a directory on disk. No signature requirement; loaded at
  the `local` trust tier.

```json
"modules": {
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

When installing a module that provides an interface already provided by
something installed:

```
$ mesh-shell install @mesh/pulseaudio-audio

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

Install proceeds, module lands on disk, but the installer prints a clear
block *per module* that has unmet system deps:

```
⚠  @mesh/mpris-media has unmet system dependencies:
     Binary 'playerctl' (>=2.0) — not found on $PATH.
       Reason: MPRIS player control.
       Install with:
         Debian/Ubuntu: sudo apt install playerctl
         Arch:          sudo pacman -S playerctl
         Fedora:        sudo dnf install playerctl

The module is installed but will start in the 'unavailable' state until
the missing dependency is present. See `mesh-shell doctor` for status.
```

The same check runs on every module load, producing the module's health
record — see [`health.md`](./health.md).

## CLI

```
mesh-shell install <id>[@version]            # fetch, resolve, install
mesh-shell install ./path/to/module          # local install
mesh-shell uninstall <id>
mesh-shell update [<id>]                     # update one or all
mesh-shell list                              # installed modules + versions
mesh-shell pin <interface> <module-id>       # set the active provider
mesh-shell unpin <interface>                 # clear pin, fall back to priority
mesh-shell search <query>
mesh-shell doctor                            # full health + dep check
mesh-shell why <id>                          # show why a module is installed (who depends on it)
```

## Directories

```
/usr/share/mesh/modules/               system-installed modules
~/.local/share/mesh/modules/           user-installed modules
~/.local/share/mesh/dev-modules/       `mesh-shell dev` loaded
~/.cache/mesh/packages/                downloaded package archives
~/.config/mesh/settings.json           system-wide user settings
~/.config/mesh/modules/<id>.json       per-module user overrides
~/.config/mesh/modules.lock.json       resolved versions
```

User modules override system modules with the same ID. Core modules at the
system path cannot be uninstalled but can be overridden by installing a
user module with the same ID.

## Conflict matrix

| Situation                                                   | Installer behaviour                                         |
| ----------------------------------------------------------- | ----------------------------------------------------------- |
| Same module + version already installed                     | Skip                                                        |
| Same module, different version                              | Reconcile to one version satisfying all ranges              |
| No version satisfies all ranges                             | Fail; list the conflicting requirers                        |
| Two providers for a single-active interface                 | Allow; warn; prompt for pin only if priorities tie          |
| Two providers for a multi-active interface (icons / locale) | Allow; order by priority; no prompt                         |
| Required native lib / binary missing                        | Warn; module installed; health set to `unavailable` at load |
| Update adds elevated capabilities                           | Prompt user to re-approve                                   |
| Signature mismatch                                          | Block unconditionally                                       |
| Circular module dependency                                  | Fail resolution; print the cycle                            |

## Summary

- `module.json` is the single source of truth (manifest + schema +
  defaults inline).
- Dependencies are kinded: MESH modules (installed), interfaces
  (must have a provider), and system artefacts (detected, never
  installed).
- Resolution is plan-then-commit with a lockfile; failed transactions
  roll back.
- Multiple providers for the same interface are a feature, not a
  conflict — the installer warns and prompts only when ambiguous.
- System-dep gaps become module health states visible to frontends at
  runtime (see [`health.md`](./health.md)).
