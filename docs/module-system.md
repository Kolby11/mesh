# Module System

MESH modules should feel easy to author without letting the ecosystem turn
into one-off backend/frontend pairs. The core rule is:

> A module is the installable MESH unit. An interface is the contract. A
> provider implements the contract. A frontend consumes the contract. Shared
> Luau libraries hold reusable implementation patterns.

This gives users one workflow whether they are building UI, a backend service,
a theme, an icon pack, or a shared library.

Canonical vocabulary lives in [MESH Module Vocabulary](module-vocabulary.md).
`module.json` is the only supported author-facing manifest name. Historical
`package.json`, `mesh.toml`, and old top-level `id/type/api_version`
`module.json` manifests are rejected with migration diagnostics.

## Principles

1. **One module model.** Every installable thing is a module with
   canonical `module.json`.
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
   APIs should use normal Luau object surfaces. In the v1.14 authoring model,
   `local` members are private, non-local variables/functions are public
   object members, `self` is the current instance context, and `require(...)`
   imports external services, libraries, and component definitions. Rust still
   owns routing, validation, replay, permissions, lifecycle, and diagnostics
   underneath that syntax.

## Luau Authoring Model

MESH separates the current runtime instance from external dependencies:

```luau
local audio = require("mesh.audio@>=1.0")

local private_cache = {}

volume = 0

function increase_volume(amount)
  volume = volume + amount
  audio.set_volume(volume)
end

function render(self)
  local id = self.meta.id
  local last_volume = self.storage.last_volume
end
```

- `local` variables and functions are private to the script.
- Non-local variables and functions are public members of the module,
  component instance, or backend provider object.
- `self.meta` exposes identity and diagnostic context for the current
  frontend component or backend provider instance.
- `self.storage` exposes shell-backed persistent storage scoped to the current
  module/component identity.
- `require(...)` is for external dependencies: shell APIs, service/interface
  proxies, Luau libraries, and frontend component definitions.

### Persistent `self.storage`

`self.storage` is a small JSON-like document owned by the shell and scoped to
the current frontend component instance or backend provider instance. Use it
for durable preferences and provider-local state that should survive runtime
recreation:

```luau
function init(self)
  local saved = self.storage.language
  if saved == "en" or saved == "sk" then
    mesh.locale.set(saved)
  end
end

function onSelectSlovak()
  self.storage.language = "sk"
  mesh.locale.set("sk")
end
```

Supported values are `nil`, booleans, numbers, strings, arrays, and plain
objects. Assigning `nil` removes a key. `self.storage:snapshot()` returns a
plain table copy of the current document. Functions, userdata, component
definitions, component instances, event channels, and other non-serializable
values are rejected with non-fatal diagnostics.

Storage loads before lifecycle user code runs. Frontend component storage
flushes on `unmount`; backend provider storage flushes on `stop`; the shell can
also call the explicit flush path during orderly shutdown. Writes are coalesced
in memory until a flush point, and persistence failures preserve the in-memory
value while emitting diagnostics. Frontend reads during `render(self)` are
tracked by key, so writes to watched keys rerender only the affected component;
writes to unwatched keys do not trigger unrelated rerenders.

For frontend components, `require("./Component")` returns a component
definition. Markup instantiates it, and `bind:this` exposes the mounted
instance:

```xml
<AudioSlider device_id="{active_device}" bind:this={audio_slider} />
```

The attribute `device_id` becomes a public field on that mounted instance.
The bound `audio_slider` reference exposes public fields and functions such as
`audio_slider.volume` and `audio_slider.increase_volume(10)`.

Older docs and shipped code may mention `module.state`, `module.exports`, or
`module.events`. Those names describe the v1.12 runtime lanes and remain useful
as compatibility/internal vocabulary, but new author-facing syntax should use
the `self` plus Lua public/private member model above.

## Module Manifest Shape

Use `module.json` for every new module:

`module.json` is the author-facing manifest. Top-level fields should identify
the module and its release metadata, such as `name`, `version`, `description`,
`private`, `license`, and `repository`. All MESH-specific fields live under
`mesh`.

