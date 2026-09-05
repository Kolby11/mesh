# 08 — Settings

> Part of the [MESH Specification](README.md).

Ownership follows [00 §2](00-philosophy.md#2-core-owns-platform-invariants-modules-own-experiences):
the authoritative settings service is built into core. Its UI is ordinary,
replaceable `.mesh` components, as are devtools and package UI.

One logical settings service, sparse and namespaced. **Defaults never live in
stored overrides**
— they come from `<props>` declarations, in-script backend props, interface
props, manifest surface placement, and shell prop declarations. The store
holds *only values the user changed*.

Ordered customizable-slot placements are sparse profile composition, not
settings-store preferences. They live in profile nodeSlots records because
they affect the activation closure and runtime component tree. Source defaults
apply when no record exists; a reset deletes the record; an explicit empty
nodes list intentionally empties the slot. Public placement props still reuse
the component's derived props schema and generated control metadata.

"One logical service" describes the user/developer API, not one undifferentiated
ownership bucket. Its effective snapshot joins the installed catalog, active
profile composition, declared defaults, sparse overrides, and runtime status;
the writable source remains authoritative by domain. Package installation owns
availability, profiles own composition, this store owns preference overrides,
and runtime health is observed state. A settings frontend may present all four
on one module page without copying them into `settings.json`.

**Status: shipped** for the store itself (§1–§4), global/per-instance frontend-prop controls
in the generated settings UI (§5), the current shell CLI (§7), and the
`mesh.settings` service contract — reads are a revisioned state broadcast and
writes are the capability-gated `set_prop`/`unset_prop` methods
([01 §5.4](01-module-system.md)); **target** for retiring the injected
`settings` prop (components still receive their namespace as a prop rather than
reading the service they declare), script-side layer introspection
([03 §4](03-components.md#4-precedence--one-specificity-ladder)), and the
remaining service-backed CLI surface. This replaced the previous multi-file
model (`settings-default.json`,
`shell-settings.json`, per-module `config/settings.json`, six-layer stack) —
those files and their readers are deleted. Schemas no longer come from
`mesh.provides.settings` (deleted); they derive from props
([03 §5](03-components.md)).

## 1. The store

**Shipped.** One JSON document — `config/settings.json` in a repo checkout,
`$MESH_HOME/settings.json` otherwise, `MESH_SETTINGS_PATH` overriding both.
Loaded once into `mesh_core_config::SettingsStore` and shared with every
component; the shell watches the file and re-applies changes live.

**Storage ownership.** Core owns persistence and effective-value resolution.
API consumers use the built-in `mesh.settings` contract instead of depending on
its files; human-editable configuration remains supported. Replaceable settings
UI does not imply a third-party replacement for the settings engine or storage
backend. Core may evolve persistence behind the same contract. Every logical
top-level key is a namespace:

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
  defaults into the store — a module that changes its own defaults in an
  update still reaches a user who never overrode them.
- **Deep-merged, not replaced.** Setting one field in a namespace leaves its
  siblings on their declared defaults. Arrays (pack chains, activation-key
  lists) replace wholesale: a stored list is a complete ordered replacement by
  intent, not something to append to.
- **Ejectable.** Because the store is sparse, a module the user never
  configured has no block to hand-edit. `mesh-shell config eject <module-id>`
  materializes that module's *effective* surface placement and exposed frontend
  prop values into its namespace, where they become ordinary overrides — pinned
  from then on, like any other stored value. Author-only manifest capabilities,
  such as `mesh.surface.promotable`, are never emitted into that namespace.
  Localized surface values retain their `{ "t": "…", "fallback": "…" }`
  identity instead of pinning only the current fallback, and derived window
  identity such as `app_id` is materialized before it becomes an override.
- **Validated.** Every stored value is validated against the owning props
  declaration / core schema. Invalid values are rejected with a diagnostic
  naming the namespace, the key path, the value found, and what to do; the
  stored value is ignored and the declared default applies. A bad settings file
  is never fatal. This is shipped for the `shell` namespace, `surface` blocks,
  and frontend component props, including unknown-key detection with a "did you
  mean" suggestion. Backend and interface props remain target work with
  [03 §5](03-components.md#5-props-everywhere-non-mesh-modules).
- **Service-written.** Modules read effective values and subscribe to changes;
  they never mutate another module's settings directly. Settings components,
  CLI adapters, and automation clients write through the built-in
  `mesh.settings` service. Core owns validation, precedence, persistence, and
  transaction semantics. Durable module-*internal* data uses `self.storage`,
  a separately scoped module-writable API; a storage write is not a settings
  mutation. Neither path confers access to another module's private state.
- Profile composition owns root instances and ambiguous provider choices
  ([01 §5.2](01-module-system.md)); the settings service holds preference
  *values*, not module-graph topology.

The settings provider exposes effective values with their source layer even
when no raw namespace exists. Writes use typed operations (`set`, `unset`,
`reset`, and atomic transactions with an expected revision); clients subscribe
to structured changes. Profile and package mutations use their own typed
services/capabilities rather than arbitrary JSON paths, although the settings
frontend presents them together.

### 1.1 Profile scope

**Status: shipped.**

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
| Per-module icon chain/overrides | `mesh.uses.resources.icons` + `mesh.icons` | `<module>.icons.*` |
| Host/runtime knobs | host prop declarations | `shell.*` |
| Keybinds | `mesh.contributes.keybinds` triggers | `shell.keyboard.surface_shortcuts` |

### 2.1 Blur quality (`shell.render.blur`)

Element `filter: blur()` ([04 §9](04-styling.md)) is the one style whose cost
scales with the area it covers rather than with the number of elements, so it
gets a user-facing dial:

```json
{
  "shell": {
    "render": {
      "blur": { "passes": 1, "max_radius": 96 }
    }
  }
}
```

- `passes` (1–3, default 1) — blur passes per filtered layer. Each pass runs at
  the reduced sigma that keeps the total blur constant, so more passes buy a
  smoother falloff, not a wider one, and cost roughly proportionally
  (measured ~2.8x for two passes). Out-of-range values clamp.
- `max_radius` (default 96) — radii above this are dropped with a painter
  diagnostic instead of rasterized, bounding the worst frame a stylesheet can
  ask for.

## 3. Precedence

The full ladder is defined once, in [03 §4](03-components.md): author default
→ user global → author instance → user per-instance → script → imperative.
The store contributes the two user layers; scripts can inspect them and their
provenance separately from the effective result (introspection remains target
API work in [03 §4](03-components.md#4-precedence--one-specificity-ladder)).
Script/imperative overrides are runtime state and do not persist preferences
implicitly. Per-instance keys are the
composition instance key, prefixed by the root-graph instance id when one
exists (`@mesh/navigation-bar#top/import:audio`).

## 4. Provider-swap survival

Props declared by an interface live under the contract's namespace
(`mesh.audio`), so pinning a different provider preserves them. Props
declared by a provider module live under the module's namespace and are
simply ignored while that provider is inactive — kept, not reset; re-pinning
brings them back.

## 5. Generated settings UI

**Status: partially shipped.** Exposed props from a frontend module's primary
component produce typed global and per-instance controls, effective values, and
per-row reset actions. Writes are validated and persisted as sparse
active-profile overrides. Surface/resource editing controls remain target work;
contributed settings pages, composition overrides, and module graph inspection
are shipped.

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
- **Module graph info (shipped)** — uses/provides, active provider selection (writes the
  active profile through the appropriate service), capabilities, health, diagnostics
  ([01 §9](01-module-system.md)).

### Which layout renders — one precedence ladder

**Status: shipped.**

```
composition slot override  >  module-provided page  >  generated-from-props fallback
```

- The **generated fallback** stays. Without it a third-party module has zero
  settings UI until someone writes an adapter — worse centralization than a
  hardcoded host.
- A **module-provided page** is an ordinary `mesh.settings.page` contribution
  ([01 §4.3](01-module-system.md)): the module's opinion, not a privilege. Its
  props declarations still govern validation and persistence.
- A **composition** may `replace`, `suppress`, and `order` pages
  ([01 §5.2](01-module-system.md)), so a shell family can restyle or replace
  `@mesh/audio`'s page without touching the audio module.

A module whose page was suppressed reads as having none, so the host falls back
to the generated rows for it rather than showing nothing. There is no
`settings_ui` entrypoint: it was the module-keyed spelling of a contribution
that now has a contract-keyed one.

## 6. Reading settings from modules

Normal prop reads return effective values. This does not hide user intent:
scripts may explicitly inspect the underlying layers through the target
introspection API in [03 §4](03-components.md#4-precedence--one-specificity-ladder).
Scripts decide how to apply preferences within the platform's validation and
permission boundaries.

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

**Shipped** (`mesh-shell config`):

```
mesh-shell config path                   # the settings file path
mesh-shell config show [namespace]       # whole document, or one namespace's overrides
mesh-shell config doctor                 # report values MESH cannot use, and orphaned
                                         # namespaces; exits non-zero on any error
mesh-shell config eject <module-id>      # materialize effective surface + exposed props (§1)
mesh-shell config reset <namespace>      # drop a namespace's overrides
```

**Target** (`mesh settings`, through the settings service):

```
mesh settings get <namespace>[.key]      # effective value + which layer supplied it
mesh settings set <namespace>.<key> <v>  # validate + write the sparse override
mesh settings unset <namespace>.<key>    # delete override; default wins
mesh settings reset <namespace>          # remove all overrides in a namespace
mesh settings doctor                     # as `config doctor`, through the service
```

Orphaned namespaces (module uninstalled) are reported, not auto-deleted —
reinstalling restores the user's configuration.
