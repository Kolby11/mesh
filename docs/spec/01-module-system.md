# 01 — Module System

> Part of the [MESH Specification](README.md).

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
| module kind | The module's primary role: `frontend`, `backend`, `interface`, `component`, `composition`, `library`, `theme`, `icon-pack`, `font-pack`, `language-pack`. |
| element | Base UI primitive exposed by MESH core (`box`, `button`, `icon`, …). |
| component | User-authored reusable `.mesh` unit composed from elements/components. |
| interface | Named, versioned contract: state fields, methods, events, types, consumer capabilities. Data, not code. |
| extension point | Named, versioned UI contract: a region one module hosts and other modules fill (§4.3). |
| composition | Installable module selecting roots, providers, resources, and slot arrangement (§5.2). |
| provider | Backend module implementation of an interface. |
| contribution | Something a module adds to the installed graph (`mesh.provides.*`). |
| dependency | Something a module needs (`mesh.uses.*`). |
| capability | Host power granted to a module (`shell.surface`, `exec.wpctl`, …). |
| resource pack | Module kind contributing semantic-name → asset mappings. |
| library | Module contributing importable Luau code. |
| entrypoint | Named launch/UI entry contributed by a module. |

## 2. Design rules (non-negotiable)

1. **Everything is a module.** One installable unit, one manifest shape. The
   defaults shipped in `@mesh` scope hold no privileged status.
2. **The core is a wiring layer.** It discovers modules, validates manifests,
   routes interface/provider records, and forwards events. Service behavior
   (audio, network, power, …) lives exclusively in Luau provider modules. A
   `if service == "audio"` branch in Rust is a bug.
3. **Frontends depend on contracts, never on backend module IDs.**
4. **Modules own their derived state.** Backends emit raw data; frontends
   compute display state (icon names, labels) in their own scripts. The core
   never injects computed display fields into service payloads.