- Do not use top-level `type` for module kind; use `mesh.kind`.
- Do not use top-level `id`; use npm's top-level `name`.
- Do not put MESH dependency objects in top-level `dependencies`; use
  `mesh.uses`.
- Do not put capabilities, providers, entrypoints, settings, themes, or binary
  requirements at the top level.
- Use `mesh.uses` for dependencies, resource requirements, binaries, and
  capabilities.
- Use `mesh.provides` for layout entries, settings schemas, i18n catalogs,
  libraries, and concrete resource contributions.
- Use `mesh.implements` only for backend provider implementations of
  interface contracts.

Package managers can be used as development/distribution tooling around these
files. They are not the authority for MESH behavior. MESH reads the
`mesh` section, validates capabilities and native requirements, resolves
interface providers, and decides which modules are enabled.

Minimal frontend:

```json
{
  "name": "@alice/panel",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "entry": "src/main.mesh",
    "uses": {
      "interfaces": { "mesh.audio": ">=1.0" },
      "resources": { "icons": ["@mesh/icons-default"] },
      "capabilities": ["shell.surface", "service.audio.read"]
    },
    "provides": {
      "settings": { "namespace": "@alice/panel", "schema": {} }
    }
  }
}
```

`mesh.entry` fills `entrypoints.main`. For simple frontend modules it also
creates a default `main` layout contribution unless `mesh.provides.layout`
already lists explicit entries.

## Migration Diagnostics

Old manifest file names and top-level manifest shapes are replacement or
removal targets, not public author-facing aliases. They are no longer accepted
as compatibility inputs.

| Input | Severity | Author action | Runtime behavior |
| ---- | ------- | ------------ | --------------- |
| `package.json` | error | rename package.json to module.json and use canonical `name/version/mesh` | Fails manifest loading. |
| legacy `module.json` with `id/type/api_version` | error | replace legacy fields with `name`, `version`, and `mesh` | Fails manifest loading. |
| `mesh.toml` | error | replace mesh.toml with canonical module.json | Fails manifest loading. |
| `plugin.json` | error | remove plugin.json or replace it with module.json | Fails manifest loading. |
| multiple manifest files | error | keep canonical module.json and remove the old manifest file | Fails manifest loading until the ambiguous old file is removed. |

The root installed-module graph follows the same rule. `config/module.json`
uses the root graph shape directly because it is not an installable module.

The installed set is **auto-discovered** from `modulesDir`: when the root graph
lists no `modules`, the loader scans the directory for `module.json` files and
builds the installed set from each module's own manifest (which already declares
its `name` and `kind`). The root file then holds **decisions only** — which
modules are `disabled`, the active `providers`, the layout `entrypoint`, and the
active `theme`. A discovered module is enabled unless named in `disabled`, and a
single-implementer interface needs no `providers` entry (it is auto-selected):

```json
{
  "schemaVersion": 1,
  "modulesDir": "../modules",
  "disabled": ["@mesh/text-selection-proof", "@mesh/debug-inspector"],
  "providers": {
    "mesh.audio": "@mesh/pipewire-audio"
  },
  "layout": {
    "entrypoint": "@mesh/navigation-bar:main"
  }
}
```

An explicit `modules` map is still honored for full manual control (each entry
gives `kind`, `path`, `enabled`); when present, auto-discovery is skipped and the
`disabled` list does not apply. The decisions-only form above is preferred.

## Author Workflow Examples

These examples show the canonical authoring shape. Runtime structs still keep
some older internal names so shipped behavior can migrate incrementally, but
new manifests should use `mesh.uses`, `mesh.provides`, and `mesh.implements`.

### Frontend Surface

Frontend modules contribute UI entrypoints and consume interface contracts.
They declare component-module imports, interfaces, resource packs, capabilities,
settings, i18n catalogs, icon requirements, keybinds, and layout entries in one
manifest:

