# Pluggable Shell Backend

This document specifies the pluggable backend architecture for MESH. The backend is the core runtime that hosts plugins and provides them with the services they need to build shell experiences.

The backend does not define what the shell looks like or how it behaves. That is the job of plugins. The backend provides lifecycle management, security enforcement, rendering coordination, Wayland surface management, inter-plugin communication, and system service access.

## Design principles

1. The shell is defined by its plugins. The backend is infrastructure.
2. Plugins are isolated by default and granted capabilities explicitly.
3. The default experience must be complete without third-party plugins.
4. Plugin APIs are versioned independently from the shell release.
5. Compositor differences are abstracted behind capability queries.
6. Performance-critical paths stay in the core. Plugins describe intent; the core executes.

## Plugin model

### What a plugin is

A plugin is a distributable unit that extends the shell. Every plugin has:

- a manifest declaring identity, capabilities, dependencies, and metadata
- one or more entrypoints
- optional assets (icons, translations, styles, schemas)

### Plugin types

| Type | Purpose | Example |
|------|---------|---------|
| `backend` | Service implementation for a specific system | PipeWire audio, NetworkManager network |
| `surface` | Top-level shell UI (frontend) | Panel, launcher, notification center |
| `widget` | Embeddable UI inside a surface | Clock, battery indicator, weather card |
| `service` | State provider or action handler | Battery, network, media, notifications |
| `theme` | Visual token set | Dark theme, accent color pack |
| `language-pack` | Translations for core or other plugins | French language pack |
| `icon-pack` | Icon set | Custom symbolic icon theme |

### Plugin identity

Every plugin has a namespaced identifier:

```
@scope/name
```

`scope` is the author or organization. `name` is the plugin name. Together they form a globally unique ID within the MESH ecosystem.

Examples:

```
@mesh/panel
@mesh/launcher
@community/weather-widget
@alice/custom-theme
```

Core plugins shipped with MESH use the `@mesh` scope.

## Plugin manifest

Every plugin must include a `mesh.toml` manifest at its package root.

```toml
[package]
id = "@scope/name"
version = "1.2.0"
type = "widget"
api_version = "0.1"
license = "MIT"
description = "A short description of what this plugin does."
authors = ["Alice <alice@example.com>"]
repository = "https://github.com/alice/mesh-weather"

[compatibility]
mesh = ">=0.1.0, <1.0.0"
compositors = ["wlr-layer-shell-v1"]  # optional, defaults to any

[dependencies]
"@mesh/theme" = ">=0.1.0"
"@mesh/locale" = ">=0.1.0"
"@mesh/network-service" = { version = ">=0.1.0", optional = true }

[capabilities]
required = [
    "shell.widget",
    "theme.read",
    "locale.read",
]
optional = [
    "service.network.read",
    "service.location.read",
]

[entrypoints]
main = "src/main.luau"
settings_ui = "src/settings.luau"  # optional

[settings]
schema = "settings.schema.toml"

[i18n]
default_locale = "en"
translations = "i18n/"

[theme]
tokens_used = ["color.surface", "color.on-surface", "color.primary", "spacing.md", "radius.md"]

[assets]
icons = "assets/icons/"
```

### Manifest rules

- `api_version` declares which version of the plugin API this plugin targets. The core loads the appropriate compatibility shim if needed.
- `compatibility.compositors` lists Wayland protocol extensions the plugin requires. If the current compositor does not support them, the plugin will not load. If omitted, the plugin is assumed compositor-agnostic.
- `capabilities.required` must all be granted or the plugin will not load.
- `capabilities.optional` may be denied. The plugin must handle their absence.
- `dependencies` are resolved before the plugin loads. Circular dependencies are rejected.

## Plugin lifecycle

A plugin moves through these states:

```
Discovered -> Resolved -> Loaded -> Initialized -> Running -> Suspended -> Unloaded
                                                                    |
                                                                    v
                                                                 Errored
```

### Discovered

The core scans plugin directories and reads manifests. No code is executed. Invalid manifests are logged and skipped.

### Resolved

The core resolves the dependency graph across all discovered plugins. Plugins with unsatisfiable dependencies are rejected with a diagnostic message. Circular dependencies are rejected.

Resolution order:

