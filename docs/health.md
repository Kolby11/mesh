# Plugin Health

Every plugin has a **health state** — a first-class runtime primitive the
core tracks and frontends subscribe to. Health is how a missing system
dependency, a daemon that stopped, or a degraded capability becomes
something the user sees as "Audio unavailable: install `playerctl`" rather
than a silent broken widget.

Health and installation share one source of truth: the **same dependency
declaration** in `plugin.json` drives the installer's pre-flight check
*and* the plugin's runtime self-check. There is no duplication between
"what the installer looks for" and "what the plugin verifies on start".

## States

A plugin reports one of three states at a time:

| State | Meaning |
|-------|---------|
| `healthy` | Running normally. All required deps present, all declared features available. |
| `degraded` | Running, but one or more optional features are unavailable. The plugin works; some capability is missing. |
| `unavailable` | Cannot run. A required dependency is missing, the daemon is down, or an unrecoverable error occurred. The plugin is loaded but does not register its interface implementation. |

State transitions emit events (see [Health channel](#health-channel)) so
subscribers can update UI in real time.

## Health record

Each state carries structured context so frontends can render something
useful:

```json
{
  "state":           "unavailable",
  "reason":          "playerctl not found on $PATH",
  "fix_suggestion":  "Install playerctl via your system package manager.",
  "missing": [
    {
      "kind":    "binary",
      "name":    "playerctl",
      "version": ">=2.0",
      "packages": {
        "debian": "playerctl",
        "arch":   "playerctl",
        "fedora": "playerctl"
      }
    }
  ],
  "since":           "2026-04-20T10:12:03Z",
  "recoverable":     true
}
```

Fields:

| Field | Type | Purpose |
|-------|------|---------|
| `state` | enum | `healthy` / `degraded` / `unavailable` |
| `reason` | string | One-line human-readable summary |
| `fix_suggestion` | string | What the user can do, in plain language |
| `missing` | array | Structured list of missing deps (for tooling and package-manager integration) |
| `degraded_features` | array | Names of optional capabilities that are not available (only on `degraded`) |
| `since` | ISO 8601 | When the plugin last entered this state |
| `recoverable` | bool | Whether periodic re-checks may succeed (e.g. user installs `playerctl` without restarting the shell) |

`reason` and `fix_suggestion` come from the plugin's dependency
declaration — the `reason` field on each `native_libs` / `binaries` /
`fonts` entry, plus the per-distro `packages` map. Plugin authors write
this once, in `plugin.json`; the installer and the runtime both use it.

## How health is set

1. **Install-time probe.** The installer runs the dep check, lands the
   plugin on disk, and writes the result into the plugin's initial health
   record. Missing hard deps → `unavailable`. Missing optional deps →
   `degraded`.
2. **Load-time probe.** On every shell start, the plugin re-runs the same
   checks. Results are fresh (the user may have fixed or broken things
   since install).
3. **Runtime reports.** The plugin can update its own state during
   execution through the `PluginContext.diagnostics` API:
   ```luau
   mesh.diagnostics.degraded("Per-app volume not supported on this sound server")
   mesh.diagnostics.unavailable("PipeWire daemon is not running")
   mesh.diagnostics.healthy()
   ```
4. **Periodic re-check.** The core re-runs probes for `recoverable`
   plugins at a low cadence (default every 30s for `unavailable`, 5m for
   `degraded`). A user who installs `playerctl` while the shell is
   running should see the media widget come alive without restarting.
5. **Dependency health propagation.** A plugin's reported state is
   **combined** with the health of anything it depends on. A frontend
   consuming `mesh.audio` from a backend that's `unavailable` sees the
   interface as `unavailable` — it does not need to code for both cases
   separately.

## Health channel

Health flows on the same typed-channel bus as everything else. The core
exposes:

- `plugin.health/<plugin-id>` — state changes for a specific plugin
- `interface.health/<interface-name>` — state changes for an interface
  (computed from the active provider's health)
- `plugin.health` — a fan-out channel that fires for every state change
  across the shell, used by the diagnostics panel

Subscription works like any other channel:

```luau
mesh.events.on("interface.health/mesh.audio", function(h)
    if h.state == "unavailable" then
        showBanner(h.reason, h.fix_suggestion)
    elseif h.state == "degraded" then
        showBadge(h.reason)
    else
        hideBanner()
    end
end)
```

## Frontend consumption

### Via the interface proxy

`mesh.interfaces.get(...)` returns the proxy and the current health:

```luau
local audio, status = mesh.interfaces.get("mesh.audio", ">=1.0")

if not audio then
    -- no provider at all
    showUnavailable(status and status.reason or "No audio backend installed.")
    return
end

audio:on_health(function(h) refreshUI(h) end)
```

`status` is always present — even when the lookup returns `nil`, it
carries the reason (no provider, provider is unavailable, capability
denied, version range unsatisfiable).

### Via `widget-fallback`

For the very common "show a default state when the backing service is
unavailable" pattern, the core provides a turnkey template:

```xml
<widget-fallback interface="mesh.audio" version=">=1.0">
  <template slot="healthy">
    <icon name="volume" :level="volumeLevel"/>
  </template>

  <template slot="degraded">
    <icon name="volume" :level="volumeLevel"/>
    <badge>{{degraded_features | join(', ')}} unavailable</badge>
  </template>

  <template slot="unavailable">
    <column class="fallback">
      <icon name="volume-off"/>
      <text class="muted">{reason}</text>
      <text class="hint">{fix_suggestion}</text>
    </column>
  </template>
</widget-fallback>
```

The core switches templates as the interface's health state changes. No
JavaScript-style conditional rendering, no polling — subscription is wired
by the renderer.

The templates are all optional: omit `degraded` to collapse it into
`healthy`; omit `unavailable` to get the core's default "Service
unavailable" card.

## Backend health and `unavailable` impl registration

When a backend's self-check returns `unavailable`, it **does not register
its interface implementation** with the registry. Concretely:

- The registry behaves as though the backend weren't installed at all for
  the purposes of picking an active provider.
- The next-priority provider takes over if one exists.
- The backend is still loaded (so its health record exists and its
  recoverable re-check can re-probe), but it is inert.

This keeps frontends honest: "no active provider" and "the active
provider is broken" are the same situation from their perspective — both
resolve via health data. There is no third case to code for.

## Backend-declared optional features

A backend reports which optional methods/events from the contract it
supports when it registers:

```
registry.register(
    interface = "mesh.audio",
    version   = "1.0",
    provider  = "@mesh/pulseaudio-audio",
    optional_capabilities = ["set_mute"],
    ...
)
```

Methods or events the backend does not advertise become:

- runtime: `audio.has("set_app_volume") == false`; calling the method
  raises `unsupported_operation`
- health: reported as `degraded` with `degraded_features: ["set_app_volume"]`
  until either the backend gains the capability or the user swaps to a
  backend that has it

Frontends built against the contract should *check capability before
calling optional methods*. Linting at package time flags direct calls to
contract-optional methods that aren't guarded.

## Diagnostics panel

The shell's built-in diagnostics panel is wired to the `plugin.health`
fan-out channel. It shows, at a glance:

- Every plugin and its current health
- Missing system deps across the whole installation, with the right
  package-manager commands
- Which backend is currently active for each interface
- History of state transitions (so "why did my volume widget disappear
  five minutes ago?" is answerable)

The panel is a plugin (`@mesh/diagnostics`). Like everything else, it
can be replaced — its contract is `mesh.diagnostics.ui`.

## CLI

```
mesh doctor                       # full health report for every plugin
mesh health <plugin-id>           # single plugin
mesh health --interface mesh.audio
mesh health --watch               # live stream of health events
```

`mesh doctor` output is designed to be the first thing a user runs when
something looks wrong:

```
$ mesh doctor

Shell       healthy
Theme       @mesh/default-theme (dark)        healthy
Locale      en                                 healthy

Interfaces
  mesh.audio    @mesh/pipewire-audio@0.1.0    healthy
  mesh.network  @mesh/networkmanager@0.1.0    healthy
  mesh.power    @mesh/upower@0.1.0             unavailable
                  Reason:  UPower daemon is not running.
                  Fix:     sudo systemctl start upower.service
  mesh.media    @mesh/mpris-media@0.1.0        degraded
                  Reason:  playerctl not found on $PATH.
                  Fix:     sudo apt install playerctl (Debian/Ubuntu)

Surfaces
  @mesh/panel               healthy
  @mesh/quick-settings      degraded — mesh.power unavailable
```

## Summary

- Health is a first-class runtime primitive with three states and a
  structured reason.
- The same `plugin.json` dep declaration drives both install-time probes
  and runtime self-checks. No duplicated reason strings.
- Unavailable backends don't register; the registry and frontends see a
  single "no active provider" story.
- Frontends subscribe to per-interface health through the normal event
  bus; `widget-fallback` handles the common template-switch case
  declaratively.
- `mesh doctor` is the one command a user runs when things look wrong.
