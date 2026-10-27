# 01 — Module System

> Part of the [MESH Specification](README.md). Principles and canonical
> vocabulary: [00 — Platform Philosophy](00-philosophy.md).

A **module** is the installable MESH unit. An **interface** is the contract.
A **provider** implements the contract. A **frontend** consumes the contract.
Shared Luau **libraries** hold reusable implementation patterns. **Resource
packs** (icons, fonts, themes, languages) map semantic names to assets.

This one workflow covers UI, backends, themes, icon packs, and libraries: users
create modules, modules compose through interfaces and resources, and the Rust
core remains a generic runtime.

## 1. Vocabulary

Use these terms precisely in code, docs, diagnostics, and planning. Old terms
(*package*, *plugin*, *trait*, *addon*, `package.json`, `mesh.toml`) are
replacement debt, never public synonyms.

| Term | Definition |
| ---- | ---------- |
| module | Installable, configurable MESH unit (`module.json` at its root). |
| module kind | The module's primary role: `ui`, `backend`, `interface`, `composition`, `library`, `theme`, `icon-pack`, `font-pack`, `language-pack`. |
| element | Base UI primitive exposed by MESH core (`box`, `button`, `icon`, …). |
| component | User-authored reusable `.mesh` unit composed from elements/components. |
| interface | Named, versioned contract: state fields, methods, events, types, consumer capabilities. Data, not code. |
| extension point | Named, versioned UI contract: a region one module hosts and other modules fill (§4.3). |
| composition | Installable module selecting roots, providers, resources, and slot arrangement (§5.2). |
| provider | Implementation of a service interface by a backend module or an explicit built-in core service (§5.4). |
| contribution | Something a module adds to the installed graph (`mesh.provides.*` or `mesh.contributes.*`). |
| dependency | Something a module needs (`mesh.uses.*`). |
| capability | Permission for a host API or service operation (`shell.surface`, `service.audio.read`, …). |
| resource pack | Module kind contributing semantic-name → asset mappings. |
| library | Module contributing importable Luau code. |
| entrypoint | Named launch/UI entry contributed by a module. |

## 2. Design authority

[00](00-philosophy.md) defines the ownership rules: core owns platform
invariants and built-in mechanisms; modules own desktop experiences and domain
services. Backends expose stable domain state, frontends derive presentation,
and access follows explicit contracts and grants. Core-provided services in
§5.4 are platform authorities, not exceptions for privileged module names.

There is one manifest and component model. Reduce ceremony within that model
(inline contracts, sole-provider selection), while keeping dependencies,
exports, and permissions explainable from declarations. The schemas below
implement these principles; they do not define a second philosophy.

## 3. The manifest — `module.json`

**Status: shipped** (canonical loader with migration diagnostics; superseded
sections still present in code are deletion targets — see §3.4).

Every module has one `module.json` at its root. Top-level fields are package
identity and release metadata (`name`, `version`, `description`, `license`,
`repository`, `private`). All MESH behavior lives under `mesh`.

```json
{
  "name": "@alice/volume-panel",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "ui",
    "entry": "src/main.mesh",
    "uses": {
      "modules": { "@alice/volume-popover": ">=0.1.0" },
      "interfaces": { "mesh.audio": ">=1.0" },
      "resources": { "icons": ["@mesh/icons-default"] },
      "capabilities": ["shell.surface", "service.audio.read", "service.audio.control"],
      "iconRequirements": { "required": ["audio-volume-muted", "audio-volume-high"] }
    },
    "provides": {
      "layout": [{ "id": "main", "entrypoint": "src/main.mesh", "label": "Volume Panel" }]
    },
    "contributes": {
      "i18n": [{ "id": "en", "locale": "en", "path": "config/i18n/en.json" }],
      "keybinds": {
        "mute": {
          "label": { "t": "keybind.mute.label", "fallback": "Mute audio" },
          "trigger": { "kind": "shortcut", "key": "m" }
        }
      }
    },
    "surface": { "anchor": "top", "layer": "top" },
    "accessibility": { "role": "toolbar" },
    "i18n": { "defaultLocale": "en", "supportedLocales": ["en"] }
  }
}
```

Rules:

- Module identity is npm-style top-level `name` (`@scope/name`); never a
  top-level `id` or `type`.
- `mesh.entry` fills `entrypoints.main`; for simple frontends it also creates a
  default `main` layout contribution when `mesh.provides.layout` is absent.
- `mesh.uses` holds everything the module *needs*: module deps, interface
  deps (`interfaces` / `optionalInterfaces`), resource-pack deps
  (`resources.icons/fonts/themes/i18n`), host capabilities, runtime binaries,
  icon requirements.
- `mesh.provides` holds general graph contributions such as layout entries,
  extension points, libraries, and fonts. The shared typed envelope
  for theme packs, icon packs, i18n catalogs, and keybind actions is
  `mesh.contributes`; each bucket keeps its domain-specific payload.
- `mesh.implements` is only for backend provider records.
- The validator keeps buckets strict: module/resource deps are `@scope/name`
  ids; interface deps are dotted contract names (`mesh.audio`); capabilities
  are permission names (`service.audio.read`, `shell.surface`).
- Old manifest inputs (`package.json`, `mesh.toml`, `plugin.json`, legacy
  top-level `id/type/api_version`) **fail loading** with a replacement
  diagnostic. Multiple manifest files in one module fail until resolved.

### 3.1 Closed core, open provides

**Status: target.**

Core `mesh` fields (`kind`, `entry`, `uses`, `implements`, `surface`,
`accessibility`, `i18n`, `theme`, pack sections, and the typed
`contributes` buckets) are a **closed schema**: unknown core fields and
near-miss typos produce diagnostics. `mesh.provides.*` and
`mesh.uses.resources.*` are **open namespaces**: unknown
contribution kinds are preserved in the installed graph as typed opaque records
so third-party tools/modules can define new contribution kinds without a core
release. Superseded manifest sections in code (`ServiceSection`,
`DependenciesSection`, `AssetsSection`, `ExportsSection`, `IconsSection`, dead
generations in `model.rs`) are deleted outright.

### 3.2 Module kinds