```json
{
  "name": "@alice/volume-panel",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "entry": "src/main.mesh",
    "uses": {
      "modules": { "@alice/volume-popover": ">=0.1.0" },
      "interfaces": { "mesh.audio": ">=1.0" },
      "resources": { "icons": ["@mesh/icons-default"] },
      "capabilities": ["shell.surface", "service.audio.read", "service.audio.control"],
      "iconRequirements": {
        "required": ["audio-volume-muted", "audio-volume-high"]
      }
    },
    "i18n": {
      "defaultLocale": "en",
      "supportedLocales": ["en"]
    },
    "keybinds": {
      "mute": {
        "label": { "t": "keybind.mute.label", "fallback": "Mute audio" },
        "trigger": { "kind": "shortcut", "key": "m" }
      }
    },
    "provides": {
      "layout": [
        { "id": "main", "entrypoint": "src/main.mesh", "label": "Volume Panel" }
      ],
      "settings": { "namespace": "@alice/volume-panel", "schema": {} },
      "i18n": [
        { "id": "en", "locale": "en", "path": "config/i18n/en.json" }
      ]
    },
    "surface": {
      "anchor": "top",
      "height": 56,
      "size": "fixed"
    },
    "accessibility": { "role": "toolbar" }
  }
}
```

`require("@alice/volume-popover")` must be declared in `mesh.uses.modules`.
`require("mesh.audio@>=1.0")` must be declared in `mesh.uses.interfaces`.
Optional contracts use `mesh.uses.optionalInterfaces` and should be imported
with `pcall(require, ...)` in Luau.
The manifest validator keeps these buckets separate: module/resource
dependencies must be module ids like `@scope/name`, interface dependencies must
be dotted contract names like `mesh.audio`, and capabilities must be host-power
names like `service.audio.read` or `exec.hyprctl`.

Frontend modules with a main `.mesh` entrypoint also form a surface contract.
The installed graph records that contract as one typed frontend surface record:
main entrypoint, optional settings namespace, accessibility role/label, and
surface sizing policy. If an enabled frontend declares a main entrypoint but
omits `mesh.surface` or `mesh.accessibility`, graph diagnostics emit
`missing_frontend_surface_layout` or `missing_frontend_accessibility` so module
authors see the incomplete surface metadata before settings/debug UI has to
guess.

#### Surface configuration (`mesh.surface`)

Core ships the canonical surface schema and its defaults; a frontend declares
**only the fields it wants to override** in one compact `mesh.surface` block.
There is no need to hand-write a `settings.schema.surface` properties block or a
separate `mesh.surfaceLayout` section — both are replaced by `mesh.surface`.

```json
"surface": {
  "anchor": "top",
  "layer": "top",
  "width": 0,
  "height": 56,
  "exclusive_zone": 56,
  "keyboard_mode": "none",
  "visible_on_start": true,
  "size": "fixed"
}
```

Fields split by audience, but they live in one place:

- **User-editable defaults:** `anchor`, `layer`, `width`, `height`,
  `exclusive_zone`, `keyboard_mode`, `visible_on_start`, `margins`,
  `display_transition`. The shell can generate settings UI for these from the
  core base schema, and user `config/settings.json` `surface.*` overrides apply
  on top.
- **Renderer policy (not user-editable):** `size` (`fixed` |
  `content_measured`), `prefers_content_children_sizing`, and the
  `min_*`/`max_*` clamps.

Any field the author omits falls back to the core default. `mesh.surfaceLayout`
remains accepted as a legacy alias so older manifests still parse, but new
modules should use `mesh.surface`.

### Backend Provider

Backend providers implement interfaces and declare native runtime health inputs
as dependencies:

```json
{
  "name": "@alice/lmsensors",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "backend",
    "entry": "src/main.luau",
    "uses": {
      "modules": { "@alice/thermal-interface": ">=1.0.0" },
      "capabilities": ["exec.sensors"],
      "binaries": [
        {
          "name": "sensors",
          "reason": "Read thermal sensor data from lm-sensors.",
          "packages": { "debian": "lm-sensors", "arch": "lm_sensors" }
        }
      ]
    },
    "implements": [
      {
        "interface": "alice.thermal",
        "version": "1.0",
        "baseModule": "@alice/thermal-interface",
        "provider": "lmsensors",
        "label": "lm-sensors",
        "priority": 100
      }
    ],
    "provides": {
      "settings": { "namespace": "@alice/lmsensors", "schema": {} }
    }
  }
}
```

