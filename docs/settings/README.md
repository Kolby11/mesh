# Settings

MESH's settings system has two goals:

1. **Extensibility** — any module can declare its own settings and the core
   will generate a UI, validate input, and apply user overrides without
   module-specific code.
2. **Portability across implementations** — a user's configuration for
   `mesh.audio` should survive swapping PipeWire for PulseAudio. Shared
   contracts carry a shared schema; backends add their own on top.

All settings are **JSON**. TOML is used for manifests and schemas; the runtime
settings themselves — defaults and user overrides — are JSON so they can be
read and written by the generated UI and inspected with standard tooling.

## Layers

Settings are resolved through an ordered stack. Later layers override earlier
ones, key by key:

```
 1. Contract default         (shipped with the interface contract package)
 2. Implementation default   (shipped with the backend/frontend module)
 3. System default            (/usr/share/mesh/settings-default.json or distro pkg)
 4. User system override      (~/.mesh/settings.json)
 5. User module override      (~/.mesh/modules/<module-id>/settings.json)
 6. Runtime override          (set at runtime via IPC / CLI, not persisted unless flushed)
```

Only layers 3–6 are user-writable. Layers 1–2 ship inside packages and are
read-only. The core composes all six into an effective view on every read.

### System-wide vs. module-wide

- **System-wide settings** apply across the whole shell (active theme,
  locale, allow-unsigned modules, module frame budgets, …). Keys live at the
  root of `~/.mesh/settings.json`.
- **Module-wide settings** belong to a single module. They live under a
  module-scoped key in the system file *or* in a dedicated per-module file
  for users who prefer that split. Both forms are valid; the core merges them.

The user override wins over the system default. A per-module file wins over
the inline module section in the system file - that's the override direction.

## File formats

### System file - `~/.mesh/settings.json`

```json
{
  "theme": {
    "active": "mesh-default-dark"
  },
  "i18n": {
    "locale": "en",
    "fallback_locale": "en"
  },
  "modules": {
    "allow_unsigned": false,
    "auto_update": false
  },
  "keyboard": {
    "button_activation_keys": ["Enter", "Space"],
    "toggle_activation_keys": ["Space", "Enter"],
    "slider_decrement_keys": ["ArrowLeft", "ArrowDown"],
    "slider_increment_keys": ["ArrowRight", "ArrowUp"],
    "surface_shortcuts": {
      "@mesh/navigation-bar": {
        "mute": { "key": "m" }
      }
    }
  },

  "interfaces": {
    "mesh.audio":   { "pin": "@mesh/pipewire-audio" },
    "mesh.network": { "pin": "@mesh/networkmanager" }
  },

  "@mesh/panel": {
    "clock_format": "24h",
    "show_seconds": false,
    "show_battery_percent": true
  }
}
```

Top-level keys that are *not* module IDs are reserved for the shell itself
(`theme`, `i18n`, `modules`, `interfaces`). Module-scoped overrides use the
module's fully qualified ID as the key.

### Per-module file - `~/.mesh/modules/<module-id>/settings.json`

```
~/.mesh/modules/@mesh/panel/settings.json
~/.mesh/modules/@community/weather-widget/settings.json
```

Each file is a flat JSON object containing that module's keys only:

```json
{
  "clock_format": "12h",
  "show_seconds": true
}
```

Per-module files are the canonical target for the generated settings UI -
writing one setting does not require the UI to rewrite the whole system
file.

### System defaults — `/usr/share/mesh/settings-default.json`

Same shape as the user system file. Distributions ship this to set sensible
defaults before any user has touched anything. The file in this repo's
`config/settings-default.json` is the project's fallback.

## Keyboard settings

Shell-owned keyboard defaults live under the top-level `keyboard` object.

- `button_activation_keys` controls which keys activate focused buttons.
- `toggle_activation_keys` controls focused switch and checkbox activation.
- `slider_decrement_keys` and `slider_increment_keys` define focused slider step keys.
- `surface_shortcuts` lets the shell remap module-declared shortcut ids on a
  per-surface basis without editing the module itself.

Example override:

```json
{
  "keyboard": {
    "surface_shortcuts": {
      "@mesh/navigation-bar": {
        "mute": { "key": "u" }
      }
    }
  }
}
```

In that example, the navigation bar still declares the `mute` shortcut, but
the shell changes its effective key from the module default to `u`.

## Keys, namespaces, and validation

Every key has exactly one owner:

- **Shell keys** (`theme.*`, `i18n.*`, `modules.*`, `interfaces.*`) — owned by
  the core. Schema lives in `mesh-core-config`.
- **Contract keys** (`mesh.audio.*`, `mesh.network.*`, …) — owned by the
  interface contract. Every implementation inherits them.