| Kind | Purpose | Kind-scoped sections |
| ---- | ------- | -------------------- |
| `ui` | `.mesh` component: a shell surface, a widget or an embeddable component | optional `mesh.surface` (default placement), `mesh.accessibility`, `mesh.contributes.keybinds`, `mesh.theme`, `mesh.fonts` |
| `backend` | Provider implementing interfaces (Luau `main.luau`) | `mesh.implements`, `mesh.uses.binaries`, in-script `props {}` |
| `interface` | Data-only contract package | `mesh.interface` |
| `composition` | An installable shell composition: roots, provider bindings, resources, extension point arrangement | `mesh.compose`, `mesh.extends` |
| `library` | Importable Luau helpers or a theme token family; grants no capabilities | `mesh.provides.libraries`, `mesh.theme.tokens` — see [04 §4.1](04-styling.md#41-token-families) |
| `theme` | Theme tokens + component defaults (CSS) | `mesh.contributes.themes` — see [04](04-styling.md) |
| `icon-pack` | Semantic icon name → asset mappings | `mesh.contributes.icons` — see [05](05-icons.md) |
| `font-pack` | Font role → installed family mappings | `mesh.font_pack` — see [06](06-fonts.md) |
| `language-pack` | Translation catalogs for other modules | `mesh.contributes.i18n` — see [07](07-i18n.md) |

**Shipped:** a `ui` module has one primary public component; extra public
entries are explicit contributions (§4.3). Other modules consume it with
`require("@scope/name")`. `mesh.surface` is optional default placement: a `ui`
module that declares it is mounted as a root on direct install and in the legacy
root graph, and contributes its default layout entry. Without it, install only
makes the component available. Profiles may explicitly mount any `ui` module as
a root instance (`mesh-shell profile add <profile> <module>`). A root without
declared placement takes core defaults plus the instance's sparse `surface`
override and runs with its existing capability grants. Required interfaces gate
activation either way.

The former `frontend` and `component` kinds are rejected with a migration
diagnostic: replace either with `"kind": "ui"`. A frontend's `mesh.surface`
stays as its default placement.

Theme, icon-pack, font-pack and language-pack remain separate kinds. A mixed
resource-pack kind was evaluated and not adopted: each resource has its own
selection rule (one theme, ordered icon and font pack chains, language packs
scoped to a target module), so a mixed pack would still need per-resource
profile selection and validation. Distributing a matching set is a packaging
concern: separate modules can be installed together and selected by a
composition (§5.2).

### 3.3 Surface placement (`mesh.surface`)

**Status: shipped.**

`mesh.surface` carries **placement only** — the compositor concerns CSS cannot
express. Declare only deltas; omitted fields fall back to core defaults.

`role` picks which compositor protocol realizes the surface, and thereby which
of the other fields apply:

| `role` | Protocol | Placement fields |
| ------ | -------- | ---------------- |
| `layer` (default) | `zwlr_layer_shell_v1` — shell chrome: panels, launchers, overlays | `anchor`, `layer`, `keyboard_mode` |
| `window` | `xdg_toplevel` — an ordinary application window that tiles, floats, moves between workspaces, and closes | `title`, `appId`, `resizable`, `decorations` |

`visible_on_start` and `blur` apply to both.

```json
"surface": {
  "role": "window",
  "title": { "t": "settings.title", "fallback": "Settings" },
  "appId": "mesh.settings",
  "visible_on_start": false
}
```

Fields belonging to the *other* role are a **graph diagnostic**
(`surface_role_field_mismatch`), not a silently ignored key: a block that names
both an anchor and `role: "window"` states two incompatible intents, and a
compositor places a toplevel regardless of what the manifest asks.
Surface enum values are checked with the same canonical parsers used by runtime
resolution before role policy is applied; an unknown value produces an
`invalid_surface_enum` graph diagnostic naming the path, value, and canonical
allowed values rather than silently selecting a default.

For a surface whose manifest declares `promotable`, the user can move it
between roles through the sparse settings store (`"surface": { "role":
"window" }`, [08](08-settings.md)) — the author's `role` is the startup
default. A non-promotable surface rejects a stored role change, and the
author-only capability is never a user setting.

#### Promotable surfaces

**Status: shipped.**

`"promotable": true` says the surface may be moved between roles *while it is
running* — popped out of the shell into a window, and docked back. `role` then
names the role it **starts** as.

```json
"surface": {
  "role": "layer", "promotable": true,
  "anchor": "right", "layer": "overlay", "keyboard_mode": "on_demand",
  "title": { "t": "settings.title", "fallback": "Settings" },
  "appId": "mesh.settings"
}
```

A promotable surface is the **one exemption** from the role-mismatch diagnostic
above: it is realized under both protocols at different points in its life, so
both field sets apply to it and declaring both is the only way to describe it.
Each set takes effect in the role it belongs to.

Promotion is opt-in because it is a claim about the component, not about the
manifest: a root laid out as a 32px panel widget is not automatically a sensible
window. A runtime role change is **refused** for a surface that does not declare
it.

The change is non-destructive. Only the compositor object is swapped — the
component's Lua VM, retained tree, page selection, scroll offsets, focus, and
service subscriptions all survive, because none of them is role-dependent. Three
paths trigger it:

| Trigger | Reaches |
| ------- | ------- |
| `mesh.shell.set_surface_role(surface_id, role)` | in-surface controls (a "pop out" button) |
| `mesh.shell.toggle_surface_role(surface_id)` | a control that flips whichever role is current |
| `shell:promote_surface:<id>`, `shell:demote_surface:<id>`, `shell:toggle_surface_role:<id>` over the automation IPC | a *compositor* keybind, since MESH keybinds are focused-surface actions and cannot grab a global hotkey ([11](11-automation-ipc.md)) |

These role requests accept an exact surface instance ID or a module ID when
that module has exactly one root surface, just like show/hide/toggle. With
multiple root instances, callers must supply the instance ID.

A component tells the two apart in CSS with `:windowed`
([04 §6.1](04-styling.md)) rather than in script state, so its chrome follows
the surface's actual role even when the role was changed from outside it. That
is how one header offers "pop out" as a layer surface and "dock back" as a
window.

Because the roles size in opposite directions (below), a promotable surface goes
through a fresh first-configure pass on every change rather than carrying over a
size measured under the other role.

#### Promoted embedded widgets

**Status: shipped.** An embedded widget can be promoted without promoting its
parent surface. The shell keeps the widget's retained node and shared surface
VM in place, removes its pixels from the parent layout, and paints the keyed
subtree into an independent `xdg_toplevel`. Its state, live `bind:this`
references, handlers, and service subscriptions therefore continue across the
move. A widget can be controlled from Luau with
`mesh.shell.promote_widget(surface_id, node_key)` /
`mesh.shell.demote_widget(surface_id, node_key)`, which take the owning surface
and the retained node key; `mesh.shell.set_widget_role(surface_id, node_key,
role)` is the explicit role form. Closing the promoted window demotes the widget back into its parent.

Surface **sizing is CSS**: the laid-out box of the component root
(`width: 100%` spans the anchored edge, `fit-content` shrinks to content,
`min-*`/`max-*` clamp), measured by `measure_content_size()`. A top or bottom
root whose width is `auto` or `100%` requests the protocol's spanning width;
a fit or fixed width is requested as measured, and the compositor centres it
along that edge. The show/hide
transition is a CSS `transition` on the root. There are no manifest sizing
fields and no compatibility aliases. See [03 — Components](03-components.md).

The two roles size in **opposite directions**, which is the one thing a
component author must know before setting `role: "window"`:

- A **layer surface** tells the compositor its CSS-measured size. When the
  compositor configures less on a fixed axis (an output smaller than the
  request), that configure binds: the tooltip overlay reserve gives way first,
  then the content lays out into what is left, and pointer input follows the
  same geometry. A root with a definite `width`/`height` keeps requesting that
  size, so pairing it with `max-width: 100%` / `max-height: 100%` lets it fit
  a small output and grow back on a larger one. On an axis the root spans
  (`width`/`height: 100%`), the content takes the whole configured size; the
  tooltip overlay reserve applies only to fixed axes. The reserve exists only
  on compositors without `xdg_popup`; elsewhere a tooltip that leaves the
  content box is promoted to a popup and the layer is exactly its content.
- A **window** is *told* its size by the compositor's configure — tiling
  layout, maximize, fullscreen, interactive resize — and content lays out into
  it. The measured size is only the initial request. A component with a rigid
  root (fixed `width`/`height`, or `min-*` equal to `max-*`) should declare
  `"resizable": false`, which pins the window to that size by reporting it as
  both the toplevel min and max; otherwise write a root that can absorb a size
  it did not choose.

  A root that absorbs it is written in CSS, not in the manifest: the
  compositor's toplevel states arrive as the `:fullscreen`, `:maximized`,
  `:activated`, and `:tiled` pseudo-states on every node of the surface
  ([04 §6.1](04-styling.md)), so a floating size on the base rule and
  `width: 100%` on the filling states is the whole mechanism. Note that
  `"resizable": false` and a fullscreen-aware root are contradictory intents —
  pinning min to max is what stops a compositor from resizing the window at
  all.

  Size and toplevel-state changes apply throughout the surface even when focus
  or hover changes in the same frame. A role switch paints the replacement
  surface in its resting styles, without interpolating its geometry from the
  previous role.

A window has no say in its position: xdg-shell gives the client none. Placement
control comes from compositor window rules keyed on `appId`, which defaults to
the module id.

Closing a window (title-bar button or compositor binding) **hides** the
surface; the module, its services, and its Lua state survive, so reopening it is
the same cheap show as reopening a hidden panel.

### 3.4 What the manifest no longer carries

Per the props model ([03](03-components.md)) and settings model
([08](08-settings.md)):

- **No `mesh.provides.settings` schema.** Settings schemas derive from
  `<props>` (components), in-script `props {}` (backends), and interface
  props (interfaces). A module's settings namespace is its module id; an
  interface's is its contract name. *(The legacy schema is removed; backend
  and interface props remain target work.)*