1. Core plugins (always loaded first)
2. Service plugins (provide APIs others depend on)
3. Surface plugins
4. Widget plugins
5. Theme, language, and icon packs (loaded in parallel, no ordering requirement)

### Loaded

The plugin's code and assets are loaded into memory. For Luau plugins, the script is parsed and type-checked. For compiled plugins, the shared library or WASM module is loaded.

No plugin code is executed yet.

### Initialized

The core calls the plugin's `init` entrypoint. The plugin receives:

- its scoped configuration
- handles to its granted capabilities
- a reference to the plugin context (logging, diagnostics, state store)

The plugin performs setup: registering event handlers, subscribing to services, declaring its UI tree. If `init` fails, the plugin moves to `Errored`.

### Running

The plugin is active. It receives events, updates state, and submits UI updates through the core's rendering pipeline.

### Suspended

The plugin is temporarily inactive. This happens when:

- the shell surface containing the plugin is not visible
- the user explicitly disables the plugin
- the system enters a low-power state

Suspended plugins receive no events and submit no UI. Their state is preserved in memory. Resuming returns them to `Running`.

### Unloaded

The plugin is removed from memory. This happens on:

- shell shutdown
- plugin uninstall
- plugin update (unload old, load new)

The core calls `shutdown` before unloading. The plugin should persist any state it needs through the scoped state store.

### Errored

The plugin encountered an unrecoverable fault. The core:

1. Logs the error with full context
2. Removes the plugin's UI from the render tree
3. Notifies dependent plugins
4. Shows a placeholder in the UI if the plugin was visible (not a blank gap)
5. Optionally offers the user a "restart plugin" action

A plugin that errors repeatedly (3 times within 60 seconds) is disabled until the next shell restart or manual re-enable.

## Plugin execution model

### Execution tiers

Plugins run in one of three execution tiers. The tier determines isolation level, performance characteristics, and trust requirements.

#### Tier 1 — In-process (Rust)

- Compiled Rust code loaded as a dynamic library
- Full performance, no serialization overhead
- Can crash the shell if it panics (mitigated by catch_unwind at boundaries)
- Reserved for core plugins and explicitly trusted packages
- Must pass review and signing by the MESH project

#### Tier 2 — Sandboxed interpreter (Luau)

- Luau scripts running in the embedded interpreter
- Sandboxed: no filesystem, no network, no process spawning by default
- Access to system resources only through granted capability handles
- This is the default and recommended tier for community plugins
- Hot-reloadable during development

#### Tier 3 — Sandboxed compiled (WASM)

- WebAssembly modules running in a WASM runtime (e.g. wasmtime)
- Near-native performance with memory isolation
- Communicates with the core through a defined ABI
- Suitable for performance-sensitive community plugins
- More complex to build than Luau plugins

### Frame budgets

Every plugin that produces UI is assigned a frame budget. The budget is the maximum time the plugin may spend computing its UI update per frame.

- Default budget: 4ms per plugin per frame (targeting 60fps, ~16ms total frame time)
- If a plugin exceeds its budget, the core skips that plugin's update and reuses the previous frame
- Repeated budget overruns are logged and surfaced to the user
- The user can adjust budgets per plugin in shell settings

### Thread model

- The core runs on the main thread and owns the Wayland event loop
- Tier 1 plugins run on the main thread (must not block)
- Tier 2 and 3 plugins run on dedicated worker threads, one per plugin
- UI updates from worker threads are submitted to the core via a lock-free channel
- The core applies UI updates on the main thread during the next frame

## Capability system

### How capabilities work

A capability is a named permission that grants access to a specific host API or system resource. Plugins declare required and optional capabilities in their manifest. The core grants or denies them at load time.

A capability is represented as a handle. Plugins cannot forge handles. The core mints handles during initialization and passes them to the plugin. If a plugin does not have a handle, it cannot call the associated API.

### Capability categories

#### Shell capabilities

| Capability | Grants |
|---|---|
| `shell.surface` | Create and manage a top-level shell surface |
| `shell.widget` | Register as an embeddable widget |
| `shell.overlay` | Display overlay or popup UI |
| `shell.notification` | Post notifications to the notification system |
| `shell.clipboard.read` | Read clipboard contents |
| `shell.clipboard.write` | Write to clipboard |
| `shell.input.keyboard` | Receive keyboard input events |
| `shell.input.pointer` | Receive pointer input events |
| `shell.screenshot` | Capture screen content (high privilege) |

