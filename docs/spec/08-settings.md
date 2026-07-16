# 08 — Settings

> Part of the [MESH Specification](README.md).

One logical settings service, sparse and namespaced. **Defaults never live in
stored overrides**
— they come from `<props>` declarations, in-script backend props, interface
props, manifest surface placement, and shell prop declarations. The store
holds *only values the user changed*.

**Status: target.** This replaces the previous multi-file model
(`settings-default.json`, `shell-settings.json`, per-module
`config/settings.json`, six-layer stack) — those files and their readers are
deletion targets. Schemas no longer come from `mesh.provides.settings`
(deleted); they derive from props ([03 §5](03-components.md)).

## 1. The store

**Target storage representation.** The default settings-service provider may
store JSON under the MESH dotfiles/state layout, but consumers use the
`mesh.settings` service contract rather than depending on that file. Alternate
providers may use another persistence implementation while preserving the same
typed behavior. Every logical top-level key is a namespace:

```json
{
  "shell": {
    "theme":  { "active": "@alice/theme", "mode": "dark",
                "tokens": { "color-primary": "#FF6B00" } },
    "locale": { "active": "sk-SK", "chain": ["sk-SK", "sk", "en"] },
    "icons":  { "packs": ["@mesh/user-icons", "@mesh/icons-material"] },
    "fonts":  { "packs": ["@mesh/fonts-default"], "ui_family": "body" },
    "keyboard": { "surface_shortcuts": { "@mesh/navigation-bar": { "mute": { "key": "u" } } } },
    "tooltip":  { "delay_ms": 200 }
  },

  "mesh.audio": { "props": { "global": { "default_output_priority": "headphones" } } },

  "@mesh/pipewire-audio": { "props": { "global": { "poll_interval": 1000 } } },

  "@mesh/navigation-bar": {
    "surface": { "anchor": "bottom" },
    "props": {
      "global":   { "density": "compact" },
      "instances": { "@mesh/navigation-bar#top/import:audio": { "track_width": "28px" } }
    },
    "icons": { "overrides": { "settings": "lucide/settings" } }
  }
}
```

Namespace kinds:

| Namespace | Owner | Contents |
| --------- | ----- | -------- |
| `"shell"` | Core | Theme/mode + token overrides ([04](04-styling.md)), locale ([07](07-i18n.md)), pack chains ([05](05-icons.md), [06](06-fonts.md)), keyboard ([10](10-keyboard.md)), tooltip, and other shell props |
| `mesh.<interface>` | Interface contract | Shared props that survive provider swaps (§4) |
| `@scope/name` | Module | `props` (global + instances), `surface` placement overrides, per-module `icons`/`fonts` chains and overrides |

Rules:

- **Sparse.** A key exists only if the user changed it. `mesh settings unset`
  deletes the key; the declared default wins again. Nothing ever copies
  defaults into the store.
- **Validated.** Every write is validated against the owning props
  declaration / core schema. Invalid values are rejected with a diagnostic
  and the stored value is ignored (falls through to default).
- **Service-written.** Modules read effective values and subscribe to changes;
  they never mutate another module's settings directly. Settings components,
  CLI adapters, and automation clients write through the selected
  `mesh.settings` service provider. The core exposes generic validated storage
  and transport primitives rather than settings policy. (Durable
  module-*internal* state is
  `self.storage`, which is a different, module-writable surface.)
- Profile composition owns root instances and ambiguous provider choices
  ([01 §5.2](01-module-system.md)); the settings service holds preference
  *values*, not module-graph topology.

### 1.1 Profile scope

Configuration overrides are profile-scoped by default. Resolution layers a
shared user default beneath the active profile and instance override. Durable
service-owned data such as histories or indexes is shared across profiles unless
the service contract explicitly declares another scope.

## 2. Where defaults come from

| Value | Default source | User override location |
| ----- | -------------- | ---------------------- |
| Component config | `<props>` defaults in `.mesh` | `<module>.props.global` / `.instances` |
| Backend config | in-script `props {}` in `main.luau` | `<provider-module>.props.global` |
| Interface shared config | `props` in the contract JSON (`module.json`) | `mesh.<interface>.props.global` |
| Surface placement | `mesh.surface` in the manifest | `<module>.surface.*` |
| Host/runtime knobs | host prop declarations | `shell.*` |
| Keybinds | `mesh.keybinds` triggers | `shell.keyboard.surface_shortcuts` |

## 3. Precedence

The full ladder is defined once, in [03 §4](03-components.md): author default
→ user global → author instance → user per-instance → script → imperative.
The store contributes exactly the two user layers. Per-instance keys are the
composition instance key, prefixed by the root-graph instance id when one
exists (`@mesh/navigation-bar#top/import:audio`).

## 4. Provider-swap survival

Props declared by an interface live under the contract's namespace
(`mesh.audio`), so pinning a different provider preserves them. Props
declared by a provider module live under the module's namespace and are
simply ignored while that provider is inactive — kept, not reset; re-pinning
brings them back.

## 5. Generated settings UI

**Status: target** (settings surface module → v1.22).

For every module, the settings surface renders, with zero module-specific
code:

- **Props rows** from the module's props declarations: typed controls
  ([03 §3.2](03-components.md)), i18n labels, global scope by default with a
  "this instance only" switch where instances exist, and a per-row reset
  (= unset).
- **Surface placement** controls from the core placement schema.
- **Resource chains** — icon/font pack pickers and per-name override pickers
  writing the §1 shapes; the icon picker writes the user icon-pack module
  ([05 §4.2](05-icons.md)).
- **Module graph info** — uses/provides, active provider selection (writes the
  active profile through the appropriate service), capabilities, health, diagnostics
  ([01 §9](01-module-system.md)).

Modules needing a custom layout may ship a `settings_ui` entrypoint rendering
a `.mesh` component; the props declarations still govern validation and
persistence.

## 6. Reading settings from modules

Effective values only — a module never knows which layer supplied a value:

```luau
-- component/backend config: the props projection
local density = props.density

-- interface shared props (on the proxy)
local audio = require("mesh.audio@>=1.0")
local pri = audio.props.default_output_priority
```

Prop reads are tracked; changes rerender/notify only affected consumers.
Runtime settings reloads (file watch) reapply theme/locale/module changes
hot. *(Proxy `props` access: target; component props tracking: with
[03](03-components.md) Phase 1.)*

## 7. CLI

```
mesh settings get <namespace>[.key]      # effective value + which layer supplied it
mesh settings set <namespace>.<key> <v>  # validate + write the sparse override
mesh settings unset <namespace>.<key>    # delete override; default wins
mesh settings reset <namespace>          # remove all overrides in a namespace
mesh settings doctor                     # orphaned namespaces, invalid values, unknown keys
```

Orphaned namespaces (module uninstalled) are reported, not auto-deleted —
reinstalling restores the user's configuration.
