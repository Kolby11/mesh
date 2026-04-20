# Settings

MESH's settings system has two goals:

1. **Extensibility** — any plugin can declare its own settings and the core
   will generate a UI, validate input, and apply user overrides without
   plugin-specific code.
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
 2. Implementation default   (shipped with the backend/frontend plugin)
 3. System default            (/usr/share/mesh/settings-default.json or distro pkg)
 4. User system override      (~/.config/mesh/settings.json)
 5. User plugin override      (~/.config/mesh/plugins/<plugin-id>.json)
 6. Runtime override          (set at runtime via IPC / CLI, not persisted unless flushed)
```

Only layers 3–6 are user-writable. Layers 1–2 ship inside packages and are
read-only. The core composes all six into an effective view on every read.

### System-wide vs. plugin-wide

- **System-wide settings** apply across the whole shell (active theme,
  locale, allow-unsigned plugins, plugin frame budgets, …). Keys live at the
  root of `~/.config/mesh/settings.json`.
- **Plugin-wide settings** belong to a single plugin. They live under a
  plugin-scoped key in the system file *or* in a dedicated per-plugin file
  for users who prefer that split. Both forms are valid; the core merges them.

The user override wins over the system default. A per-plugin file wins over
the inline plugin section in the system file — that's the override direction.

## File formats

### System file — `~/.config/mesh/settings.json`

```json
{
  "theme": {
    "active": "mesh-default-dark"
  },
  "i18n": {
    "locale": "en",
    "fallback_locale": "en"
  },
  "plugins": {
    "allow_unsigned": false,
    "auto_update": false
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

Top-level keys that are *not* plugin IDs are reserved for the shell itself
(`theme`, `i18n`, `plugins`, `interfaces`). Plugin-scoped overrides use the
plugin's fully qualified ID as the key.

### Per-plugin file — `~/.config/mesh/plugins/<plugin-id>.json`

```
~/.config/mesh/plugins/@mesh/panel.json
~/.config/mesh/plugins/@community/weather-widget.json
```

Each file is a flat JSON object containing that plugin's keys only:

```json
{
  "clock_format": "12h",
  "show_seconds": true
}
```

Per-plugin files are the canonical target for the generated settings UI —
writing one setting does not require the UI to rewrite the whole system
file.

### System defaults — `/usr/share/mesh/settings-default.json`

Same shape as the user system file. Distributions ship this to set sensible
defaults before any user has touched anything. The file in this repo's
`config/settings-default.json` is the project's fallback.

## Keys, namespaces, and validation

Every key has exactly one owner:

- **Shell keys** (`theme.*`, `i18n.*`, `plugins.*`, `interfaces.*`) — owned by
  the core. Schema lives in `mesh-config`.
- **Contract keys** (`mesh.audio.*`, `mesh.network.*`, …) — owned by the
  interface contract. Every implementation inherits them.
- **Plugin keys** (`@scope/name.*` or the plugin's scoped object) — owned by
  the plugin itself.

Each owner publishes a schema (see next section). The core validates every
value on load and on write. Invalid values are rejected, logged, and fall
through to the next layer.

Plugins **cannot** write to their own settings. The user writes them through
the UI or by editing JSON; the core validates and persists. Plugins read an
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
**extend** with its own keys under its plugin scope:

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

| Field | Purpose |
|-------|---------|
| `type` | `"string" \| "integer" \| "float" \| "boolean" \| "enum" \| "object" \| "array"` |
| `default` | Default value. Must match `type`. |
| `values` | Allowed values when `type = "enum"`. |
| `min` / `max` | Bounds for numeric types. |
| `items` | Element schema for arrays. |
| `properties` | Field schemas for objects. |
| `description` | Human-readable description, shown in the generated UI. |
| `scope` | `"system"` or `"user"`. Restricts where this key may appear. Defaults to `"user"`. |
| `requires_capability` | Declares that editing this key requires a specific capability (e.g. `theme.write`). |

## Frontend plugin schemas

Frontend plugins declare their settings inline in `<schema>` inside the
`.mesh` file (existing panel example) *or* in a sibling `settings.schema.json`
next to the manifest. Both are accepted; the JSON file wins if both exist.

## Generated UI

For any plugin with a schema, the core generates a settings page
automatically. The page writes to the per-plugin file, not the system file,
so changes are scoped and reversible.

Plugins that need a custom layout may ship a `settings_ui` entrypoint
(declared in `plugin.json`) that renders a `.mesh` component instead of the
generated form. The schema still governs validation.

## Reading settings from a plugin

```luau
local cfg = mesh.config

-- own plugin's settings
local fmt = cfg.get("clock_format")   -- resolved through the full stack

-- contract-level settings (via the proxy)
local audio = mesh.interfaces.get("mesh.audio", ">=1.0")
local pri   = audio.config.get("default_output_priority")

-- subscribe to changes
cfg.on_change("clock_format", function(v) updateClock() end)
```

Reads always return the effective value. The plugin never needs to know
which layer supplied it.

## CLI

```
mesh settings get <key>                  # print effective value
mesh settings set <key> <value>          # write to user plugin/system file
mesh settings unset <key>                # remove override, next layer wins
mesh settings reset <plugin-id>          # remove all user overrides for a plugin
mesh settings diff                       # show which layer supplied each key
mesh settings validate                   # re-check the stack against current schemas
```

`mesh settings diff` is the debugging escape hatch — it walks the six-layer
stack and prints, for each key, where it was defined and which layer
actually supplied the effective value.

## Summary

- JSON everywhere for runtime settings; TOML for manifests/declarations.
- Six layers, composed in strict order; later wins.
- Shell-level keys live at the root; plugin keys are scoped by plugin ID.
- Interface contracts export shared schemas so settings survive backend swaps.
- Plugins publish a schema and read effective values; only the core writes.