- **Module keys** (`@scope/name.*` or the module's scoped object) - owned by
  the module itself.

Each owner publishes a schema (see next section). The core validates every
value on load and on write. Invalid values are rejected, logged, and fall
through to the next layer.

Modules **cannot** write to their own settings. The user writes them through
the UI or by editing JSON; the core validates and persists. Modules read an
immutable view and subscribe to change events.

## Reusable schemas (shared contract settings)

Contract packages export a canonical settings schema that all
implementations inherit. The point: when the user swaps backends, keys
under the contract namespace survive.

```
@mesh/audio-contract/
  mesh.toml
  interface.toml
  settings.schema.json
```

```json
// settings.schema.json
{
  "default_output_priority": {
    "type": "enum",
    "values": ["speakers", "headphones", "hdmi"],
    "default": "speakers",
    "description": "Preferred output device when multiple are available."
  },
  "auto_resume_on_device_change": {
    "type": "boolean",
    "default": true
  }
}
```

These keys are addressed under the contract's name:

```json
{
  "mesh.audio": {
    "default_output_priority": "headphones",
    "auto_resume_on_device_change": true
  }
}
```

Every backend implementing `mesh.audio` reads this block. A backend may
**extend** with its own keys under its module scope:

```json
{
  "@mesh/pipewire-audio": {
    "low_latency_mode": true,
    "quantum": 1024
  }
}
```

**Swap semantics:** pinning `audio` to `@mesh/pulseaudio-audio` preserves
everything under `mesh.audio.*`. `@mesh/pipewire-audio.*` keys are kept in
the file but ignored while that backend is inactive — no reset, no data
loss. Re-pinning brings them back.

### Schema format

Schemas are JSON. The keys supported today:

| Field                 | Purpose                                                                             |
| --------------------- | ----------------------------------------------------------------------------------- |
| `type`                | `"string" \| "integer" \| "float" \| "boolean" \| "enum" \| "object" \| "array"`    |
| `default`             | Default value. Must match `type`.                                                   |
| `values`              | Allowed values when `type = "enum"`.                                                |
| `min` / `max`         | Bounds for numeric types.                                                           |
| `items`               | Element schema for arrays.                                                          |
| `properties`          | Field schemas for objects.                                                          |
| `description`         | Human-readable description, shown in the generated UI.                              |
| `scope`               | `"system"` or `"user"`. Restricts where this key may appear. Defaults to `"user"`.  |
| `requires_capability` | Declares that editing this key requires a specific capability (e.g. `theme.write`). |

## Frontend module schemas

Frontend modules declare their settings in `package.json` under
`mesh.contributes.settings` or in a sibling `settings.schema.json` next to the
manifest. The JSON file wins if both exist.

Frontend surfaces commonly expose a `surface` object with placement and input
policy keys such as:

- `anchor`
- `layer`
- `width`
- `height`
- `exclusive_zone`
- `keyboard_mode`
- `visible_on_start`

`keyboard_mode` is the important new input-policy hook:

- `none` keeps the surface passive.
- `exclusive` makes it a dedicated keyboard sink while focused.
- `on_demand` asks the shell/compositor for keyboard focus only when the user
  actively engages that surface, which is the preferred mode for keyboardable
  shell chrome like the navigation bar.

## Generated UI

For any module with a schema, the core generates a settings page
automatically. The page writes to the per-module file, not the system file,
so changes are scoped and reversible.

Modules that need a custom layout may ship a `settings_ui` entrypoint
(declared in `package.json`) that renders a `.mesh` component instead of the
generated form. The schema still governs validation.

## Reading settings from a module

```luau
local cfg = mesh.config

-- own module's settings
local fmt = cfg.get("clock_format")   -- resolved through the full stack

-- contract-level settings (via the proxy)
local audio = require("mesh.audio@>=1.0")
local pri   = audio.config.get("default_output_priority")

-- subscribe to changes
cfg.on_change("clock_format", function(v) updateClock() end)
```

Reads always return the effective value. The module never needs to know
which layer supplied it.

## CLI

```
mesh settings get <key>                  # print effective value
mesh settings set <key> <value>          # write to user module/system file
mesh settings unset <key>                # remove override, next layer wins
mesh settings reset <module-id>          # remove all user overrides for a module
mesh settings diff                       # show which layer supplied each key
mesh settings validate                   # re-check the stack against current schemas
```

`mesh settings diff` is the debugging escape hatch — it walks the six-layer
stack and prints, for each key, where it was defined and which layer
actually supplied the effective value.

## Summary

- JSON everywhere for runtime settings; TOML for manifests/declarations.
- Six layers, composed in strict order; later wins.
- Shell-level keys live at the root; module keys are scoped by module ID.
- Interface contracts export shared schemas so settings survive backend swaps.
- Modules publish a schema and read effective values; only the core writes.