- **No surface sizing / `display_transition` / `size_policy`.** *(Shipped —
  removed.)*
- **No inline icon mappings in frontends.** Mappings live in icon-pack
  modules; frontends keep only `iconRequirements` and pack deps. *(Shipped
  direction; redundant sections are deletion targets.)*

## 4. Interfaces

**Status: shipped** for the registry, inline and external JSON contracts, keyed
external declarations, type/event validation, runtime proxy typing, LSP
contract completions, and relationship metadata. Substantial interface modules
may move the same JSON contract object to a separate `contract.json` referenced
by `module.json`; tiny contracts may remain inline. This reduces manifest noise
without adding another language or execution model. **Target:** generated
provider stubs, mocks, standalone documentation, and compatibility reports.

An interface is a named, versioned declaration of:

- **State fields** — readable values exposed through the proxy.
- **Methods** — request/response commands routed to the active provider.
- **Events** — typed channels owned by the active provider.
- **Types** — shared structs used by state, methods, events.
- **Consumer capabilities** — what a *consumer* needs to read/control it. The
  compiled contract may declare `capabilities.read`, per-event
  `capabilities.events`, and per-method `capabilities.methods` requirements;
  these policies are authoritative for custom interface names and are checked
  at activation, proxy creation, delivery, subscription, and dispatch. A proxy
  decides its method grants when acquired; dispatch checks again because it
  also serves callers without a proxy, such as core-triggered calls.
- **Shared props** — user preferences that survive provider swaps
  ([08 §4](08-settings.md)).

Interface modules are data-only packages. The shipped representation carries
the contract inline:

```json
{
  "name": "@mesh/audio-interface",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "interface",
    "interface": {
      "name": "mesh.audio", "version": "1.0",
      "domain": "audio", "relationship": "base",
      "contract": {
        "state": [
          { "name": "available", "type": "boolean" },
          { "name": "percent", "type": "float" },
          { "name": "muted", "type": "boolean" }
        ],
        "methods": [
          {
            "name": "set_volume",
            "args": [
              { "name": "device_id", "type": "string" },
              { "name": "percent", "type": "float" }
            ],
            "returns": "Result",
            "coalesce": { "key": ["device_id"] },
            "stateBinding": { "field": "percent", "fromArg": "percent" }
          },
          {
            "name": "set_muted",
            "args": [
              { "name": "device_id", "type": "string" },
              { "name": "muted", "type": "boolean" }
            ],
            "returns": "Result",
            "coalesce": { "key": ["device_id"] },
            "stateBinding": { "field": "muted", "fromArg": "muted" }
          }
        ],
        "events": [
          {
            "name": "VolumeChanged",
            "payload": [
              { "name": "device_id", "type": "string" },
              { "name": "level", "type": "float" }
            ]
          }
        ],
        "capabilities": {
          "required": ["service.audio.read"],
          "read": ["service.audio.read"],
          "events": { "VolumeChanged": ["service.audio.read"] },
          "methods": {
            "set_volume": ["service.audio.control"],
            "set_muted": ["service.audio.control"]
          }
        }
      }
    }
  }
}
```

**Shipped — command coalescing.** A method may declare
`"coalesce": { "key": ["device_id"] }` to replace queued invocations only when
all named top-level arguments match. Key names must be unique declared method
arguments; unknown or repeated names reject the contract. Values retain their
JSON types and compound structure. A missing key argument disables coalescing
for that invocation; an explicit `null` is a value. Core never infers identity
from domain-specific argument names.

Omitted/`false` disables coalescing. `true` is supported as shorthand for
`{ "key": [] }`, one key for the whole method. Use coalescing only for idempotent
setters, never relative or accumulating commands. The shell throttle and backend
queue use the same declared keys; backend queues additionally separate backend
identities. Replaced calls settle as `superseded`. Changing a method's key is a
breaking behavioral contract change, reflected in compiled metadata, behavior
fingerprints, compatibility diagnostics and generated documentation.