Required binaries missing from `PATH` produce graph diagnostics. Optional
binaries should set `"optional": true`; they are health hints, not load
blockers.

### Interface Module

Interface modules are data-only contract packages:

```json
{
  "name": "@alice/thermal-interface",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "interface",
    "interface": {
      "name": "alice.thermal",
      "version": "1.0",
      "file": "interface.toml",
      "domain": "thermal",
      "relationship": "base"
    },
    "provides": {
      "settings": { "namespace": "alice.thermal", "schema": {} }
    }
  }
}
```

The graph reports `missing_interface_contract_file` if the declared contract
file is absent.

`mesh.interface.file` is **optional** for v0. An interface module may ship only
`name`/`version`/`domain` and let the contract be inferred from the provider's
emitted state; contract-based validation (capabilities, events) applies only
once a contract file exists. This also means a backend can implement an
interface with no separate interface module at all — declare the interface name
in `mesh.implements` (no `baseModule`), and the sole-implementer auto-selection
makes it the active provider. Promote it to a full interface module with a
contract file once it is worth sharing and stabilizing.

### Library Module

Libraries contribute importable Luau code. They must not declare required
capabilities because consuming modules request host power themselves:

```json
{
  "name": "@alice/backend-kit",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "library",
    "provides": {
      "libraries": [
        { "namespace": "@alice/backend-kit", "path": "lib/" }
      ]
    }
  }
}
```

Consumers declare the library under `mesh.uses.modules` and import files with
`require("@alice/backend-kit/result")` or the module namespace pattern exposed
by that library.

### Icon Pack

Icon packs are ordinary modules that map semantic icon names to concrete theme
assets. They must use `mesh.kind: "icon-pack"`; other module kinds cannot
declare `mesh.icon_pack`.

```json
{
  "name": "@alice/icons",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "icon-pack",
    "icon_pack": {
      "id": "alice",
      "mappings": {
        "audio-volume-high": "hicolor/audio-volume-high",
        "battery-full": "hicolor/battery-full"
      }
    }
  }
}
```

Frontend modules depend on icon packs through `mesh.uses.resources.icons` and
declare semantic names through `mesh.uses.iconRequirements` or
`mesh.iconRequirements`. Required names are a hard authoring contract:
`missing_required_icon` is emitted when no enabled icon pack maps the semantic
name. Optional names are still checked and reported as `missing_optional_icon`
so settings/debug UI can explain degraded affordances without treating the
module as incomplete.

Multiple icon-pack modules can be installed and enabled at once. Frontend
modules name the packs they prefer by module id, while user settings can choose
or reorder the effective pack chain without frontend code importing a concrete
theme package.

### Language Pack

Language packs contribute concrete translation catalogs:

```json
{
  "name": "@alice/sk-language",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "language-pack",
    "provides": {
      "i18n": [
        { "id": "sk", "locale": "sk", "path": "i18n/sk.json" }
      ]
    }
  }
}
```

Font packs, theme packs, and language packs follow the same ordinary-module
pattern as icon packs: install the module, enable it in the root graph, and let
consumers depend on the semantic resource id instead of importing files from a
concrete package. Pack-specific contribution fields are kind-scoped:
`mesh.provides.fonts` belongs to `font-pack` modules,
`mesh.provides.themes` belongs to `theme` modules, and `mesh.provides.icons` /
`mesh.icon_pack` belong to `icon-pack` modules. Bundled `mesh.provides.i18n`
catalogs remain valid on normal modules for module-local translations; a
standalone `language-pack` uses the same catalog contribution shape.

Module-owned bundled catalogs can also be listed in the producing frontend's
or interface's `mesh.provides.i18n` block. `mesh.i18n.defaultLocale` controls
the fallback catalog used for diagnostics.

