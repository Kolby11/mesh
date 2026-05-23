# Module System

MESH modules should feel easy to author without letting the ecosystem turn
into one-off backend/frontend pairs. The core rule is:

> A module is the installable MESH unit. An interface is the contract. A
> provider implements the contract. A frontend consumes the contract. Shared
> Luau libraries hold reusable implementation patterns.

This gives users one workflow whether they are building UI, a backend service,
a theme, an icon pack, or a shared library.

Canonical vocabulary lives in [MESH Module Vocabulary](module-vocabulary.md).
`module.json` is the canonical author-facing manifest name. `package.json` is
an old term listed in the vocabulary inventory, and any temporary loader for it
is an internal-only migration path rather than public author vocabulary.

## Principles

1. **One module model.** Every installable thing is a module with
   `module.json`. Temporary loaders for old manifest names are internal-only
   migration paths, but new docs and examples use `module.json`.
2. **Interfaces are data, not code.** Service APIs live in interface modules
   such as `@mesh/audio-interface`. The Rust core validates and routes calls;
   it does not know audio, network, power, or media behavior.
3. **Backend modules are adapters.** A backend maps a real system source
   such as PipeWire, PulseAudio, UPower, NetworkManager, MPRIS, or a web API
   into an interface contract.
4. **Frontend modules are views.** A frontend reads interface state and calls
   interface methods. It never imports a backend module by ID.
5. **Libraries prevent reinvention.** Common parsing, polling, D-Bus helpers,
   command-result shaping, validation, and UI helpers should live in Luau
   library modules that both frontend and backend modules can depend on.
6. **Capabilities gate host power.** Shared libraries do not grant access by
   themselves. The module using a library must still request the capabilities
   needed by its calls.
7. **Modules are object instances at runtime.** Frontend and backend-facing
   APIs should use normal Luau object surfaces: `module.state`,
   `module.exports`, `module:<method>(...)`, and `module.events.Name`.
   Rust still owns routing, validation, replay, permissions, lifecycle, and
   diagnostics underneath that syntax.

## Module Manifest Shape

Use `module.json` for every new module:

`module.json` is the author-facing manifest. Top-level fields should identify
the module and its release metadata, such as `name`, `version`, `description`,
`private`, `license`, and `repository`. All MESH-specific fields live under
`mesh`.

- Do not use top-level `type` for module kind; use `mesh.kind`.
- Do not use top-level `id`; use npm's top-level `name`.
- Do not put MESH dependency objects in top-level `dependencies`; use
  `mesh.dependencies`.
- Do not put capabilities, providers, entrypoints, settings, themes, or binary
  requirements at the top level.

Package managers can be used as development/distribution tooling around these
files. They are not the authority for MESH behavior. MESH reads the
`mesh` section, validates capabilities and native requirements, resolves
interface providers, and decides which modules are enabled.

## Migration Diagnostics

Old manifest file names are replacement or removal targets, not public
author-facing aliases. When an old name is still accepted, it is an
internal migration input that emits a warning and should be replaced before
publishing a module.

| Input | Severity | Author action | Runtime behavior |
| ----- | -------- | ------------- | ---------------- |
| `package.json` | warning | replace package.json with module.json | Loads after normalizing the legacy package manifest shape. |
| legacy `module.json` with `id/type/api_version` | warning | replace legacy module.json fields with name/version/mesh | Loads after normalizing the legacy module manifest shape. |
| `mesh.toml` | warning | replace mesh.toml with module.json | Loads after normalizing the legacy TOML manifest shape. |
| `plugin.json` | error | remove plugin.json or replace it with module.json | Fails manifest loading. |
| multiple manifest files | error | keep canonical module.json and remove the old manifest file | Fails manifest loading until the ambiguous old file is removed. |

The root installed-module graph follows the same rule. `config/module.json`
uses the root graph shape directly because it is not an installable module:

```json
{
  "schemaVersion": 1,
  "modulesDir": "../modules",
  "modules": {
    "@mesh/panel": {
      "kind": "frontend",
      "path": "frontend/panel",
      "enabled": true
    }
  },
  "providers": {
    "mesh.audio": "@mesh/pipewire-audio"
  },
  "layout": {
    "entrypoint": "@mesh/panel:main"
  }
}
```

