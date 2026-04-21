# Extensibility Model

MESH's first priority is **full extensibility**. The shell ships with default
backends and frontends so it works out of the box, but nothing in the default
set is privileged: any user, distribution, or third-party author can replace
*or extend* it — including defining entirely new service categories.

This document describes the dynamic interface registration model that makes
that possible. It supersedes the static, compile-time trait list implied by
earlier drafts of `spec/pluggable-backend.md`.

> **Terminology note.** Earlier docs and the `mesh-service` crate used the
> word *trait* for what this document calls an **interface**. Interfaces are
> data (a contract package declares them); the `mesh-service` crate is being
> repositioned to host the registry, proc-macro, and runtime plumbing rather
> than a fixed list of compiled traits. Both terms may still appear in the
> code — treat them as synonyms during the transition.

The shell core starts with an empty service registry. Default interfaces and
providers are discovered from plugins on disk under the `plugins/` tree, so
the core acts as the bridge and validator rather than the source of service
functionality.

## Design goals

1. The core knows about **the registry**, not about specific services.
2. Any plugin can declare a new interface. Any plugin can implement it. Any
   plugin can consume it.
3. Defaults ship as ordinary plugins with no special status — they can be
   replaced, disabled, or overridden by priority.
4. Cross-language by construction: Luau, WASM, and Rust plugins must be able
   to publish and consume the same interface.
5. Inspired by D-Bus: named interfaces, typed methods, typed events,
   discoverable at runtime.
6. **One communication primitive.** Methods are request/response. Everything
   asynchronous — state changes, shell-wide notifications — is a typed event
   on a named channel. There is no second messaging mechanism.

## Concepts

### Interface

A named, versioned contract describing a set of methods, event channels, and
required capabilities. An interface is **not** code — it is a declaration.

```
interface: mesh.audio
version:   1.0
methods:
  default_output() -> Device?
  output_devices() -> [Device]
  set_volume(device_id: string, level: float) -> Result
  set_mute(device_id: string, muted: bool)    -> Result
events:
  DeviceChanged(device: Device)
  VolumeChanged(device_id: string, level: float)
types:
  Device { id: string, name: string, volume: float, muted: bool }
```

Events declared inside an interface are **channels owned by that interface**
— only the currently-active implementation may publish to them, and their
payload schema is part of the contract. Channels declared *outside* any
interface (e.g. `shell.toggle-quick-settings`, `theme.changed`) are unowned:
any plugin with the right capability can publish. Both are the same
primitive; ownership is the only thing that differs.

Interfaces live in **contract packages** — distributable units with
`type = "interface"` that contain only the declaration file. Both the backend
and the frontend declare a dependency on the contract.

> **On the schema grammar.** The TOML snippets below are illustrative
> pseudo-schema. The authoritative grammar for interface declarations is
> tracked separately (candidate formats: a restricted TOML dialect, JSON
> Schema + extensions, or a small custom IDL). Tooling and the `mesh-interface`
> proc-macro will consume the final format; contract authors should not treat
> these snippets as stable syntax.

### Implementation

A backend registers an implementation of an interface with the core's
**interface registry**:

```
registry.register(
    interface = "mesh.audio",
    version   = "1.0",
    provider  = "@mesh/pipewire-audio",
    priority  = 100,
    handle    = <plugin-provided dispatcher>,
)
```

Multiple implementations of the same interface may coexist. The active one is
chosen by:

1. User pin in `~/.config/mesh/config.toml` under `[services]`.
2. Otherwise the highest `priority` among implementations that report
   compatibility (e.g. "PipeWire daemon is reachable").
3. Fallback to the next candidate if init fails.

### Consumption

Frontends look up interfaces by name and version range:

```luau
local audio = mesh.interfaces.get("mesh.audio", ">=1.0, <2.0")
if audio then
    local dev = audio:default_output()
    audio:on("VolumeChanged", function(sig) ... end)
end
```

The returned handle is a proxy object. The core validates each call against
the declared interface: argument types, event payloads, capability grants.
Event subscriptions opened through the proxy are scoped to the active
implementation — if the backend is hot-swapped, the proxy stays valid and the
subscription resumes against the new implementation.