#### Service capabilities

| Capability | Grants |
|---|---|
| `service.battery.read` | Read battery state |
| `service.network.read` | Read network state |
| `service.network.control` | Modify network connections |
| `service.media.read` | Read media playback state |
| `service.media.control` | Control media playback |
| `service.audio.read` | Read audio device/volume state |
| `service.audio.control` | Change volume, mute, output device |
| `service.bluetooth.read` | Read bluetooth state |
| `service.bluetooth.control` | Pair, connect, disconnect bluetooth devices |
| `service.notifications.read` | Read notification history |
| `service.notifications.post` | Post new notifications |
| `service.notifications.manage` | Dismiss or modify existing notifications |
| `service.location.read` | Read device location |
| `service.power.read` | Read power profile state |
| `service.power.control` | Change power profile |

#### Theme and locale capabilities

| Capability | Grants |
|---|---|
| `theme.read` | Read current theme tokens |
| `theme.write` | Modify the active theme (high privilege) |
| `locale.read` | Read current locale and translations |
| `locale.write` | Modify active locale (high privilege) |

#### System capabilities

| Capability | Grants |
|---|---|
| `exec.launch-app` | Launch applications via desktop entries |
| `exec.command` | Execute arbitrary commands (high privilege) |
| `fs.read` | Read files from a scoped directory |
| `fs.write` | Write files to a scoped directory |
| `net.http` | Make outbound HTTP requests |
| `net.socket` | Open network sockets (high privilege) |
| `dbus.session` | Access the D-Bus session bus |
| `dbus.system` | Access the D-Bus system bus (high privilege) |

### Privilege levels

Capabilities are grouped into three privilege levels:

- **Standard** — safe for most plugins. Read-only access to services, theme, locale. Examples: `theme.read`, `service.battery.read`, `locale.read`.
- **Elevated** — grants meaningful system interaction. Write access, notifications, launching apps. Examples: `service.network.control`, `exec.launch-app`, `net.http`. Requires user confirmation at install.
- **High** — grants powerful or sensitive access. Screenshots, arbitrary commands, D-Bus system bus, raw sockets. Examples: `exec.command`, `shell.screenshot`, `dbus.system`. Requires explicit user opt-in with a warning.

### Capability enforcement

- Luau plugins: the interpreter only exposes API functions for granted capabilities. Ungrantable APIs do not exist in the plugin's environment.
- WASM plugins: host function imports are selectively linked based on granted capabilities.
- Rust plugins: capability handles are passed at init. Calling an API without a handle is a compile-time error (the API requires the handle as a parameter).

## Inter-plugin communication

### Event bus

The core provides a typed event bus. Plugins can:

- **Publish** events to named channels
- **Subscribe** to named channels

Events are typed. Each channel has a declared payload schema. The core validates payloads before delivery.

```
Channel: "service.battery.changed"
Payload: { level: number, charging: boolean, time_to_empty: number? }
```

Plugins can only publish to channels they own (matching their package scope) or to shared channels they have capability for. Any plugin can subscribe to any public channel.

### Service registry

Service plugins register themselves with the core under a typed interface. Consumer plugins look up services by interface ID, not by plugin ID. This allows swapping implementations.

```
Interface: "mesh.service.network"
Provider: "@mesh/networkmanager-service" (or "@community/iwd-service", etc.)
```

If multiple plugins provide the same interface, the user chooses which one is active. Only one provider per interface is active at a time.

### Direct messaging

Plugins can send direct messages to other plugins by ID if both plugins declare a messaging contract. This is for tightly coupled plugin pairs (e.g., a widget and its companion service).

Direct messaging requires the `ipc.direct` capability.

## Backend / frontend separation

### Architecture

MESH enforces a strict separation between service backends and UI frontends. They never reference each other directly. The service registry is the only bridge.