## Extend or Add a MESH Module

Use the shipped audio/navigation path as the authoring model:

1. Start with a canonical `module.json` and put all MESH behavior under
   `mesh`.
2. Make the UI a frontend module. `@mesh/navigation-bar` declares
   `mesh.kind: "frontend"`, contributes its `main` layout entrypoint, and
   declares `mesh.keybinds.mute` plus icon requirements.
3. For frontend modules, renderer migration expectations live in [the .mesh renderer contract](frontend/renderer-contract.md); module authors should not depend on proof snapshots, candidate renderer crates, or browser DOM behavior.
4. Depend on an interface contract, not a backend module ID. The navigation
   volume control imports `mesh.audio@>=1.0`; it does not import
   `@mesh/pipewire-audio` or `@mesh/pulseaudio-audio`.
5. Define the contract in an interface module. `@mesh/audio-interface`
   declares `mesh.audio`, its contract file, domain metadata, shared settings,
   and any reusable contract libraries.
6. Implement the contract with backend providers. `@mesh/pipewire-audio` and
   `@mesh/pulseaudio-audio` declare `mesh.kind: "backend"` and
   `mesh.implements` records for `mesh.audio`, each with provider metadata and
   native binary requirements.
7. Select active providers in the root graph. `config/module.json` enables the
   shipped modules, keeps both audio providers available, and selects
   `@mesh/pipewire-audio` as the active `mesh.audio` provider.
8. Put layout, settings, keybinds, icons, and resources in contributions or
   requirements. The installed graph preserves those records so the shell can
   apply user overrides and validate module gaps without re-reading arbitrary
   source files.
9. Treat diagnostics as part of the workflow. Missing providers, missing icon
   requirements, unresolved resources, settings schema gaps, and ambiguous
   legacy manifests should be reported as diagnostics with a concrete author
   action.

The Rust shell routes generic interface/provider records. PipeWire and
PulseAudio behavior stays in Luau backend provider modules.

```json
{
  "name": "@alice/lmsensors",
  "version": "1.0.0",
  "description": "Thermal sensor provider.",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "backend",
    "i18n": {
      "defaultLocale": "en",
      "supportedLocales": ["en", "sk"]
    },
    "entrypoints": {
      "main": "src/main.luau"
    },
    "capabilities": {
      "required": ["exec.sensors"]
    },
    "dependencies": {
      "modules": {
        "@alice/thermal-interface": ">=1.0.0, <2.0.0",
        "@mesh/backend-kit": ">=0.1.0"
      },
      "binaries": [
        {
          "name": "sensors",
          "reason": "Read thermal sensor data from lm-sensors."
        }
      ]
    },
    "implements": [
      {
        "interface": "alice.thermal",
        "provider": "lmsensors",
        "label": "lm-sensors",
        "priority": 100
      }
    ],
    "contributes": {
      "settings": {
        "namespace": "@alice/lmsensors",
        "schema": {}
      }
    }
  }
}
```

The `mesh.kind` value describes the module's main role:

| Kind            | Purpose                                                                 |
| --------------- | ----------------------------------------------------------------------- |
| `interface`     | Declares a named contract, types, methods, events, and shared settings. |
| `backend`       | Provides one or more interfaces.                                        |
| `frontend`      | Contributes `.mesh` UI entrypoints, widgets, surfaces, or settings UI.  |
| `theme`         | Contributes root theme tokens, component defaults, and mode files.      |
| `icon-pack`     | Contributes icons, usually as a multi-active `mesh.icons` provider.     |
| `font-pack`     | Contributes fonts.                                                      |
| `language-pack` | Contributes translations.                                               |
| `library`       | Contributes importable Luau modules.                                    |

`library` is the missing piece for extensible scripting. It is not a service
provider and does not render UI; it contributes files that other modules can
import.

`mesh.i18n.supportedLocales` declares the locales a module can support so an
installer can choose which language assets to fetch or enable with the module.
`mesh.i18n.defaultLocale` is the module's own fallback locale and should be
included in `supportedLocales`. Bundled translation files are still listed
under `mesh.contributes.i18n`; supported locales are install metadata, while
contributions are concrete files available in this package.