## Declaring a new interface

Anyone can ship a contract package. No core change required.

```toml
# @alice/thermal-contract / mesh.toml
[package]
id         = "@alice/thermal-contract"
version    = "1.0.0"
type       = "interface"
api_version = "0.1"

[interface]
name    = "alice.thermal"
version = "1.0"
file    = "interface.toml"
```

```toml
# interface.toml
[[methods]]
name     = "sensors"
returns  = "[Sensor]"

[[methods]]
name     = "read"
args     = [{ name = "sensor_id", type = "string" }]
returns  = "float"

[[events]]
name     = "TemperatureChanged"
payload  = "{ sensor_id: string, celsius: float }"

[[types.Sensor]]
fields = [
    { name = "id",    type = "string" },
    { name = "name",  type = "string" },
    { name = "units", type = "string" },
]

[capabilities]
required = ["service.thermal.read"]
```

## Implementing and consuming it

**Backend** (`@alice/lmsensors`):

```toml
[package]
type = "backend"

[service]
provides = "alice.thermal"   # matches the interface name
priority = 100

[dependencies]
"@alice/thermal-contract" = ">=1.0.0, <2.0.0"
```

**Frontend** (`@alice/thermal-widget`):

```toml
[dependencies]
"@alice/thermal-contract" = ">=1.0.0, <2.0.0"
```

```luau
local thermal = mesh.interfaces.get("alice.thermal", ">=1.0")
for _, s in ipairs(thermal:sensors()) do
    print(s.name, thermal:read(s.id))
end
```

Neither the backend nor the frontend needs core changes, patches, or review.
They only need the contract package in their dependency graph.

## Interface versioning

Interfaces follow semver:

- **Major** — breaking change. New interface name recommended (`mesh.audio.v2`
  alongside `mesh.audio`) so old consumers keep working.
- **Minor** — additive. New methods / event channels may be added; consumers
  targeting an older minor continue to work.
- **Patch** — documentation / clarification only.

The registry supports multiple versions of the same interface simultaneously.
Consumers request a range; the core routes to a compatible implementation or
returns `nil`.

### Multi-version coexistence

A backend may advertise several versions at once, which is the recommended
path during major-version migrations:

```toml
[[service]]
provides = "mesh.audio"
version  = "1.3"
priority = 100

[[service]]
provides = "mesh.audio"
version  = "2.0"
priority = 100
```

The registry indexes each `(interface, version)` pair independently. Old
consumers requesting `>=1.0, <2.0` continue to resolve to the v1 dispatcher
while new consumers requesting `>=2.0` resolve to the v2 dispatcher — both
served by the same process.

## Event channels

All asynchronous messaging flows through a single mechanism: **typed event
channels**. A channel is a `(name, payload schema)` pair. Any plugin can
subscribe to any channel it has capability for; publication is gated by
ownership.

### Ownership

- **Owned channels** — declared inside an interface contract. Only the
  currently-active implementation of that interface may publish. The channel
  name is the interface-qualified event name (e.g.
  `mesh.audio/VolumeChanged`). Payload is validated against the contract.
- **Unowned (shell) channels** — declared by a regular plugin outside any
  interface. Any plugin holding the right publish capability may emit. Use
  these for cross-cutting shell events like `shell.toggle-quick-settings` or
  `theme.changed`. Payload schema is declared on the channel itself.

Both kinds resolve through the same `mesh.events` API:

```luau
-- subscribe — works the same for owned and unowned channels
mesh.events.on("mesh.audio/VolumeChanged", function(p) ... end)
mesh.events.on("shell.toggle-quick-settings", function() ... end)

-- publish — only allowed on channels you own
mesh.events.emit("shell.toggle-quick-settings", {})
```

Interface proxies offer a convenience wrapper (`audio:on("VolumeChanged", …)`)
that resolves to the qualified channel name, but there is no separate signal
machinery underneath. It's the same bus.

### When to declare a channel inside an interface

- The event only makes sense when that interface exists, and
- the producer is whoever is implementing the interface at that moment.

Otherwise, declare a standalone channel. The distinction is about ownership,
not mechanics.