The external form keeps `module.json` canonical:

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
      "contract": "contract.json"
    }
  }
}
```

`contract.json` uses keyed `state`, `methods`, `events`, and `types` objects.
It declares descriptions, units, ranges, errors, capabilities, and optional
features. Tooling compiles inline and external forms into the same
canonical `InterfaceContract` representation. Runtime validation and LSP
completion use its strict Luau field, argument, return, and event-payload types.
Services without a validated contract have no service-member suggestions;
backend return fields and synthetic callback APIs are not inferred.
`mesh-shell types [<interface>...] [--out <dir>]` writes each compiled
contract's consumer declarations as `<interface>.d.luau`. Provider stubs,
mocks, standalone documentation, and compatibility reports are generated by
`mesh-core-service` but have no CLI yet.

**Shipped contract requirement.** Every runnable service has an explicit typed
contract, inline or external. A provider whose interface has no valid contract
is never selected or bound: the graph reports `missing_interface_contract` for
it, required consumers are blocked, and the service catalog creates no
contract-less binding.

- **Shipped — optional features.** A contract groups optional members into
  named features, `"features": { "per-app-volume": { "description": "…",
  "members": ["apps", "set_app_volume", "AppsChanged"] } }`. Names are
  lowercase words joined by `-`; members are declared state fields (which must
  have optional types), methods or events, each in at most one feature;
  `supports` is reserved. A provider lists what it supplies in
  `implements[].features`; an undeclared name is the graph diagnostic
  `unknown_provider_feature`. A missing feature never makes a provider
  ineligible. Consumers call `svc:supports(name)`, which is false without a
  provider. A method of an unsupported feature returns `{ ok = false, error }`
  without dispatch, its state fields read nil, and its events never fire; the
  shell rejects state fields and events of features the provider did not
  advertise with `service_contract_warning`. A feature over new members is an
  additive contract change; gating existing members, or removing a feature, is
  breaking. Generated consumer types include `supports`.
- **Provider eligibility.** Profile activation and graph selection use the
  same version rule: an explicit offered version must equal the declared
  contract version, and that version must satisfy the consumer's requirement.
  An omitted offered version adopts the declared version. Matching a consumer
  range alone does not make a different contract version eligible.
- **Type grammar.** Every `type`/`returns` expression is validated at graph
  build: primitives (`string`, `int`, `float`, `boolean`, `object`, `any`),
  PascalCase named types declared under `contract.types` (plus the builtin
  `Result`), with `[]` (array) and `?` (optional) suffixes. Invalid
  expressions or references to undeclared types produce
  `invalid_interface_contract` diagnostics, and the interface's providers
  cannot be activated.
- **One owning declaration.** Shared domains are published as independent
  `interface` modules. Providers implement them and depend on the owning module.
  A backend may declare a private interface under `mesh.interfaces[]` in its
  publisher namespace (`@alice/weather` may declare `alice.weather`). Each name
  has exactly one enabled declaring module. Duplicate standalone, inline, or
  mixed declarations are graph errors, including identical copies; provider
  priority never selects a contract.
- **Namespace provenance.** Service and extension-point declarations in
  `mesh.*` require assessed `core` distribution provenance. A module name alone
  does not grant this right; local/community/verified sources cannot claim the
  standard namespace even with an `@mesh/` id. Other declarations use the
  publisher's dotted namespace. These publication rules grant no host powers.
- **No untyped fallback.** A backend that implements an interface nothing
  declares, or whose declaration has no contract, is not a provider. Interface
  modules without a contract report `missing_interface_contract`, as does each
  provider of such an interface.
- **State ranges and defaults.** A state field may declare `minimum` and
  `maximum` (`int`/`float` only) and a `default` that matches its type and
  range. The compiled contract normalizes every provider snapshot before it is
  stored: absent fields with a default are filled, numbers are clamped into
  range (an `int` stays integral), and anything else that violates the
  contract rejects the snapshot with a `service_contract_warning`. Frontends
  therefore read range-checked values. Before a service publishes state (or
  when no provider is installed, for an optional or a required service; see
  §5), its proxy reads `available` as `false` and each field as its default,
  and `require` and static imports return that proxy instead of raising. Widening or dropping a range, or
  dropping a default, is a breaking contract change; narrowing is additive.
- **Reactive command state.** A method may declare
  `"stateBinding": { "field", "fromArg" }` or
  `"stateBinding": { "field", "toggle": true }`: on successful dispatch the
  shell updates that field in the interface's canonical shared state,
  publishes it to every observer, retains it across stale provider snapshots,
  and releases the pending binding when the provider confirms the value.
- Do not put provider identity (`source_module`) in contract state — that is
  runtime metadata.

**Shipped standard domains.** `@mesh/wm-interface`, `@mesh/power-interface`
and `@mesh/brightness-interface` own `mesh.wm`, `mesh.power` and
`mesh.brightness`; their contracts remain available with no provider enabled.
The Hyprland, UPower and backlight providers depend on these modules and do not
redeclare them.

`mesh.wm` 1.1 treats workspace `id` as an opaque provider-local integer, stable
for that workspace's lifetime, rather than a position or a numeric name.
Providers normalize native IDs into exact Luau/JSON integers when necessary.
`active_workspace` and `focus_workspace(id)` use that same identity. Workspace
`name` is a display label and may be non-numeric; `windows` is the count.
Optional `index` is a numeric label/display position and may change independently
of ID; `output` names an output or is nil when unassigned. Optional `visible`
means active on its own output, while `focused` means globally focused by the
default seat; several outputs can have visible workspaces at once. Optional
`urgent` carries attention state. Omitted metadata means unknown, not false.
Consumers render the provided records and pass their IDs, rather than inventing
1-based IDs. The shipped bar follows this rule and the Hyprland adapter preserves
named workspace IDs, output and focus metadata, using names for native dispatch.
Its normal workspace list excludes compositor-specific special workspaces.
It consumes structured data through the bounded `mesh.json.decode` host API;
the parser performs no I/O and service normalization stays in Luau.

This distinction follows [niri's workspace model](https://docs.rs/niri-ipc/latest/niri_ipc/struct.Workspace.html)
(stable ID versus output-local index and active/focused flags) and
[Sway's IPC contract](https://github.com/swaywm/sway/blob/master/sway/sway-ipc.7.scd)
(named workspaces, output, visible/focused flags and tree node IDs). These are
contract compatibility checks; niri and Sway provider modules remain unshipped.
Hyprland's [workspace selectors](https://wiki.hypr.land/Configuring/Basics/Dispatchers/)
require named workspaces to be addressed by name rather than their negative
internal IDs.

### 4.1 Versioning

Interfaces follow semver. Major = breaking (prefer a new name, `mesh.audio.v2`,
so old consumers keep working); minor = additive; patch = clarification. The
graph-derived resolved service catalog binds each consumer request to one
compatible `(interface, contract version, provider version, operation-policy
fingerprint)` tuple for the active graph generation. A backend may advertise
several versions during migrations; the catalog keeps those immutable records
while exposing only the graph-selected compatible binding to a consumer.
Consumers request ranges (`require("mesh.audio@>=1.0")`).

### 4.2 Relationships & domains

Anyone may ship a new interface — the core never blocks independent contracts.
`mesh.interface.domain` groups related interfaces;
`mesh.interface.relationship` states intent: `base` (broad shared contract),
`extension` (builds on another via `extends`), `independent` (deliberately
different model; give a `reason`). When an enabled independent interface
shares a domain with a base one, the graph records soft "consider extending"
guidance — discoverability pressure, never a load error.

### 4.3 Extension points — UI contracts

**Status: shipped.**

A service interface lets a frontend depend on a *contract* instead of a backend
module id. An **extension point** is the same idea for UI: a named, versioned
contract naming a region one module renders and other modules fill.

Extension points are declared by `interface` modules, for the same reason
service contracts are — they are data, they are versioned, and both sides must
depend on the contract without depending on each other. An interface module may
declare extension points, a service contract, or both.

```json
{
  "name": "@mesh/shell-ui-interface",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "interface",
    "extensionPoints": {
      "mesh.settings.page": {
        "version": "1.0",
        "multiple": true,
        "props": [
          { "name": "namespace", "type": "string" },
          { "name": "title", "type": "string" },
          { "name": "icon", "type": "string?" }
        ]
      }
    }
  }
}
```

A **host** declares which points it renders and lays them out. Hosting is
explicit and versioned because a host renders another module's UI inside its own
surface — a trust decision, never an inference:

```json
"hosts": { "mesh.settings.page": { "version": ">=1.0", "layout": "column" } }
```

```html
<slot extension-point="mesh.settings.page" />
```

Hosts may also expose a contract as named customizable slots. These slots use
explicit composition/profile placement instead of automatically rendering every
contribution. The host record contains a slots map whose keys are stable local
names and whose defaults are stable source-module-id:contribution-id
references. Markup addresses the same record with a static name,
extension-point, and customizable mode.

A customizable slot name is unique in its component entry and must have a
matching host record. Existing slots remain automatic unless explicitly marked
customizable.

A **contributor** names the point, never the host:

```json
"provides": {
  "extensionPoints": {
    "mesh.settings.page": [
      { "id": "audio", "entry": "src/settings.mesh", "order": 100,
        "props": { "title": { "t": "audio.settings.title", "fallback": "Audio" } } }
    ]
  }
}
```

Rules:

- **Contract names, never module ids.** `mesh.settings.page` is valid;
  `@mesh/settings:custom-settings` is rejected. Replacing the settings frontend
  must not break every contributed page.
- A contribution's `entry` is a `.mesh` component compiled as an **alternate
  root of the contributing module**: its own execution environment, resolved
  capabilities, and settings namespace, rendered inside the host's tree.
  Current contexts may share a thread-owned VM; this does not expose their
  private environments or grant the host's capabilities. A broken contribution
  gets the bounded error placeholder (§8) and cannot blank its host.
- A contribution resolves into **every** enabled host of that point. Two
  settings frontends both receive the pages; that is correct, not a conflict.
- Render order is `(order, source module id, contribution id)` — deterministic
  across rebuilds.
- `multiple: false` means at most one contribution; more is
  `extension_point_overfilled`.
- Contribution props are typechecked against the declaration using the same
  type grammar as service contracts (§4).
- **A contribution creates no dependency edge on the host.** The edge terminates
  at the contract, so a host that itself depends on a contributor is not a
  cycle. Module-keyed slots made that adapter pattern unloadable.

Diagnostics: `unknown_extension_point`, `extension_point_version_mismatch`,
`invalid_extension_point_props`, `extension_point_overfilled`, and the
informational `unhosted_contribution` (a valid state when the host is installed
but not composed).

### 4.4 Events — one communication primitive

Methods are request/response. **Everything asynchronous is a typed event on a
named channel.** There is no second messaging mechanism.

- **Owned channels** are declared inside an interface; only the active
  provider publishes; payloads validate against the contract. Frontends
  subscribe via direct named channels on the proxy
  (`audio.VolumeChanged:on(fn)`).
- **Shell requests** are typed host calls, not channels. **Shipped:**
  `mesh.shell.show_surface`, `hide_surface`, `toggle_surface`, `hide_popover`,
  `activate_popover`, `set_surface_role`, `toggle_surface_role`,
  `promote_widget`, `demote_widget`, `set_widget_role` and
  `position_surface`, plus `mesh.locale.set` and the `task` library. Each
  validates its arguments and the `shell.surface` (or `locale.write`)
  capability at the call and raises a Luau error there; the request reaches
  the shell as a typed effect with no JSON payload. Publishing one of the
  removed `shell.*` surface, locale or scheduler channels raises an error that
  names its replacement, and the module scan reports
  `removed_shell_event_publish`. Interface-domain commands must go through the
  interface proxy, not raw channel publishes
  (`raw_interface_domain_event_publish` diagnostic); unknown `shell.*`
  publishes report `unknown_shell_event_publish`.

  The remaining `shell.*` channels carry **debug requests only**. Changing composition
  or configuration is not a shell channel: those are methods on core-provided
  interfaces (§5.4), because mutations need typed arguments, operation-specific
  authorization, and an acknowledged success or rejection. Typed asynchronous
  events remain suitable for reporting changes; a fire-and-forget shell publish
  is not a substitute for a management transaction.

Static analysis of `.mesh`/`.luau` sources checks emitted events against the
provider's contract (`undeclared_interface_event_emit`) and static frontend
subscriptions against consumed contracts
(`undeclared_interface_event_subscription`). Runtime delivery validates
declared payload schemas and drops invalid events with a
`service_contract_warning` diagnostic. Dynamic event names stay runtime-only.

## 5. Providers and the root graph

**Status: shipped.**

This section describes the current repository graph. Named shell profiles in
§5.2 are the shipped composition model.

Backends declare provider records:

```json
"implements": [{
  "interface": "mesh.audio", "version": "1.0",
  "baseModule": "@mesh/audio-interface",
  "provider": "pipewire", "label": "PipeWire", "priority": 100
}]
```

The **root graph** (`config/module.json`) is decisions-only. The installed set
auto-discovers from `modulesDir` (each module's own manifest declares its name
and kind); the root file holds only what is genuinely the user's choice:

```json
{
  "schemaVersion": 1,
  "modulesDir": "../modules",
  "disabled": ["@mesh/text-selection-proof"],
  "providers": { "mesh.audio": "@mesh/pipewire-audio" },
  "layout": { "entrypoint": "@mesh/navigation-bar:main" }
}
```

- A discovered module is enabled unless listed in `disabled`.
  Shipped UI modules, including the debug inspector, follow the same activation
  and capability rules: disabled modules are not compiled or mounted and receive
  no runtime capability grants.
- When exactly one enabled backend implements an interface, it is
  **auto-selected**; `providers` entries are needed only where several
  implement one interface (a genuine user choice).
- An explicit `modules` map is honored for full manual control (skips
  auto-discovery); the decisions-only form is preferred.
- The graph keeps all installed providers visible, validates the selection,
  surfaces failures through health ([02 §5](02-installation.md)), and
  preserves contract-level props across provider swaps.
- Preference values (active theme/mode, pack chains, locale) live in the
  **settings store** ([08](08-settings.md)), not the root graph. The root
  graph owns module-graph decisions; settings own look-and-feel values.

### 5.1 Multiple instances of a UI module

**Status: target.**

Module identity is not the only surface identity. The root graph may declare
named instances with the `module-id#instance-id` key form:

```json
"layout": {
  "entrypoint": "@mesh/navigation-bar:main",
  "instances": {
    "@mesh/navigation-bar#top":    { "surface": { "anchor": "top" } },
    "@mesh/navigation-bar#bottom": { "surface": { "anchor": "bottom" } }
  }
}
```

A bare module reference means the implicit `#default` instance. The instance
key scopes surface bookkeeping, the settings namespace (per-instance props,
[08 §3](08-settings.md)), and `self.storage`. No new mechanism — the existing
per-instance scoping keys on.

### 5.2 Composition modules

**Status: shipped.**

A profile alone is a config file: no version, no dependencies, no lock entry,
no capability review. A **composition module** is a profile that is also a
module, so a whole shell family can be installed, published, pinned, updated,
rolled back, and forked with the machinery modules already have.

```json
{
  "name": "@alice/desk",
  "version": "2.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "composition",
    "extends": "@mesh/desk",
    "uses": {
      "modules": { "@mesh/navigation-bar": "^3.0.0" },
      "sources": {
        "@mesh/navigation-bar": { "git": "https://github.com/mesh/navigation-bar", "ref": "v3" }
      }
    },
    "compose": {
      "roots": {
        "@mesh/navigation-bar#top": { "module": "@mesh/navigation-bar",
                                      "surface": { "anchor": "top" } }
      },
      "providers": { "mesh.audio": "@mesh/pipewire-audio" },
      "resources": { "theme": "@alice/desk-theme", "icons": ["@mesh/icons-default"] },
      "slots": {
        "mesh.settings.page": {
          "replace":  { "@mesh/audio": "@alice/desk-audio-page" },
          "suppress": ["@mesh/navigation-bar"],
          "order":    ["@alice/desk-audio-page", "@mesh/network"]
        }
      },
      "settings": { "shell": { "i18n": { "locale": "en-US" } } }
    }
  }
}
```

Rules:

- **A composition binds; it never owns.** It selects a provider, it does not
  contain one. Backends are effectively machine singletons while compositions
  are swappable, so a family that owned its audio backend would restart audio on
  every switch. Durable service data stays shared; configuration stays scoped.
- **A composition holds no privilege.** `mesh.uses.capabilities` on a
  composition is a hard error. It can only select among what its members already
  declare; install shows the union of the closure as a capability diff. Without
  this a composition becomes the privileged layer replaceable modules exist to
  avoid.