### Keybind Contributions

Modules declare keybind actions in `mesh.keybinds`. Each action can include
`label`, `description`, `category`, a default `trigger`, and
`localizedTriggers` for locale-specific defaults. The installed graph keybind contributions
preserve the action id, default trigger, and localized triggers so later
dispatch, conflict, and accessibility phases can inspect the complete
declaration without re-reading manifests.

Focused-surface keybinds are semantic actions, not global hotkeys. A rendered
control subscribes by setting `keybind` to the declared action id and providing
an `onkeybind` handler. The shell resolves the effective binding from user
override, exact locale access key, parent locale access key, then generic
trigger. Locale defaults apply to `access_key` declarations; shortcut
declarations keep their generic shortcut unless a user override exists.

Invalid declarations, duplicate effective bindings, unresolved overrides,
missing runtime subscribers, and unsafe overrides are reported through
non-fatal component diagnostics. Resolved bindings are also exposed as
accessibility keyboard shortcut metadata on subscribed controls and as
structured `mesh.debug.keybinds` entries for debug consumers.

## Interface Modules

Interface modules are the stabilizing layer. A contract should define the
portable minimum, not every feature a provider could ever expose.

Recommended layout:

```text
@mesh/audio-interface/
  module.json
  interface.toml
  settings.schema.json
  lib/
    audio_types.luau
```

Recommended `module.json`:

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
          }
        }
      },
      "libraries": [
        {
          "namespace": "@mesh/audio-interface",
          "path": "lib/"
        }
      ]
    }
  }
}
```

Interface contracts should separate:

- **State fields:** readable values exposed through the proxy.
- **Methods:** command calls routed to the active provider.
- **Events:** typed channels owned by the active provider.
- **Types:** shared structs used by state, methods, and events.
- **Capabilities:** access required to consume or implement the contract.
- **Shared settings:** user preferences that survive provider swaps.

Do not put provider identity such as `source_module` in the contract state.
That is runtime metadata.

### Interface Relationship Metadata

MESH is open: any module author can create their own interface. The core should
not block independent interfaces just because a base module already exists.
Instead, interface packages describe how they relate to the ecosystem so tools
can encourage reuse where it helps interoperability.

Use `mesh.interface.domain` to group related interfaces, and
`mesh.interface.relationship` to explain intent:

| Relationship  | Meaning                                                                                                            |
| ------------- | ------------------------------------------------------------------------------------------------------------------ |
| `base`        | A broad shared contract for a domain, such as `mesh.audio`.                                                        |
| `extension`   | Extra surface area that builds on another interface.                                                               |
| `independent` | A separate model in the same or a new domain. Allowed, but less interoperable unless frontends target it directly. |

An audio extension can be explicit:

```json
{
  "name": "@alice/audio-streams-interface",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "interface",
    "interface": {
      "name": "alice.audio-streams",
      "version": "1.0",
      "file": "interface.toml",
      "domain": "audio",
      "extends": "mesh.audio",
      "relationship": "extension"
    }
  }
}
```

An independent audio interface is also valid:

```json
{
  "name": "@alice/audio-graph-interface",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "interface",
    "interface": {
      "name": "alice.audio-graph",
      "version": "1.0",
      "file": "interface.toml",
      "domain": "audio",
      "relationship": "independent",
      "reason": "experimental graph-first routing model"
    }
  }
}
```

When an enabled independent interface shares a domain with a base interface,
the core records soft guidance such as "consider extending `mesh.audio`." This
is not a load error. It is discoverability pressure: base modules should
provide the common state and commands, while independent interfaces remain
available for genuinely different models.

## Backend Workflow

Backend authors should write adapters, not standalone ecosystems.

1. Pick or create an interface module.
2. Depend on the interface module.
3. Depend on any reusable backend libraries.
4. Request capabilities and system dependencies.
5. Implement `init()`, optional `on_poll()`, and `on_command_<method>()`.
6. Export `state` or call `mesh.service.emit(...)`.
7. Return command results as `{ ok = true }` or `{ ok = false, error = "..." }`.

Backend capabilities are permissions for host APIs, not a restatement of the
interface being implemented. An audio backend should not request
`service.audio.read` or `service.audio.control` just because it provides
`mesh.audio`; those are consumer permissions for frontends or automation that
read audio state or publish audio commands. A provider uses `implements` to
declare the interface it implements, then requests only the generic host powers
it needs, such as `exec.wpctl`, `exec.pactl`, `exec.aplay`, `dbus.system`, or
`net.http`.

Example:

```luau
local poll = require("@mesh/backend-kit/poll")
local result = require("@mesh/backend-kit/result")