```
┌─────────────────────────────────────────────────┐
│              Service Traits (mesh-service)       │
│      AudioService, NetworkService, PowerService  │
│      MediaService, BrightnessService, ...        │
└──────────┬──────────────────────────┬────────────┘
           │                          │
  ┌────────▼─────────┐     ┌─────────▼────────┐
  │  Backend Plugins  │     │  Backend Plugins  │
  │  (implementations)│     │  (implementations)│
  │                   │     │                   │
  │  @mesh/pipewire   │     │  @mesh/pulseaudio │
  │  @mesh/upower     │     │  @community/iwd   │
  └────────┬─────────┘     └─────────┬────────┘
           │                          │
           └────────────┬─────────────┘
                        │
           ┌────────────▼────────────┐
           │    Service Registry     │
           │  (one active per trait) │
           └────────────┬────────────┘
                        │
           ┌────────────▼────────────┐
           │   Frontend Plugins      │
           │  (surfaces + widgets)   │
           │                         │
           │  Uses trait API only.   │
           │  Never imports backend. │
           └─────────────────────────┘
```

### Service traits

Each system service is defined as a Rust trait in the `mesh-service` crate:

- `AudioService` — output/input devices, streams, volume, mute
- `NetworkService` — connections, devices, wifi scan, connect/disconnect
- `NotificationService` — list, close, actions
- `PowerService` — battery, power profiles
- `MediaService` — players, playback control
- `BrightnessService` — get/set brightness

Traits use async methods and return `Result` types. Each trait also defines an event enum so backends can push changes to subscribers.

### Backend plugins

A backend plugin has `type = "backend"` in its manifest and includes a `[service]` section:

```toml
[package]
id = "@mesh/pipewire-audio"
type = "backend"

[service]
provides = "audio"
backend_name = "PipeWire"
priority = 100
```

At load time, the backend registers its trait implementation with the service registry:

```
registry.register::<dyn AudioService>("audio", "pipewire", "@mesh/pipewire-audio", backend)
```

Multiple backends can provide the same service. The one with the highest priority is selected by default. The user can override this in config:

```toml
# ~/.config/mesh/config.toml
[services]
audio = "@mesh/pulseaudio-audio"    # override: use PulseAudio instead of PipeWire
network = "@mesh/networkmanager"    # explicit selection
```

### Frontend plugins

Frontend plugins (surfaces and widgets) look up services by trait, never by backend:

```lua
-- A volume widget does not know or care if PipeWire or PulseAudio is running
local audio = mesh.services.get("audio")
local devices = audio.output_devices()
local default = audio.default_output()

audio.subscribe(function(event)
    if event.type == "DeviceChanged" then
        mesh.ui.request_redraw()
    end
end)
```

If no backend is registered for a service, the frontend receives `nil` and should show a graceful fallback (e.g. "No audio service available").

### Auto-detection

When no explicit backend is configured, the core:

1. Discovers all backend plugins for each service type
2. Checks system compatibility (e.g. is PipeWire running? is NetworkManager on D-Bus?)
3. Selects the highest-priority compatible backend
4. Falls back to the next candidate if the first fails to initialize

This means MESH works out of the box on most systems without manual backend configuration.

### Swapping backends at runtime

The service registry supports hot-swapping backends:

1. The new backend initializes and registers with the registry
2. The registry replaces the old backend's trait object
3. Frontends that hold subscriptions receive a `BackendChanged` event
4. Frontends re-query state from the new backend

This enables switching audio systems, network managers, or power daemons without restarting the shell.

## Compositor abstraction

### Problem

Wayland compositors implement different protocol extensions. MESH must work on multiple compositors without requiring every plugin to handle compositor differences.

### Abstraction layer

The core provides a `CompositorCapabilities` interface that reports what the current compositor supports:

```
compositor.supports("wlr-layer-shell-v1") -> bool
compositor.supports("ext-session-lock-v1") -> bool
compositor.supports("cosmic-workspace-v1") -> bool
compositor.name() -> string  // "sway", "hyprland", "cosmic-comp", etc.
compositor.version() -> string
```

Plugins declare compositor requirements in their manifest. The core checks compatibility at load time.

### Surface abstraction

The core provides a `ShellSurface` trait that abstracts over compositor-specific surface protocols:

```
ShellSurface
  .anchor(edge)          // position on screen edge
  .set_size(w, h)
  .set_exclusive_zone(z) // reserve screen space
  .set_layer(layer)      // background, bottom, top, overlay
  .set_keyboard_interactivity(mode)
  .show()
  .hide()
```

