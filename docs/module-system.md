# Module System

MESH modules should feel easy to author without letting the ecosystem turn
into one-off backend/frontend pairs. The core rule is:

> A module is a package. An interface is the contract. A provider implements
> the contract. A frontend consumes the contract. Shared Luau libraries hold
> reusable implementation patterns.

This gives users one workflow whether they are building UI, a backend service,
a theme, an icon pack, or a shared library.

## Principles

1. **One package model.** Every installable thing is a module with
   `package.json` and a `mesh` section. Legacy `module.json`, `plugin.json`,
   and `mesh.toml` can remain loaders during migration, but new docs and
   examples should use `package.json`.
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

## Package Shape

Use `package.json` for every new module:

`package.json` is intentionally npm-compatible. Top-level fields should remain
normal package fields such as `name`, `version`, `description`, `private`,
`license`, and `repository`. All MESH-specific fields must live under `mesh`.
This avoids editor warnings from the standard npm schema:

- Do not use top-level `type` for module kind; use `mesh.kind`.
- Do not use top-level `id`; use npm's top-level `name`.
- Do not put MESH dependency objects in top-level `dependencies`; use
  `mesh.dependencies`.
- Do not put capabilities, providers, entrypoints, settings, themes, or binary
  requirements at the top level.

npm, pnpm, Yarn, or Bun can be used as development/distribution tooling around
these files. They are not the authority for MESH behavior. MESH reads the
`mesh` section, validates capabilities and native requirements, resolves
interface providers, and decides which modules are enabled.

The root installed-module graph follows the same rule. `config/package.json`
uses normal package metadata at the top level and stores the active MESH module
graph under `mesh`:

```json
{
  "name": "@mesh/local-config",
  "version": "0.1.0",
  "private": true,
  "mesh": {
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
}
```

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

| Kind | Purpose |
| --- | --- |
| `interface` | Declares a named contract, types, methods, events, and shared settings. |
| `backend` | Provides one or more interfaces. |
| `frontend` | Contributes `.mesh` UI entrypoints, widgets, surfaces, or settings UI. |
| `theme` | Contributes token files. |
| `icon-pack` | Contributes icons, usually as a multi-active `mesh.icons` provider. |
| `font-pack` | Contributes fonts. |
| `language-pack` | Contributes translations. |
| `library` | Contributes importable Luau modules. |

`library` is the missing piece for extensible scripting. It is not a service
provider and does not render UI; it contributes files that other modules can
import.

`mesh.i18n.supportedLocales` declares the locales a module can support so an
installer can choose which language assets to fetch or enable with the module.
`mesh.i18n.defaultLocale` is the module's own fallback locale and should be
included in `supportedLocales`. Bundled translation files are still listed
under `mesh.contributes.i18n`; supported locales are install metadata, while
contributions are concrete files available in this package.

## Interface Modules

Interface modules are the stabilizing layer. A contract should define the
portable minimum, not every feature a provider could ever expose.

Recommended layout:

```text
@mesh/audio-interface/
  package.json
  interface.toml
  settings.schema.json
  lib/
    audio_types.luau
```

Recommended `package.json`:

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

Do not put provider identity such as `source_plugin` in the contract state.
That is runtime metadata.

### Interface Relationship Metadata

MESH is open: any module author can create their own interface. The core should
not block independent interfaces just because a base module already exists.
Instead, interface packages describe how they relate to the ecosystem so tools
can encourage reuse where it helps interoperability.

Use `mesh.interface.domain` to group related interfaces, and
`mesh.interface.relationship` to explain intent:

| Relationship | Meaning |
| --- | --- |
| `base` | A broad shared contract for a domain, such as `mesh.audio`. |
| `extension` | Extra surface area that builds on another interface. |
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

local ok, audio = pcall(require, "@mesh/audio@>=1.0")
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
  package.json
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

1. Treat `package.json` plus `mesh` as the target manifest.
2. Keep loading `module.json`, `plugin.json`, and `mesh.toml` for compatibility
   but mark them legacy in docs.
3. Rename docs from "plugin" toward "module" where they describe the package
   model. Keep "frontend plugin" and "backend plugin" only as legacy wording
   or when discussing old manifests.
4. Add a library resolver to both backend and frontend Luau runtimes.
5. Move common backend helpers out of individual providers into
   `@mesh/backend-kit`.
6. Move common frontend helpers into `@mesh/ui-kit`.
7. Generate Luau type metadata from `interface.toml` so LSP completion comes
   from the contract, not by analyzing whichever backend is installed.

The end state is coherent: users create modules, modules compose through
interfaces and libraries, and the Rust core remains a generic runtime.