state = {
    available = false,
    percent = 0,
    muted = false,
}

function init()
    mesh.service.set_poll_interval(500)
end

function on_poll()
    local out = mesh.exec("wpctl", { "get-volume", "@DEFAULT_AUDIO_SINK@" })
    if not out.success then
        mesh.service.emit_unavailable()
        return
    end

    state.available = true
    state.percent = parse_percent(out.stdout)
    state.muted = string.find(out.stdout, "MUTED") ~= nil
end

function on_command_set_volume()
    local payload = mesh.service.payload()
    local volume = tostring(payload.volume or 0)
    local out = mesh.exec("wpctl", { "set-volume", "@DEFAULT_AUDIO_SINK@", volume })
    return result.from_exec(out)
end
```

The core should validate that emitted state and command handlers match the
interface contract. The backend should focus on translating the system into
that contract.

## Frontend Workflow

Frontend authors should consume interfaces and library helpers:

```luau
local fmt = require("@mesh/ui-kit/format")

local ok, audio = pcall(require, "mesh.audio@>=1.0")
if not ok then audio = nil end

volume_label = "N/A"
volume_icon = "audio-volume-muted"

function onRender()
    if not audio then
        volume_label = "N/A"
        volume_icon = "audio-volume-muted"
        return
    end

    local percent = audio.percent or 0
    local muted = audio.muted or false
    volume_label = fmt.percent(percent)
    volume_icon = fmt.audio_icon(percent, muted)
end

function onVolumeUp()
    if audio then
        audio.volume_up()
    end
end
```

Rules:

- Require interfaces by contract name, never backend module ID.
- Use `pcall(require, ...)` for optional services.
- Keep display derivation in the frontend script.
- Use libraries for formatting and common UI behavior.
- Publish shell events with `mesh.events`; mutate services with proxy methods.

### Frontend Theme Contributions

Frontend modules may declare a `mesh.theme` block in their manifest. This is
the module-owned theme contribution that Mesh validates and installs under the
active theme file's `modules.<module-id>` subtree.

Example:

```json
{
  "name": "@mesh/weather",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "theme": {
      "tokens": {
        "weather.color.sunny": "#F6B73C",
        "weather.color.rainy": "#5B8DEF"
      },
      "defaults": {
        "components": {
          "base": {
            "transition": "background-color token(animation.duration.fast) token(animation.curves.bezier.standard)"
          },
          "button": {
            "border-radius": "token(radius.md)"
          },
          "weather-chip": {
            "background": "token(@mesh/weather.weather.color.sunny)"
          }
        }
      }
    }
  }
}
```

Rules:

- `mesh.theme.tokens` defines module-owned token defaults.
- `mesh.theme.defaults.components.base` is subtree-scoped to that module.
- `mesh.theme.defaults.components.button` overrides the core primitive inside
  that module subtree only.
- custom component keys such as `weather-chip` are module-local component
  defaults.
- module contributions are not theme-variant-specific in v1.
- invalid token names, invalid style properties, or unresolved explicit token
  references block installation.

Cross-module token usage must be explicit:

```css
background: token(@mesh/weather.weather.color.sunny);
```

On installation, Mesh writes the contribution into the active authored theme
file under:

```json
{
  "modules": {
    "@mesh/weather": {
      "tokens": {},
      "defaults": {
        "components": {}
      }
    }
  }
}
```

On uninstall, Mesh removes that subtree. Any remaining references from other
modules become unresolved token warnings until they are fixed or removed.

## Luau Library Modules

Library modules are regular modules with `mesh.kind = "library"`.

Recommended manifest:

```json
{
  "name": "@mesh/backend-kit",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "library",
    "contributes": {
      "libraries": [
        {
          "namespace": "@mesh/backend-kit",
          "path": "lib/"
        }
      ]
    }
  }
}
```

Recommended layout:

```text
@mesh/backend-kit/
  module.json
  lib/
    result.luau
    poll.luau
    dbus.luau
    process.luau
    json.luau