This maps to `wlr-layer-shell` where available and degrades to `xdg-toplevel` with best-effort positioning elsewhere.

### Capability-based degradation

If a compositor does not support a required protocol:

- Plugins that listed it in `compatibility.compositors` do not load. The user sees a message explaining why.
- Plugins that did not list it load normally but may find some `CompositorCapabilities` queries return false. They adapt at runtime.

## Plugin API surface

### Core context

Every plugin receives a `PluginContext` at init:

```
PluginContext
  .id          -> string         // this plugin's ID
  .version     -> string         // this plugin's version
  .log         -> Logger         // scoped logger
  .config      -> Config         // this plugin's settings (read-only unless it owns them)
  .state       -> StateStore     // scoped persistent key-value store
  .events      -> EventBus       // publish/subscribe
  .services    -> ServiceRegistry // look up service interfaces
  .diagnostics -> Diagnostics    // report health, metrics
```

### UI API

Plugins that produce UI use a declarative component API:

```
UI
  .create_tree(template) -> ComponentTree
  .update_tree(tree, patch)
  .request_redraw()
  .animate(property, from, to, duration, easing)
```

The core owns rendering. Plugins submit component trees. The core diffs, layouts, and paints.

### Theme API

```
Theme (requires theme.read)
  .token(name) -> value          // e.g. theme.token("color.primary")
  .tokens(group) -> map          // e.g. theme.tokens("color")
  .on_change(callback)           // called when theme changes
```

### Locale API

```
Locale (requires locale.read)
  .current() -> string           // e.g. "en-US"
  .translate(key) -> string      // look up translation
  .translate(key, args) -> string // look up with interpolation
  .format_number(n) -> string
  .format_date(d, pattern) -> string
  .on_change(callback)
```

### Settings API

```
Config
  .get(key) -> value
  .get_all() -> map
  .on_change(key, callback)     // called when user changes a setting
  .schema() -> Schema           // the plugin's declared schema
```

Plugins do not write their own config. The core writes config based on user input validated against the plugin's schema.

### State store API

```
StateStore
  .get(key) -> value?
  .set(key, value)
  .delete(key)
  .clear()
```

Scoped per plugin. Persisted to disk by the core. Plugins cannot access other plugins' state stores.

## Security model

### Threat model

| Threat | Vector | Mitigation |
|---|---|---|
| Malicious plugin code | Compromised or intentionally harmful plugin | Capability sandbox, execution tier isolation |
| Supply chain attack | Trusted plugin updated with malicious code | Package signing, update diffing, reproducible builds |
| UI spoofing | Plugin overlays fake auth dialog | Core owns trusted UI chrome (password prompts, capability dialogs). Plugins cannot draw over trusted surfaces |
| Data exfiltration | Plugin reads sensitive data and sends it out | `net.http` is a capability, not a default. Most plugins should never need it |
| Resource abuse | Plugin mines crypto or leaks memory | CPU/memory budgets per plugin. Core kills plugins exceeding limits |
| Keystroke interception | Plugin captures input meant for other contexts | `shell.input.keyboard` is an elevated capability. Input routing is managed by the core |
| Privilege escalation | Plugin escapes sandbox | Luau has no FFI. WASM is memory-isolated. Rust plugins are review-gated |
| Dependency confusion | Attacker publishes `@mesh/panel` on community registry | `@mesh` scope is reserved. Scopes are verified at publish time |

### Trust tiers

| Tier | Description | Isolation | Review |
|---|---|---|---|
| Core | Shipped with MESH | In-process | Full code review by MESH maintainers |
| Verified | Reviewed by MESH project | Luau/WASM sandbox | Code review, signed by MESH |
| Community | Published by anyone | Luau/WASM sandbox | No review. User accepts risk at install |
| Local | Developer's own, on their machine | Configurable | None |

### Package signing

- Core and verified plugins are signed with the MESH project key
- Community plugins are signed with the author's key
- The core verifies signatures at install and at load
- Unsigned plugins only load if the user explicitly enables unsigned loading in settings

### Update policy

- Plugin updates are fetched from the registry
- Before applying an update, the core shows a diff of changed capabilities
- If an update adds new elevated or high-privilege capabilities, the user must re-approve
- Users can pin plugins to specific versions
- Automatic updates are opt-in per plugin