### Theme Pack

Theme packs contribute theme modes:

```json
{
  "name": "@alice/theme",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "theme",
    "provides": {
      "themes": [
        {
          "id": "alice",
          "label": "Alice",
          "default_mode": "dark",
          "modes": {
            "dark": "themes/dark/theme.css",
            "light": "themes/light/theme.css"
          }
        }
      ]
    }
  }
}
```

Theme mode paths point at package-relative CSS theme entries. Future
resource-pack kinds should follow the same `mesh.uses.resources.*` and
`mesh.provides.*` pattern.

## Extend or Add a MESH Module

Use the shipped audio/navigation path as the authoring model:

1. Start with a canonical `module.json` and put all MESH behavior under
   `mesh`.
2. Make the UI a frontend module. `@mesh/navigation-bar` declares
   `mesh.kind: "frontend"`, uses interface/resource dependencies, contributes
   its `main` layout entrypoint, and declares `mesh.keybinds.mute` plus icon
   requirements.
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
8. Put dependencies and host powers in `mesh.uses`; put layout, settings,
   i18n, libraries, and concrete resources in `mesh.provides`. The installed
   graph preserves those records so the shell can apply user overrides and
   validate module gaps without re-reading arbitrary source files.
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
    "entry": "src/main.luau",
    "uses": {
      "modules": {
        "@alice/thermal-interface": ">=1.0.0, <2.0.0",
        "@mesh/backend-kit": ">=0.1.0"
      },
      "capabilities": ["exec.sensors"],
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
        "version": "1.0",
        "baseModule": "@alice/thermal-interface",
        "provider": "lmsensors",
        "label": "lm-sensors",
        "priority": 100
      }
    ],
    "provides": {
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
| -------------- | ---------------------------------------------------------------------- |
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
included in `supportedLocales`. Bundled translation files are listed under
`mesh.provides.i18n`; supported locales are install metadata, while
provided catalogs are concrete files available in this package.

### Frontend Surface Contracts

Frontend authoring remains one `module.json` contract, not a separate settings
file per concern. Use `mesh.entry` or `mesh.entrypoints.main` for the `.mesh`
surface entrypoint, `mesh.provides.settings` for the settings schema namespace,
`mesh.surfaceLayout` for non-user sizing/layout policy, and
`mesh.accessibility` for the root role and label. The graph keeps these fields
as a single frontend surface record while preserving the existing typed indexes
for interfaces, icon requirements, i18n catalogs, keybinds, settings schemas,
and component-module dependencies.

`mesh.surfaceLayout.keyboard_mode` declares the module default keyboard
interactivity policy: `none`, `on_demand`, or `exclusive`. User settings may
override that default per module, and temporary runtime focus transfers may
override it while a popover owns focus. The durable manifest/settings contract
describes policy only; the shell remains the owner of current keyboard focus
state.

### Keybind Contributions

Modules declare keybind actions in `mesh.keybinds`. Each action can include
`label`, `description`, `category`, a default `trigger`, and
`localizedTriggers` for locale-specific defaults. The installed graph keybind contributions
preserve the action id, default trigger, and localized triggers so later
dispatch, conflict, and accessibility phases can inspect the complete
declaration without re-reading manifests.

Raw strings in manifest text fields are literals. A value such as
`"label": "keybind.mute.label"` displays as that exact text and should be
treated as a migration mistake if the author meant to localize it. Use a
field-local localized text object when the field should resolve through the
module's catalogs:

```json
{
  "mesh": {
    "i18n": {
      "defaultLocale": "en",
      "supportedLocales": ["en", "sk"]
    },
    "keybinds": {
      "mute": {
        "label": { "t": "keybind.mute.label", "fallback": "Mute audio" },
        "description": {
          "t": "keybind.mute.description",
          "fallback": "Toggle audio mute"
        },
        "category": { "t": "keybind.category.audio", "fallback": "Audio" },
        "trigger": { "kind": "shortcut", "key": "m" }
      }
    },
    "provides": {
      "i18n": [
        { "id": "en", "locale": "en", "path": "config/i18n/en.json" },
        { "id": "sk", "locale": "sk", "path": "config/i18n/sk.json" }
      ]
    }
  }
}
```

The `t` value is resolved in the declaring module's i18n namespace. The
`fallback` value is required and is used when the active locale and fallback
locale do not provide the key. Missing keys are non-fatal diagnostics that
include the module id, field path, key, and fallback.

Use `mesh.i18n` to declare the module's locale support and fallback locale.
Use `mesh.provides.i18n` to list bundled catalog files. Use field-local
localized text objects only for the fields that should be catalog-backed;
plain labels such as `"Navigation Bar"` remain valid literal strings.

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
The installed graph also scans static `.mesh` templates for keybind
subscriptions. A node using `keybind="{this.keybinds.mute.id}"` or
`keybind="mute"` must have a matching `mesh.keybinds.mute` declaration and an
`onkeybind` handler, otherwise graph diagnostics emit
`undeclared_keybind_subscription` or `keybind_subscription_missing_handler`.
Legacy `settings.json` shortcut declarations are migration-only. They can no
longer create keybind actions by themselves; the action must exist in
`mesh.keybinds`, and any legacy setting for the same id is reported as ignored
so authors migrate labels, categories, default triggers, localized triggers,
and scope into the manifest.

### Popover Focus Ownership

Open popovers through `mesh.popover.activate(surface_id, event, options)` from
the trigger control's click/key handler. Pass the original event whenever the
popover should participate in keyboard return focus: the shell extracts
`event.surface.id` and the trigger node key from `event.current` /
`event.current_target`, stores that trigger relationship, and can transfer Tab
focus into the popover. `options.focus` controls whether activation immediately
focuses the popover; omit it or set `true` for keyboard-owned popovers, and set
`false` for pointer-first popovers where the first click should land inside the
opened surface.

When focus is transferred into a popover, the shell records return focus as
`(trigger_surface, trigger_key)` and marks the popover to close on focus leave.
Modules should call `mesh.popover.hide(surface_id)` for explicit dismiss
actions, but they should not maintain their own durable focus ownership state;
the shell owns keyboard focus, return focus, and close-on-focus-leave behavior.

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
    "provides": {
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
| ------------ | ----------------------------------------------------------------------------------------------------------------- |
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

The interface contract's `[capabilities]` (`required`/`optional`) are the
**consumer** capabilities. The graph checks them against frontends that consume
the interface. It does not require providers to declare them — and if a provider
*does* declare a consumer capability for an interface it implements, the graph
emits `provider_declares_consumer_capability` with a concrete action to remove
it. This keeps capability declarations meaningful instead of drifting into
copy-pasted noise.

Example:

```luau
local poll = require("@mesh/backend-kit/poll")
local result = require("@mesh/backend-kit/result")

state = {
    available = false,
    percent = 0,
    muted = false,
}

function start(self)
    mesh.service.set_poll_interval(500)
end

function on_poll(self)
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

Providers publish typed interface events through named `self` channels:

```luau
function on_command_set_volume(self)
    self.VolumeChanged:fire({
        device_id = "default",
        level = state.percent,
    })
end
```

The core should validate that emitted state and command handlers match the
interface contract. The backend should focus on translating the system into
that contract.
Compatibility `mesh.service.emit_event("EventName", payload)` calls are still
accepted, but static event names are validated against the provider's interface
TOML. If a backend emits an event that is not declared under `[[events]]`, the
installed graph reports `undeclared_interface_event_emit`.

## Frontend Workflow

Frontend authors should consume interfaces and library helpers:

```luau
local fmt = require("@mesh/ui-kit/format")

local ok, audio = pcall(require, "mesh.audio@>=1.0")
if not ok then audio = nil end

volume_label = "N/A"
volume_icon = "audio-volume-muted"

function render(self)
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

`mesh.events.publish("shell.*", ...)` is reserved for shell-owned commands such
as positioning, popovers, debug toggles, and theme selection. Interface-domain
commands must go through the required interface proxy, for example
`wm.switch_workspace(1)` rather than publishing
`mesh.hyprland.switch_workspace`. Static `.mesh` sources that publish
`mesh.*` channels now receive `raw_interface_domain_event_publish` graph
diagnostics.

Declared shell-owned channels are:

- `shell.show-surface`, `shell.hide-surface`, `shell.toggle-surface`
- `shell.position-surface`, `shell.activate-popover`
- `shell.set-theme`, `shell.set-locale`
- `shell.toggle-debug-overlay`, `shell.toggle-debug-layout-bounds`,
  `shell.toggle-debug-profiling`, `shell.run-debug-benchmark`
- `shell.brightness-down`, `shell.brightness-up`, `shell.set-brightness`
- `shell.toggle-calendar`

Static `.mesh` sources that publish another `shell.*` channel receive
`unknown_shell_event_publish` until the shell namespace is extended
deliberately.

### Frontend Theme Contributions

Frontend modules may declare a `mesh.theme` block in their manifest. This is
the module-owned theme contribution that Mesh validates and installs under an
explicit module scope for the active theme package.

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
            "transition": "background-color var(--animation-duration-fast) var(--animation-curves-bezier-standard)"
          },
          "button": {
            "border-radius": "var(--radius-md)"
          },
          "weather-chip": {
            "background": "var(--weather-color-sunny)"
          }
        }
      }
    }
  }
}
```

Rules:

- `mesh.theme.tokens` defines module-owned token defaults.
- `mesh.theme.defaults.components.base` maps to the module-scoped `node` rule.
- `mesh.theme.defaults.components.button` overrides the core primitive inside
  that module subtree only.
- custom component keys such as `weather-chip` are module-local component
  defaults.
- module contributions are not theme-variant-specific in v1.
- invalid token names, invalid style properties, or unresolved theme variable
  references block installation.

Module-scoped theme variable usage must stay inside the module scope:

```css
background: var(--weather-color-sunny);
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
    "provides": {
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

Use kinded dependencies inside `mesh.uses`:

```json
{
  "mesh": {
    "uses": {
      "modules": {
        "@mesh/audio-interface": ">=1.0.0, <2.0.0",
        "@mesh/backend-kit": ">=0.1.0"
      },
      "interfaces": {
        "mesh.audio": ">=1.0"
      },
      "resources": {
        "themes": ["@mesh/shell-theme"],
        "icons": ["@mesh/material-icons"]
      }
    }
  }
}
```

Interpretation:

- `modules` means package-level dependency.
- `interfaces` means "I need a provider for this interface."
- `optionalInterfaces` means "I can use this interface when available."
- `resources.themes`, `resources.icons`, `resources.fonts`, and
  `resources.i18n` are resource dependencies.
- System dependencies such as binaries and native libraries should remain
  detected, not installed; declare runtime binaries in `mesh.uses.binaries`.

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

When exactly one enabled backend implements an interface, the graph
**auto-selects it** — the root graph only needs a `providers` entry for
interfaces with more than one implementer (where the choice is genuinely the
user's). In the shipped graph, `mesh.audio` has two providers (PipeWire and
PulseAudio) and is selected explicitly, while single-provider interfaces such as
`mesh.power` and `mesh.hyprland` are resolved automatically and need no entry.

The graph should:

- keep all installed providers visible,
- validate that the selected module is enabled and implements the interface,
- auto-select the sole implementer when the root graph names none,
- require an explicit choice when several modules implement one interface,
- surface missing or failed providers through health diagnostics,
- preserve contract-level settings across provider swaps.

The installed graph also exposes non-fatal compatibility diagnostics for
resource and settings contribution mismatches. Missing icon/font/language/theme
packs, required or optional semantic icons that no enabled icon pack maps, and
duplicate settings namespaces should be visible to tools and settings UI without
blocking unrelated modules from loading.

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
   `mesh.toml` as rejected historical inputs that must be replaced.
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