```

Import paths should be stable and package-qualified:

```luau
local result = require("@mesh/backend-kit/result")
local color = require("@mesh/ui-kit/color")
local audio_types = require("@mesh/audio-interface/audio_types")
```

Libraries may wrap host APIs, but host APIs remain generic:

- Good: `@mesh/backend-kit/process.luau` wraps `mesh.exec`.
- Good: `@mesh/dbus-kit` wraps generic D-Bus host calls when those exist.
- Bad: Rust core adds `mesh.audio.get_volume()`.

This keeps new system integrations possible without core-specific service
branches.

## Dependency Vocabulary

Use kinded dependencies inside `mesh.dependencies`:

```json
{
  "mesh": {
    "dependencies": {
      "modules": {
        "@mesh/audio-interface": ">=1.0.0, <2.0.0",
        "@mesh/backend-kit": ">=0.1.0"
      },
      "backend": {
        "mesh.audio": ">=1.0"
      },
      "themes": {
        "@mesh/shell-theme": ">=0.1.0"
      },
      "icons": {
        "@mesh/material-icons": ">=0.1.0"
      }
    }
  }
}
```

Interpretation:

- `modules` means package-level dependency.
- `backend` means "I need a provider for this interface."
- `themes`, `icons`, `fonts`, and `i18n` are resource dependencies.
- System dependencies such as binaries and native libraries should remain
  detected, not installed.

## Provider Selection

Multiple backend modules can implement the same interface. The active provider
is selected by the root package graph:

```json
{
  "providers": {
    "mesh.audio": "@mesh/pipewire-audio"
  }
}
```

The graph should:

- keep all installed providers visible,
- validate that the selected module is enabled and implements the interface,
- use priority only as an initial default,
- surface missing or failed providers through health diagnostics,
- preserve contract-level settings across provider swaps.

The installed graph also exposes non-fatal compatibility diagnostics for
resource and settings contribution mismatches. Missing icon/font/language/theme
packs, required semantic icons that no enabled icon pack maps, and duplicate
settings namespaces should be visible to tools and settings UI without blocking
unrelated modules from loading.

## Extending Existing Interfaces

Creating a new interface is always allowed. The ecosystem should still make
the shared path attractive:

1. Use the base interface when it already has the common state and commands.
2. Add optional fields, methods, events, or capabilities to the base interface
   when a feature is broadly useful.
3. Create an `extension` interface when the feature is related but large enough
   to be its own contract.
4. Create an `independent` interface when the model is intentionally different
   or experimental.

Example: basic volume and mute belong in `mesh.audio`. Per-app audio volume
could be an extension such as `mesh.audio.streams` or
`alice.audio-streams` extending `mesh.audio`. A graph-first PipeWire router can
be an independent audio interface if it intentionally exposes a different
model.

## Migration Recommendations

1. Treat `module.json` plus `mesh` as the target manifest.
2. Treat `package.json`, legacy `module.json` with `id/type/api_version`, and
   `mesh.toml` as internal migration inputs only.
3. Replace old public names with the canonical vocabulary in diagnostics, docs,
   tests, and examples. Do not describe old manifest names as interchangeable
   with `module.json`.
4. Add a library resolver to both backend and frontend Luau runtimes.
5. Move common backend helpers out of individual providers into
   `@mesh/backend-kit`.
6. Move common frontend helpers into `@mesh/ui-kit`.
7. Generate Luau type metadata from `interface.toml` so LSP completion comes
   from the contract, not by analyzing whichever backend is installed.

The end state is coherent: users create modules, modules compose through
interfaces and libraries, and the Rust core remains a generic runtime.