## Plugin distribution

### Registry

MESH provides a central package registry. The registry stores:

- Package metadata (manifest, description, screenshots)
- Signed package archives
- Version history
- Download counts
- Compatibility reports from users

### Installation

```
mesh install @community/weather-widget
mesh install @community/weather-widget@1.2.0   # pinned version
mesh uninstall @community/weather-widget
mesh update @community/weather-widget
mesh list                                       # installed plugins
mesh search weather                             # search registry
```

### Local development

```
mesh dev ./my-plugin                            # load from local path with hot reload
mesh dev ./my-plugin --tier local               # full trust, no sandbox
mesh package ./my-plugin                        # build distributable archive
mesh publish ./my-plugin                        # publish to registry
```

### Plugin directories

```
~/.local/share/mesh/plugins/          # user-installed plugins
/usr/share/mesh/plugins/              # system-installed plugins (core, distro)
~/.local/share/mesh/dev-plugins/      # plugins loaded via `mesh dev`
```

Core plugins at the system path take precedence. User plugins override system plugins with the same ID.

## Configuration and settings

### Shell-level configuration

The shell itself is configured in `~/.config/mesh/config.toml`:

```toml
[shell]
default_surface = "@mesh/panel"

[plugins]
allow_unsigned = false
auto_update = false

[plugins."@mesh/panel"]
enabled = true
position = "top"
height = 32

[plugins."@community/weather-widget"]
enabled = true
location = "auto"
units = "metric"
```

### Settings schema validation

Plugin settings are validated against the plugin's declared schema. The core rejects invalid values and provides defaults for missing keys.

```toml
# settings.schema.toml for a weather widget
[location]
type = "string"
default = "auto"
description = "Location for weather data. Use 'auto' for automatic detection."

[units]
type = "enum"
values = ["metric", "imperial"]
default = "metric"
description = "Temperature and measurement units."

[refresh_interval]
type = "integer"
min = 60
max = 3600
default = 600
description = "How often to refresh weather data, in seconds."
```

### Generated settings UI

The core can generate a settings UI for any plugin based on its schema. Plugin authors do not need to build their own settings screens unless they want a custom layout (via the optional `settings_ui` entrypoint).

## Error handling and diagnostics

### Plugin errors

- Errors within a plugin do not crash the shell
- The core catches panics (Rust), runtime errors (Luau), and traps (WASM)
- Failed plugins show a placeholder in the UI with an error indicator
- The user can inspect errors through the shell diagnostics panel
- The error log is written to `~/.local/share/mesh/logs/`

### Health reporting

Plugins can report health status through the `Diagnostics` API:

```
diagnostics.healthy()
diagnostics.degraded("Network timeout, using cached data")
diagnostics.error("Failed to connect to weather API")
```

The core aggregates health status and surfaces it in a diagnostics panel.

### Performance monitoring

The core tracks per-plugin:

- Frame budget usage (average and peak)
- Memory usage
- Event processing latency
- Error count and frequency

This data is available through the diagnostics panel and the `mesh status` CLI command.

## Hot reload

During development, plugins loaded via `mesh dev` support hot reload:

- The core watches the plugin directory for file changes
- On change, the plugin is unloaded and reloaded
- Plugin state is preserved through the state store (state survives reload)
- UI state (scroll position, input focus) is best-effort preserved

Hot reload is not available for Tier 1 (Rust) plugins. They require a full rebuild and shell restart.

## Versioning and compatibility

### API versioning

The plugin API is versioned with semver. The current version is declared in the core and in each plugin's manifest.

- **Major** version bump: breaking changes to the plugin API. Old plugins will not load without updating.
- **Minor** version bump: new APIs added. Old plugins continue to work.
- **Patch** version bump: bug fixes only.

### Compatibility strategy

The core supports loading plugins targeting the current major version. When a new major version ships:

- Plugins targeting the previous major version get a 6-month deprecation window
- The core includes a compatibility shim for the previous major version during this period
- After deprecation, old plugins must update or they will not load

### Minimum viable API

The initial API version (`0.x`) is explicitly unstable. Breaking changes can happen between minor versions during the `0.x` phase. The project will aim to stabilize at `1.0` once the core plugin types (surface, widget, service, theme, locale) have proven stable in real use.