- A composition declares no `entry`, no `mesh.surface`, and no `mesh.implements`.
- **A composition recommends a theme; it never sets one.** `resources.theme`
  (a theme module id) applies only while the user has not chosen a theme, so
  switching shells keeps the user's theme. `compose.settings.shell.theme` is a
  validation error. A profile may still pin a theme in its own settings
  ([08 §1.1](08-settings.md#11-profile-scope)).
- `extends` forks a family: the base composition's decisions plus the deltas you
  disagree with. Cycles are rejected.
- `mesh.uses.sources` says where to fetch a dependency that is not installed. A
  registry later fills the same map from an index.

### 5.3 Shell profiles and live switching

**Status: shipped** for profile documents, scoped preferences, multiple root
instances, activation closure, transactional live switching, and composition
instantiation. Built-in management interfaces are described in §5.4; broader
tooling and API coverage follows each detailed specification.

A profile is an **instance of a composition plus the user's deltas**:

```json
{
  "schemaVersion": 3,
  "from": { "module": "@alice/desk", "version": "2.1.0" },
  "roots":    { "@mesh/navigation-bar#top": { "active": false } },
  "settings": { "shell": { "i18n": { "locale": "sk-SK" } } }
}
```

`from` is optional: a profile without it is a hand-built composition — every
field is the whole decision rather than a delta. Layering runs base composition
→ derived composition → profile, most specific winning per field, with two
deliberate exceptions:

- **Ordered resource chains replace rather than merge.** An icon or font chain
  is an ordered fallback list; interleaving two orderings has no meaning. An
  omitted chain inherits; an explicit empty chain (`"icons": []`) selects no
  packs.
- **A user may deactivate an inherited root but not delete it.** Deletion is not
  expressible in a delta layer, and an update would resurrect it anyway.

Both composition layers and profiles accept `removeBackgroundServices`
(module ids) and `removeProviders` (canonical interface names). Each layer
subtracts those inherited entries before applying its own additions. A more
specific explicit service or provider choice may restore an entry. Removing a
provider binding clears that selection; ordinary dependency and sole-provider
inference still apply. These removals never disable a required dependency.

Profiles also persist `slots` overrides, so detaching a composition preserves
extension-point replacement, suppression, and ordering. Forced uninstall of a
composition, including an `extends` ancestor, resolves every affected profile
before removal and stores its complete effective composition without `from`.
Roots, settings, resources, and both kinds of slot overrides survive; orphaned
root overrides remain stored as inactive roots. Profile revisions advance
through the normal transaction. Forced removal of an
inherited service uses subtraction deltas while retaining composition provenance.

A user override keyed to a root the composition no longer declares is
**retained and reported** (`orphaned_profile_override`), never dropped —
discarding it would lose the user's work on every upstream rename.
`mesh profile prune` clears them on request.

Profiles are stored as `profiles/<id>.json`; `active-profile` contains the
selected id. Their positive root list is not an installed-module allow-list:
only user-composed roots and explicit choices are stored, while declared module
dependencies, resource dependencies, interface modules, and a sole compatible
provider are inferred. The installed directory remains the complete available
catalog.

```json
{
  "schemaVersion": 1,
  "roots": {
    "@alice/weather#default": {
      "module": "@alice/weather",
      "entrypoint": "main",
      "active": true
    }
  },
  "backgroundServices": [],
  "providers": {},
  "resources": { "icons": [], "fonts": [], "languages": [] },
  "settings": {
    "shell": { "i18n": { "locale": "en-US" } },
    "@alice/weather#default": { "props": { "global": { "units": "metric" } } }
  }
}
```

Presence composes an instance; `active: false` temporarily removes it while
preserving identity and overrides. Every root field is sparse in an overlay: an
overlay that omits `active` (for example, one that only moves the surface)
inherits the less-specific decision, and a root no layer decides about is
active. `surface` on an instance is sparse: omitted
fields inherit `mesh.surface`, and only user changes are written. Installing a
frontend directly creates or re-enables its `#default` instance. Component,
interface, and library modules are availability/dependency nodes rather than
independently enabled units.

Schema 3 adds sparse nodeSlots, keyed by root instance and then by the
component-local slot name. Each record holds an ordered nodes list; every node
has a stable id, a use reference in source-module-id:contribution-id form, and
literal public prop overrides.

Each list replaces the less-specific list wholesale. An explicit empty list
empties the slot, while an absent key inherits composition or author defaults.
Placement ids preserve component VM and storage identity across reorder. The
contribution must satisfy the slot contract, and props validate against its
exposed props. Selected modules join the activation closure and gain no
capabilities from their placement.

A profile is a declarative starting point for one shell composition. It lists
root component instances and their surface placement, explicit provider choices
where several compatible providers exist, resource selections, root background
services, and profile-scoped overrides. It is not a systemd-like unit graph:
there are no process users, privilege levels, restart policies, or general
ordering language.

Required services are inferred from component interface dependencies. A sole
compatible provider may be selected automatically; ambiguity requires an
explicit profile binding. Components with missing optional services degrade
locally. A required service never rejects the whole candidate; it affects only
its consumers:

- **No provider installed:** the consumer activates and sees the interface's
  health as `unavailable` until a provider is installed or enabled.
- **Providers installed but none can serve it** (incompatible version,
  ambiguous without a binding, or no valid typed contract): the consumer is
  blocked with `interface_dependency_blocked`.

Unrelated modules activate either way. [02 §3](02-installation.md) applies the
same rule at install time.

Startup and live switching resolve the complete `extends` chain and profile
before mounting frontend roots or computing effective settings. Inherited roots,
surface overrides, settings, resources, and slot placements therefore have the
same meaning in both paths. Resolution preserves the stored profile revision;
it never writes inherited defaults into the sparse profile. Shared-settings
reloads apply the same resolved composition. `resources.theme` remains a
recommendation beneath the user's shared theme choice; profile theme settings
explicitly pin a theme.

Live profile switching is transactional: validate the candidate, diff it
against the active graph, retain identical service instances, initialize new
services and hidden surfaces, reveal the new roots, then remove orphaned
objects. Failure before commit leaves the active profile untouched.

`mesh-shell profile use <id>` requests that transaction from a running shell
over its private IPC socket; when no shell is running it updates the pointer for
the next start. `profile set` and `profile unset` edit sparse profile settings.

### 5.4 Core-provided interfaces

**Status: shipped.**

The shell is the provider for five interfaces. They are declared, resolved, and
capability-checked exactly like a backend module's, and they are how a module
changes composition or configuration:

These are core-owned platform mechanisms. Settings, devtools, and package UIs
are ordinary `.mesh` components consuming them; replacing a UI does not replace
the authoritative settings engine or transaction machinery. See
[00 §2](00-philosophy.md#2-core-owns-platform-invariants-modules-own-experiences).

| Interface | Methods | Capability |
| --- | --- | --- |
| mesh.composition | apply_node_slot, reset_node_slot | service.composition.control |
| `mesh.packages` | `install`, `uninstall`, `set_module_enabled`, `set_provider`, `switch_profile` | `service.packages.control` |
| `mesh.settings` | `set_prop`, `unset_prop` | `service.settings.control` |
| `mesh.theme` | `set_theme`, `set_icon_theme`, `set_font_family`, `set_backdrop_image` | `service.theme.control` |
| `mesh.locale` | — (state projection; host read/write APIs) | `locale.read` / `locale.write` |

The fifth core interface, mesh.composition, publishes the active profile
generation, roots, named slots, effective/default placements, and compatible
contribution palette. Its apply_node_slot and reset_node_slot methods require
service.composition.control. Writes replace or reset one complete ordered list
and include the observed generation; stale writes fail with
node_edit_generation_conflict. It exposes no profile paths or editor-specific
canvas model, so a visual editor remains an ordinary `ui` module.

The capability is the whole gate. **No module id is consulted**, so a
third-party settings frontend that declares `service.packages.control` has the
same reach as `@mesh/settings`, and `@mesh/settings` without the capability has
none. This is what makes the settings experience replaceable rather than merely
mountable.

Locale writes remain on the `mesh.locale.set` host API, which enforces
`locale.write` before publishing the shell request. The `mesh.locale` service
projection is read only and carries the committed locale, fallback chain,
direction, selection revision, policy, and durable control-plane revision.

A command whose payload does not match the declared arguments is reported as
unsupported and applied nowhere — core-provided methods never substitute a
default for a malformed argument.

## 6. Scripting model

**Status: shipped.** Backend and frontend scripts run in a real Luau VM
(`mlua`); hand-written parsing/interpreting is migration debt to remove.

- `local` = private; bare non-local assignments = public reactive members.
- `self.meta` = instance identity/diagnostics; `self.storage` = shell-backed
  persistent JSON-like document scoped to the module/component/provider
  instance (loads before lifecycle code; flushes on unmount/stop; tracked
  reads rerender only affected components).
- `require`/`import` are the single resolver for builtin `mesh.*` libraries,
  interface proxies, Luau libraries (`@scope/kit/file`), and component
  definitions (`./x.mesh`, `@scope/name`).
- **Shipped — frontend Luau libraries.** `require("@scope/name/path")` loads
  `path.luau` under a `mesh.provides.libraries` root (`@scope/name` alone loads
  `init.luau`) of a module the requiring module lists in `mesh.uses.modules`.
  Each component context evaluates a library once, in a private environment
  that reads through to the caller's, so library globals never become reactive
  members. Paths cannot leave the root; sources are limited to 1 MiB; a cycle
  is an error. Libraries run with the caller's capabilities and grant none.
  Backend contexts do not resolve libraries yet. `import(spec, ...names)` returns
  named fields as multiple values.
- Ambient `mesh` global keeps genuinely ambient backend powers (`mesh.exec`,
  `mesh.service`, `mesh.config`); discoverable subsystems prefer explicit
  `require`.
- Backends expose `start(self)` (setup, poll registration), optional
  `on_poll(self)`, `on_command_<method>()` returning `{ ok = true }` /
  `{ ok = false, error = "…" }`, and fire declared events via
  `self.EventName:fire(payload)`.
- **Shipped — recursive contract types.** Named types may recurse through an
  optional field or an array, allowing finite trees and linked values. Required
  scalar cycles are rejected. Structural validation checks every member, with a
  depth limit of 64 and a traversal budget of 65,536 values per type check;
  runtime JSON boundaries may impose stricter limits.
- **Shipped — backend tasks.** Backends have the frontend `task` library
  (`task.spawn`, `task.delay`, `task.sleep`, `task.cancel`) once `start(self)`
  runs. `start`, `on_poll`, `on_command_<method>`, stream hooks and `stop` run as coroutines, as do
  tasks: inside them `mesh.exec`, `mesh.socket_request` and `mesh.read_file`
  (`fs.read:<pattern>`, absolute regular files up to the output limit) suspend the
  callback while a host worker runs, and other callbacks keep running. The
  provider becomes ready when `start` finishes; a poll that suspended
  finishes before the next poll starts; a suspended command replies when its
  handler returns and is dropped if the caller cancels. Only successful
  callback completion publishes a service snapshot. Failed callbacks discard
  pending publications while retaining private Luau mutations and table identity;
  the next successful callback may publish those mutations. Later stream callbacks
  remain queued while a stream callback is suspended. Stop cancels previous tasks
  and has a two-second teardown deadline. Calls from contexts Luau cannot yield
  from (metamethods, `table.sort` comparators) still block.
- **Shipped — native backend events.** Capability-gated D-Bus signals, PipeWire
  Node/Metadata invalidations, route netlink groups, wlr foreign-toplevel events,
  inotify watches and Unix socket streams use the bounded, generation-owned
  stream lifecycle. Sources register asynchronously, report readiness/failure,
  and are cancelled and awaited at shutdown; individual handles can be closed.
  `mesh.service.set_poll_interval(0)` disables polling. Shipped NetworkManager,
  UPower and PipeWire providers prefer native subscriptions and retain monitor
  and safety-poll fallbacks. Domain state reads and controls remain module-owned.
  API filters, record formats, limits and fallback behavior are in
  [Backend host events](../modules/backend-events.md).
- Core-triggered service actions use that same declared method and generic
  dispatcher. For example, startup playback calls `mesh.audio.play_sound`
  rather than sending directly to a known backend handler.
- Libraries wrap host APIs; host APIs stay generic. Good:
  `@mesh/backend-kit/process` wraps `mesh.exec`. Bad: Rust core adds
  `mesh.audio.get_volume()`.

## 7. Capabilities & security

**Status: partially shipped** (capability model, sandbox policy, typed trust
tiers, lock provenance metadata, root-graph minimum-tier enforcement, and
detached Ed25519 verification against root-graph trust anchors). Registry key
distribution remains target work.

A capability is a named permission for a host API or service operation.
Required capabilities must all be granted or the module does not load;
optional ones may be denied and
the module must degrade. Enforcement is by construction: the Luau environment
only exposes API functions for granted capabilities; there is nothing to call
without the grant.

- **Consumer capabilities** (`service.audio.read`, `service.audio.control`)
  belong to frontends/automation consuming an interface; they are declared in
  the interface contract's `[capabilities]`.
- **Provider capabilities** are the generic host powers an implementation
  needs (`exec.argv:wpctl:["status"]`, `dbus.read:…`, `net.http`). A provider declaring a
  consumer capability for an interface it implements gets
  `provider_declares_consumer_capability` with a removal action.
- Executable provider access uses `exec.argv:<program>:<json-args>` grants.
  The host resolves `<program>` through `PATH` or an explicit path, stores its
  canonical target, and matches the complete argument vector. A `*` argument
  is an explicit single-argument glob; a JSON `*` permits any argument vector.
  A `*` inside an argument matches only ASCII letters, digits and `._/@:+=-`,
  so it cannot end the string, word or address it sits in. Grants for
  interpreters and program launchers (`sh`, `python*`, `env`, `awk`, …) are
  rejected when a wildcard reaches code: the only accepted form is a shell
  running an exact `-c` script with whole-argument `*` positional values after
  it (`["-c","test -S \"$1\"","sh","*"]`). Basename grants such as `exec.wpctl` are not executable policy. The explicit
  high-risk `exec.command` capability remains the unrestricted override.
- **Shipped — scoped D-Bus calls.** `dbus.call:<bus>:<destination>:<interface>`
  permits `mesh.dbus_call` to one interface at one bus name, or at any name with
  `*` (for per-client services such as StatusNotifier items). `<bus>` is
  `system` (High) or `session` (Elevated).
- **Shipped — scoped D-Bus reads and signals.** `dbus.read:<bus>:<destination>:<interface>`
  permits only `Properties.Get`/`GetAll` for that interface and `Introspect` at
  that name; `dbus.subscribe:<bus>:<sender>` permits `mesh.dbus_subscribe`
  for that sender. Both are Standard on the system bus and Elevated on the
  session bus. Dynamic-client reads may use `dbus.read:<bus>:*:<interface>`;
  dynamic signal sources use `dbus.subscribe:<bus>:*:<interface>` and require
  that exact `filter.interface`. Neither form permits another interface or bus;
  an unqualified `dbus.subscribe:<bus>:*` remains invalid.
- **Shipped — scoped files.** `fs.read:<pattern>` and `fs.watch:<pattern>`
  permit `mesh.read_file` and `mesh.watch_path` on matching paths. Patterns
  are absolute or start with `~/` (home); `*` matches within a segment and a
  `**` segment matches any number of segments. Paths match as written after
  dropping empty and `.` segments; `..` never matches, and symlinks inside a
  granted path are followed. Patterns rooted at `/**` or `~/` are Elevated,
  others Standard. There is no unscoped `fs.read` or `fs.watch`.
- **Shipped — plain-language permissions.** Every grant has a short
  description (`describe_capability`): "run `nmcli device wifi rescan`", "read
  battery status", "watch org.freedesktop.UPower for changes". Install
  approval errors include it, and `mesh-shell permissions <module>` (also
  printed after `install`) lists a module's grants by level.
- **Shipped — HTTP.** `net.http` permits backend `mesh.http_request(spec)`:
  bounded HTTP/HTTPS requests with verified TLS, deadlines and redirects. The
  call yields in a coroutine. `cache = true` returns a module-scoped cached
  file for binary data rather than exposing arbitrary filesystem writes. See
  [backend host APIs](../modules/backend-events.md#http-requests).
- **Shipped — owned D-Bus names.** `dbus.own:<bus>:<name>` permits
  `mesh.dbus_serve` to own one well-known name and export one interface at one
  object path, including declared read-only properties; `<bus>` is `system`
  (High) or `session` (Elevated). See
  [backend host events](../modules/backend-events.md#d-bus-services).
- **Shipped — exec locale.** Exec children run in `LC_ALL=C.UTF-8` so a
  provider parses the same tool output on every host. A call can ask for
  another locale with an options table, `mesh.exec(program, args, { locale =
  "sk_SK.UTF-8" })` (also on `mesh.exec_stream`). `"inherit"` keeps the shell's
  own environment. The option requires the elevated `exec.locale` capability,
  sets only `LC_ALL`, and accepts names of letters, digits and `_ . @ -` up to
  64 bytes; no other environment variable is settable.
- **Shipped — application launch.** `exec.launch-app` exposes
  `mesh.launch_app(desktop_id)`, which finds `<desktop_id>.desktop` under
  `$XDG_DATA_HOME/applications` and `$XDG_DATA_DIRS/*/applications`, expands
  its `Exec` per the Desktop Entry Specification (quoting, escapes, field codes
  for a launch with no files), honours `Path`, and starts it in its own process
  group with no inherited stdio. It returns `{ ok = true }` or
  `{ ok = false, error = … }`; `Terminal=true`, hidden and non-application
  entries are refused. The function does not exist without the grant.
- **Shipped:** the core catalog explicitly enumerates host powers and the
  built-in service permissions; unknown names are rejected.
- **Shipped — interface-declared service permissions.** Host powers remain
  closed, while an interface contract may declare its own consumer permissions
  under `capabilities.permissions`, each with a privilege level:

  ```json
  "capabilities": {
    "read": ["service.weather.read"],
    "methods": { "refresh": ["service.weather.refresh"] },
    "permissions": {
      "service.weather.read": "standard",
      "service.weather.refresh": "elevated"
    }
  }
  ```

  A permission must live under the declaring interface's namespace
  (`service.<name>.` with `mesh.` stripped, so `mesh.weather` owns
  `service.weather.*` and `acme.sensors` owns `service.acme.sensors.*`), may
  not collide with a built-in or another interface's permission, and must be
  `standard`, `elevated`, or `high`. A refused declaration is an
  `invalid_service_permission` diagnostic and is left out. Valid ones join the
  graph's capability catalog: consumers request and are granted them like any
  other capability, install review classifies them at their declared level,
  and the runtime enforces them through the contract's read, event, and method
  policies. They authorize access to that service only and never grant a host
  power, so new service contracts need no Rust changes to authorize their
  operations.

Privilege levels (fixed set, part of install UX):

| Level | Meaning | Examples |
| ----- | ------- | -------- |
| `standard` | Safe read access | `theme.read`, `service.audio.read`, `locale.read` |
| `elevated` | Meaningful system interaction; confirm at install | `service.network.control`, `exec.launch-app`, `net.http` |
| `high` | Powerful/sensitive; explicit opt-in with warning | `exec.command`, `exec.argv:…`, `shell.screenshot`, `dbus.call:system:…`, `automation.act` |

Trust tiers: `core` (shipped, reviewed), `verified` (reviewed + signed),
`community` (unreviewed, user accepts risk), `local` (developer path, no
signature). The tier comes from provenance: an unsigned git source is
`community` and an unsigned path source is `local`, whatever its id. The
`@mesh/` scope is reserved for modules shipped with MESH; installing a git or
path source under it is rejected, so only the unlocked shipped tree is `core`.
A detached `module.sig` contains the key id, algorithm, and
base64 signature over the canonical module id/version/digest payload. Keys are
configured under the root graph's `trustPolicy.keys`; an unknown key, changed
payload, unsupported algorithm, or malformed signature blocks the module before
dependency, provider, or frontend contribution activation. Threats and
mitigations: capability sandbox (no ambient fs/net/process in Luau),
core-owned grant decisions, execution budgets/isolation for resource abuse,
reserved `@mesh` scope, capability-diff re-approval on update. The root graph may set
`trustPolicy.minimum`; graph construction blocks lower-tier modules before
dependency, provider, or frontend contribution activation. Lock entries retain
the tier and detached signature metadata, and `verified` entries must carry a
signature that a configured key verifies.

Permission presentation is a separate platform concern. A core-owned decision
does not itself make a dialog unspoofable or impossible to overlay on Wayland.
Any stronger trusted-presentation guarantee remains target design requiring an
identified compositor/host mechanism; ordinary management UI stays in components.

## 8. Module lifecycle

**Status: shipped in outline** (discovery, load, run, error placeholders,
hot-reload of settings/sources); suspension is best-effort.

```
Discovered → Resolved → Loaded → Initialized → Running ⇄ Suspended → Unloaded
                                      └────────────→ Errored
```

- **Discovered**: manifests read, no code runs; invalid manifests are logged
  and skipped. **Resolved**: dependency graph checked; cycles and
  unsatisfiable deps rejected with diagnostics. **Loaded**: sources parsed,
  nothing executed. **Initialized**: `start()`/component mount with granted
  capabilities and scoped context. **Running**: events, state, paints.
  **Errored**: error logged with context, UI replaced by a bounded placeholder
  (a broken module must not expand or blank its host surface), dependents
  notified; repeated crashing disables the module until re-enabled.
- UI modules are compiled at startup; dev hot-reload watches sources and
  settings.
- **Execution boundary:** modules run within the shell process, with private
  environments and explicit public communication. Current same-thread Luau
  contexts share a sandboxed VM. Script errors must be logged and displayed
  locally with bounded failure handling; recovery, capability enforcement, and
  resource accounting are distinct obligations. This is not a guarantee against
  native process crashes or process-wide exhaustion.
- **Language direction:** Luau through `mlua` is current. Whether
  TypeScript/JavaScript replaces or supplements it is undecided. WASM and native
  Rust module tiers are not committed targets. Contracts remain declarative
  and language-independent; see [00 §7](00-philosophy.md#7-runtime-and-language-direction).

## 9. Diagnostics are part of the contract

**Status: shipped.**

Every gap must be visible with a concrete author action: missing providers,
missing/optional icons, unresolved resources, undeclared events, capability
misdeclarations, manifest typos, binary availability. Diagnostics name the
module id, field path, and replacement. The debug inspector's Modules tab and
the settings UI render each module's uses/provides graph: required interfaces,
active provider, icons, binaries, capabilities, keybinds, health.

## 10. Authoring workflow (the golden path)

1. `module.json` with everything under `mesh`.
2. UI = `frontend` module consuming interfaces by contract name.
3. Contract = explicit inline declaration or `interface` module (§4); shared
   interface props remain target work.
4. Implementation = `backend` module with `mesh.implements` + generic host
   capabilities + declared binaries.
5. Wiring = profile roots and explicit choices, with dependency closure and
   sole-compatible-provider resolution; the root graph retains the legacy fallback.
6. Configuration = `<props>` / in-script props; users override through the
   settings store and generated UI.
7. Resources = semantic names resolved through packs the user controls.