5. **Capabilities are explicit.** No capability inference — auditability by
   reading the manifest is a feature. Redundant/derivable declarations are
   deleted from the vocabulary instead (a provider never restates its
   interface's consumer capabilities).
6. **One model, cheap path.** No parallel "lite" authoring modes. Where
   ceremony hurts, the single path gets cheaper (sole-implementer
   auto-selection, optional contract files), not duplicated.
7. **Ergonomic-simple must not cost conceptual-simple.** Deleting boilerplate
   is good only when the system stays explainable from what's on disk.

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
    "kind": "frontend",
    "entry": "src/main.mesh",
    "uses": {
      "modules": { "@alice/volume-popover": ">=0.1.0" },
      "interfaces": { "mesh.audio": ">=1.0" },
      "resources": { "icons": ["@mesh/icons-default"] },
      "capabilities": ["shell.surface", "service.audio.read", "service.audio.control"],
      "iconRequirements": { "required": ["audio-volume-muted", "audio-volume-high"] }
    },
    "provides": {
      "layout": [{ "id": "main", "entrypoint": "src/main.mesh", "label": "Volume Panel" }],
      "i18n": [{ "id": "en", "locale": "en", "path": "config/i18n/en.json" }]
    },
    "surface": { "anchor": "top", "exclusive_zone": 56 },
    "accessibility": { "role": "toolbar" },
    "i18n": { "defaultLocale": "en", "supportedLocales": ["en"] },
    "keybinds": {
      "mute": {
        "label": { "t": "keybind.mute.label", "fallback": "Mute audio" },
        "trigger": { "kind": "shortcut", "key": "m" }
      }
    }
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
- `mesh.provides` holds everything the module *contributes*: layout entries,
  i18n catalogs, libraries, themes, fonts, icons.
- `mesh.implements` is only for backend provider records.
- The validator keeps buckets strict: module/resource deps are `@scope/name`
  ids; interface deps are dotted contract names (`mesh.audio`); capabilities
  are host-power names (`service.audio.read`, `exec.wpctl`).
- Old manifest inputs (`package.json`, `mesh.toml`, `plugin.json`, legacy
  top-level `id/type/api_version`) **fail loading** with a replacement
  diagnostic. Multiple manifest files in one module fail until resolved.

### 3.1 Closed core, open provides

**Status: target.**

Core `mesh` fields (`kind`, `entry`, `uses`, `implements`, `surface`,
`accessibility`, `keybinds`, `i18n`, `theme`, pack sections) are a **closed
schema**: unknown core fields and near-miss typos produce diagnostics.
`mesh.provides.*` and `mesh.uses.resources.*` are **open namespaces**: unknown
contribution kinds are preserved in the installed graph as typed opaque records
so third-party tools/modules can define new contribution kinds without a core
release. Superseded manifest sections in code (`ServiceSection`,
`DependenciesSection`, `AssetsSection`, `ExportsSection`, `IconsSection`, dead
generations in `model.rs`) are deleted outright.

### 3.2 Module kinds

| Kind | Purpose | Kind-scoped sections |
| ---- | ------- | -------------------- |
| `frontend` | `.mesh` UI surfaces/widgets for a shell feature | `mesh.surface` (placement), `mesh.accessibility`, `mesh.keybinds`, `mesh.theme` |
| `backend` | Provider implementing interfaces (Luau `main.luau`) | `mesh.implements`, `mesh.uses.binaries`, in-script `props {}` |
| `interface` | Data-only contract package | `mesh.interface` |
| `component` | Embeddable `.mesh` component; **no** `mesh.surface`; consumed via `require("@scope/name")` | — |
| `composition` | An installable shell composition: roots, provider bindings, resources, extension point arrangement | `mesh.compose`, `mesh.extends` |
| `library` | Importable Luau helpers; grants no capabilities | `mesh.provides.libraries` |
| `theme` | Theme tokens + component defaults (CSS) | `mesh.provides.themes` — see [04](04-styling.md) |
| `icon-pack` | Semantic icon name → asset mappings | `mesh.icon_pack` — see [05](05-icons.md) |
| `font-pack` | Font role → installed family mappings | `mesh.font_pack` — see [06](06-fonts.md) |
| `language-pack` | Translation catalogs for other modules | `mesh.provides.i18n` — see [07](07-i18n.md) |

### 3.3 Surface placement (`mesh.surface`)

**Status: shipped.**

`mesh.surface` carries **placement only** — the compositor concerns CSS cannot
express. Declare only deltas; omitted fields fall back to core defaults.

`role` picks which compositor protocol realizes the surface, and thereby which
of the other fields apply:

| `role` | Protocol | Placement fields |
| ------ | -------- | ---------------- |
| `layer` (default) | `zwlr_layer_shell_v1` — shell chrome: panels, launchers, overlays | `anchor`, `layer`, `exclusive_zone`, `keyboard_mode`, `margins` |
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

The user can move a surface between roles through the sparse settings store
(`"surface": { "role": "window" }`, [08](08-settings.md)) — the author's `role`
is a default like every other placement field.

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
| `mesh.events.publish("shell.set-surface-role", { surface_id, role })` | in-surface controls (a "pop out" button) |
| `shell.toggle-surface-role` on the same channel | a control that flips whichever role is current |
| `shell:promote_surface:<id>`, `shell:demote_surface:<id>`, `shell:toggle_surface_role:<id>` over the automation IPC | a *compositor* keybind, since MESH keybinds are focused-surface actions and cannot grab a global hotkey ([11](11-automation-ipc.md)) |

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
`shell.promote-widget` / `shell.demote-widget`, each carrying the owning
`surface_id` and retained `node_key`; `shell.set-widget-role` is the explicit
role form. Closing the promoted window demotes the widget back into its parent.

Surface **sizing is CSS**: the laid-out box of the component root
(`width: 100%` spans the anchored edge, `fit-content` shrinks to content,
`min-*`/`max-*` clamp), measured by `measure_content_size()`. The show/hide
transition is a CSS `transition` on the root. There are no manifest sizing
fields and no compatibility aliases. See [03 — Components](03-components.md).

The two roles size in **opposite directions**, which is the one thing a
component author must know before setting `role: "window"`:

- A **layer surface** tells the compositor its CSS-measured size.
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
  interface's is its contract name. *(Target — `provides.settings` is a
  deletion target wherever it still parses.)*
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
- **Consumer capabilities** — what a *consumer* needs to read/control it.
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
            "coalesce": true,
            "stateBinding": { "field": "percent", "fromArg": "percent" }
          },
          {
            "name": "set_muted",
            "args": [
              { "name": "device_id", "type": "string" },
              { "name": "muted", "type": "boolean" }
            ],
            "returns": "Result",
            "coalesce": true,
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
        "capabilities": { "required": ["service.audio.read"] }
      }
    }
  }
}
```

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
feature groups. Tooling compiles inline and external forms into the same
canonical `InterfaceContract` representation. Runtime validation and LSP
completion use its strict Luau field, argument, return, and event-payload types.
Provider stubs, mocks, standalone documentation, and compatibility reports
remain target tooling.

- **Type grammar.** Every `type`/`returns` expression is validated at graph
  build: primitives (`string`, `int`, `float`, `boolean`, `object`, `any`),
  PascalCase named types declared under `contract.types` (plus the builtin
  `Result`), with `[]` (array) and `?` (optional) suffixes. Invalid
  expressions or references to undeclared types produce
  `invalid_interface_contract` diagnostics and the interface loads without a
  typed contract.
- **Inline declaration for single-provider domains.** A backend module may
  declare its interface contract itself under `mesh.interfaces[]` (same
  shape as `mesh.interface`) — no separate interface module needed. A
  standalone interface module always wins over inline duplicates of the same
  name; duplicate inline declarations resolve to the highest-priority
  provider's copy, and every conflict emits
  `duplicate_interface_declaration`. Promote an inline contract to a
  standalone interface module once a second provider exists.
- `mesh.interface.contract` is **optional** for v0: the contract can be
  inferred from the provider's emitted state, and a backend may implement an
  interface with **no declaration at all** (name in `mesh.implements` with no
  `baseModule`). Interface modules without a contract report
  `missing_interface_contract`.
- **Reactive command state.** A method may declare
  `"stateBinding": { "field", "fromArg" }` or
  `"stateBinding": { "field", "toggle": true }`: on successful dispatch the
  shell updates that field in the interface's canonical shared state,
  publishes it to every observer, retains it across stale provider snapshots,
  and releases the pending binding when the provider confirms the value.
- Do not put provider identity (`source_module`) in contract state — that is
  runtime metadata.

### 4.1 Versioning

Interfaces follow semver. Major = breaking (prefer a new name, `mesh.audio.v2`,
so old consumers keep working); minor = additive; patch = clarification. The
registry indexes each `(interface, version)` pair independently; a backend may
advertise several versions at once during migrations; consumers request ranges
(`require("mesh.audio@>=1.0")`).

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
  root of the contributing module**: its own VM, its own capabilities, its own
  settings namespace, rendered inside the host's tree. A broken contribution
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
- **Unowned shell channels** (`shell.toggle-surface`, `shell.show-surface`, …)
  are published through `mesh.events` by any module holding the capability.
  Interface-domain commands must go through the interface proxy, not raw
  channel publishes (`raw_interface_domain_event_publish` diagnostic);
  unknown `shell.*` publishes report `unknown_shell_event_publish`.

  `shell.*` carries **surface and debug requests only**. Changing composition
  or configuration is not a shell channel: those are methods on core-provided
  interfaces (§5.4), because a channel cannot express argument types, cannot
  be capability-checked per operation, and cannot report a malformed payload.

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

### 5.1 Multiple instances of a frontend module

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
- `extends` forks a family: the base composition's decisions plus the deltas you
  disagree with. Cycles are rejected.
- `mesh.uses.sources` says where to fetch a dependency that is not installed. A
  registry later fills the same map from an index.

### 5.3 Shell profiles and live switching

**Status: shipped** for profile documents, scoped preferences, multiple root
instances, activation closure, transactional live switching, and composition
instantiation. Typed profile/package service contracts for replaceable settings
frontends remain target behavior.

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
  is an ordered fallback list; interleaving two orderings has no meaning.
- **A user may deactivate an inherited root but not delete it.** Deletion is not
  expressible in a delta layer, and an update would resurrect it anyway.

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
preserving identity and overrides. `surface` on an instance is sparse: omitted
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
locally; missing required contracts reject the candidate before activation.

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

| Interface | Methods | Capability |
| --- | --- | --- |
| mesh.composition | apply_node_slot, reset_node_slot | service.composition.control |
| `mesh.packages` | `install`, `uninstall`, `set_module_enabled`, `set_provider`, `switch_profile` | `service.packages.control` |
| `mesh.settings` | `set_prop`, `unset_prop` | `service.settings.control` |
| `mesh.theme` | `set_theme`, `set_icon_theme`, `set_font_family` | `service.theme.control` |
| `mesh.locale` | — (state only) | — |

The fifth core interface, mesh.composition, publishes the active profile
generation, roots, named slots, effective/default placements, and compatible
contribution palette. Its apply_node_slot and reset_node_slot methods require
service.composition.control. Writes replace or reset one complete ordered list
and include the observed generation; stale writes fail with
node_edit_generation_conflict. It exposes no profile paths or editor-specific
canvas model, so a visual editor remains an ordinary frontend module.

The capability is the whole gate. **No module id is consulted**, so a
third-party settings frontend that declares `service.packages.control` has the
same reach as `@mesh/settings`, and `@mesh/settings` without the capability has
none. This is what makes the settings experience replaceable rather than merely
mountable.

Locale is state-only on purpose: `mesh.locale.set` is a host API that already
enforces `locale.write`. Adding a service method would create a second
capability name for one write.

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
  definitions (`./x.mesh`, `@scope/name`). `import(spec, ...names)` returns
  named fields as multiple values.
- Ambient `mesh` global keeps genuinely ambient backend powers (`mesh.exec`,
  `mesh.service`, `mesh.config`); discoverable subsystems prefer explicit
  `require`.
- Backends expose `start(self)` (setup, poll registration), optional
  `on_poll(self)`, `on_command_<method>()` returning `{ ok = true }` /
  `{ ok = false, error = "…" }`, and fire declared events via
  `self.EventName:fire(payload)`.
- Core-triggered service actions use that same declared method and generic
  dispatcher. For example, startup playback calls `mesh.audio.play_sound`
  rather than sending directly to a known backend handler.
- Libraries wrap host APIs; host APIs stay generic. Good:
  `@mesh/backend-kit/process` wraps `mesh.exec`. Bad: Rust core adds
  `mesh.audio.get_volume()`.

## 7. Capabilities & security

**Status: partially shipped** (capability model, sandbox policy, typed trust
tiers, lock provenance metadata, and root-graph minimum-tier enforcement);
cryptographic signature verification and registry key distribution remain
target work.

A capability is a named permission for a host API. Required capabilities must
all be granted or the module does not load; optional ones may be denied and
the module must degrade. Enforcement is by construction: the Luau environment
only exposes API functions for granted capabilities; there is nothing to call
without the grant.

- **Consumer capabilities** (`service.audio.read`, `service.audio.control`)
  belong to frontends/automation consuming an interface; they are declared in
  the interface contract's `[capabilities]`.
- **Provider capabilities** are the generic host powers an implementation
  needs (`exec.wpctl`, `dbus.system`, `net.http`). A provider declaring a
  consumer capability for an interface it implements gets
  `provider_declares_consumer_capability` with a removal action.
- Capability names are opaque strings; contract packages may introduce new
  ones but must classify each with a privilege level. The core refuses
  contracts introducing unclassified capabilities.

Privilege levels (fixed set, part of install UX):

| Level | Meaning | Examples |
| ----- | ------- | -------- |
| `standard` | Safe read access | `theme.read`, `service.audio.read`, `locale.read` |
| `elevated` | Meaningful system interaction; confirm at install | `service.network.control`, `exec.launch-app`, `net.http` |
| `high` | Powerful/sensitive; explicit opt-in with warning | `exec.command`, `shell.screenshot`, `dbus.system`, `automation.act` |

Trust tiers: `core` (shipped, reviewed), `verified` (reviewed + signed),
`community` (unreviewed, user accepts risk), `local` (developer path, no
signature). Threats and mitigations: capability sandbox (no ambient fs/net/
process in Luau), core-owned trusted chrome (modules cannot draw over
permission dialogs), per-module budgets/isolation for resource abuse, reserved
`@mesh` scope, capability-diff re-approval on update. The root graph may set
`trustPolicy.minimum`; graph construction blocks lower-tier modules before
dependency, provider, or frontend contribution activation. Lock entries retain
the tier and optional detached signature metadata, and `verified` entries must
carry that metadata.

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
- Frontend modules are compiled at startup; dev hot-reload watches sources and
  settings.
- Execution tiers: Luau sandbox is the default and recommended tier. WASM
  (sandboxed compiled) and Rust (in-process, review-gated, toolchain-pinned)
  are **target** tiers; interface contracts are the cross-language seam —
  contracts are data, so per-language bindings can be generated without core
  releases.

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
3. Contract = `interface` module (or inferred, §4) with shared props.
4. Implementation = `backend` module with `mesh.implements` + generic host
   capabilities + declared binaries.
5. Wiring = root graph decisions (often nothing: auto-discovery +
   sole-implementer auto-selection).
6. Configuration = `<props>` / in-script props; users override through the
   settings store and generated UI.
7. Resources = semantic names resolved through packs the user controls.