## Capability classification

Capability names are opaque strings, but the core still has to assign each
one a privilege tier (standard / elevated / high) so the installer can show
the right confirmation UI. Contract packages carry that classification
alongside the interface declaration:

```toml
[[capabilities]]
name  = "service.thermal.read"
level = "standard"
description = "Read thermal sensor values."

[[capabilities]]
name  = "service.thermal.control"
level = "elevated"
description = "Change fan curves and thermal limits."
```

The core refuses to load a contract that introduces a capability without a
declared level. Third-party tiers beyond standard/elevated/high are not
supported — the three tiers are part of the install UX.

## Defaults are plugins

All default behavior ships as ordinary plugins in the `@mesh` scope:

| Interface | Default implementations |
|-----------|-------------------------|
| `mesh.audio` | `@mesh/pipewire-audio` (priority 100), `@mesh/pulseaudio-audio` (50) |
| `mesh.network` | `@mesh/networkmanager` (100) |
| `mesh.power` | `@mesh/upower` (100) |
| `mesh.media` | `@mesh/mpris-media` (100) |

They hold no privileged status. A user who prefers `iwd` for network or a
custom `pw-cli`-based audio daemon just installs a plugin with higher
priority, or pins it in config.

Frontends likewise (`@mesh/panel`, `@mesh/launcher`, …) are ordinary surface
plugins. You can disable them and ship your own.

## Cross-language story

Because interfaces are declared data, the core generates per-language bindings
at load:

- **Luau** — proxy tables with type-checked method calls and coroutine-based
  event subscription.
- **WASM** — generated host-function imports matching the interface ABI.
- **Rust (Tier 1)** — a proc macro in `mesh-interface` reads the contract
  declaration at build time and emits a trait + proxy struct for that version.
  Backends implement the trait; frontends use the proxy. Both sides must be
  built against the same contract *package version*; because Rust has no
  stable ABI across compiler versions, they must also be built with a
  compatible toolchain (the MESH release pins the Rust toolchain for Tier 1
  plugins). Luau and WASM plugins have no such constraint — they go through
  the registry's dynamic dispatch and interoperate across languages freely.

This keeps strongly-typed ergonomics where it matters (Rust) without forcing
every new service category through a core release.

## Capability enforcement

The contract lists the capabilities required to *implement* and to *consume*
the interface. The core checks both at registration and at lookup time. A
frontend that lacks `service.thermal.read` cannot acquire the proxy, and a
backend that lacks it cannot register an implementation.

New capability names can be introduced by contract packages — the core
treats capability identifiers as opaque strings, the same way D-Bus treats
interface names.

## Inspection & tooling

The `mesh` CLI mirrors the registry's introspection surface:

```
mesh interfaces                         # list all registered interfaces
mesh interfaces describe alice.thermal  # print methods, events, types
mesh interfaces providers alice.thermal # show implementations + priorities
mesh interfaces call alice.thermal sensors
```

The shell also exposes this through its diagnostics panel so users can see
exactly which plugin is currently backing each interface and switch between
candidates.

## Relationship to `spec/pluggable-backend.md`

`spec/pluggable-backend.md` describes the plugin lifecycle, manifest format,
capability system, and security model — all of which still apply. This
document refines its "Backend / frontend separation" section: service traits
are no longer a fixed list baked into `mesh-service`, they are
user-extensible interface contracts distributed as normal packages.

### The `mesh-service` / `mesh-interface` crates

The `mesh-service` crate in the workspace **no longer owns a list of
compiled-in traits**. Its post-migration responsibilities are:

- The interface registry data structures and lookup API.
- Default dispatch / proxy runtime (used by Luau and WASM bindings).
- Re-exports consumed by the `mesh-interface` proc macro.

`mesh-interface` (new) hosts the build-time machinery that turns an interface
declaration into Rust types. Contracts themselves live in their own packages
(e.g. `@mesh/audio-contract`), not in a central crate.

## See also

- [`theming/icons.md`](./theming/icons.md) — icon packs use the same
  contract-and-registry model, with one deliberate divergence: multiple
  `mesh.icons` providers stay active at once as an ordered fallback chain.
