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
| module kind | The module's primary role: `frontend`, `backend`, `interface`, `component`, `library`, `theme`, `icon-pack`, `font-pack`, `language-pack`. |
| element | Base UI primitive exposed by MESH core (`box`, `button`, `icon`, …). |
| component | User-authored reusable `.mesh` unit composed from elements/components. |
| interface | Named, versioned contract: state fields, methods, events, types, consumer capabilities. Data, not code. |
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
| `library` | Importable Luau helpers; grants no capabilities | `mesh.provides.libraries` |
| `theme` | Theme tokens + component defaults (CSS) | `mesh.provides.themes` — see [04](04-styling.md) |
| `icon-pack` | Semantic icon name → asset mappings | `mesh.icon_pack` — see [05](05-icons.md) |
| `font-pack` | Font role → installed family mappings | `mesh.font_pack` — see [06](06-fonts.md) |
| `language-pack` | Translation catalogs for other modules | `mesh.provides.i18n` — see [07](07-i18n.md) |

### 3.3 Surface placement (`mesh.surface`)

**Status: shipped.**

`mesh.surface` carries **placement only** — the layer-shell concerns CSS
cannot express: `anchor`, `layer`, `exclusive_zone`, `keyboard_mode`,
`visible_on_start`, `margins`. Declare only deltas; omitted fields fall back
to core defaults.

Surface **sizing is CSS**: the laid-out box of the component root
(`width: 100%` spans the anchored edge, `fit-content` shrinks to content,
`min-*`/`max-*` clamp), measured by `measure_content_size()`. The show/hide
transition is a CSS `transition` on the root. There are no manifest sizing
fields and no compatibility aliases. See [03 — Components](03-components.md).

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

**Status: shipped** (registry, contracts, event validation, relationship
metadata); the contract-file grammar is a restricted TOML dialect and may
still evolve.

An interface is a named, versioned declaration of:

- **State fields** — readable values exposed through the proxy.
- **Methods** — request/response commands routed to the active provider.
- **Events** — typed channels owned by the active provider.
- **Types** — shared structs used by state, methods, events.
- **Consumer capabilities** — what a *consumer* needs to read/control it.
- **Shared props** — user preferences that survive provider swaps
  ([08 §4](08-settings.md)).

```toml
# @mesh/audio-interface / interface.toml
[[methods]]
name = "set_volume"
args = [{ name = "device_id", type = "string" }, { name = "level", type = "float" }]
returns = "Result"

[[events]]
name    = "VolumeChanged"
payload = "{ device_id: string, level: float }"

[capabilities]
required = ["service.audio.read"]
```

Interface modules are data-only packages:

```json
{
  "name": "@mesh/audio-interface",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "interface",
    "interface": {
      "name": "mesh.audio", "version": "1.0", "file": "interface.toml",
      "domain": "audio", "relationship": "base"
    }
  }
}
```

- `mesh.interface.file` is **optional** for v0: the contract can be inferred
  from the provider's emitted state, and a backend may implement an interface
  with **no separate interface module at all** (declare the name in
  `mesh.implements` with no `baseModule`). Promote to a full contract package
  once it's worth sharing. One model, cheaper path.
- Contract-based validation (events, capabilities) applies once a contract
  file exists. Missing declared files report `missing_interface_contract_file`.
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

### 4.3 Events — one communication primitive

Methods are request/response. **Everything asynchronous is a typed event on a
named channel.** There is no second messaging mechanism.

- **Owned channels** are declared inside an interface; only the active
  provider publishes; payloads validate against the contract. Frontends
  subscribe via direct named channels on the proxy
  (`audio.VolumeChanged:on(fn)`).
- **Unowned shell channels** (`shell.toggle-surface`, `shell.set-theme`, …)
  are published through `mesh.events` by any module holding the capability.
  Interface-domain commands must go through the interface proxy, not raw
  channel publishes (`raw_interface_domain_event_publish` diagnostic);
  unknown `shell.*` publishes report `unknown_shell_event_publish`.

Static analysis of `.mesh`/`.luau` sources checks emitted events against the
provider's contract (`undeclared_interface_event_emit`) and static frontend
subscriptions against consumed contracts
(`undeclared_interface_event_subscription`). Runtime delivery validates
declared payload schemas and drops invalid events with a
`service_contract_warning` diagnostic. Dynamic event names stay runtime-only.

## 5. Providers and the root graph

**Status: shipped.**

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
- Libraries wrap host APIs; host APIs stay generic. Good:
  `@mesh/backend-kit/process` wraps `mesh.exec`. Bad: Rust core adds
  `mesh.audio.get_volume()`.

## 7. Capabilities & security

**Status: shipped** (capability model, sandbox policy); signing/registry
enforcement is **target** and deliberately not blocked by installer v1.

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
`@mesh` scope, capability-diff re-approval on update.

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
