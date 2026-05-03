---
phase: 04-real-core-surfaces
reviewed: 2026-05-03T06:55:39Z
depth: standard
files_reviewed: 11
files_reviewed_list:
  - crates/core/runtime/backend/src/lib.rs
  - crates/core/runtime/scripting/src/context.rs
  - crates/core/shell/src/shell/component.rs
  - docs/plugins/frontend/core/README.md
  - packages/plugins/backend/core/audio-interface/interface.toml
  - packages/plugins/backend/core/pipewire-audio/src/main.luau
  - packages/plugins/backend/core/pulseaudio-audio/src/main.luau
  - packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh
  - packages/plugins/frontend/core/quick-settings/src/components/wifi-item.mesh
  - packages/plugins/frontend/core/quick-settings/src/components/wifi-section.mesh
  - packages/plugins/frontend/core/quick-settings/src/main.mesh
findings:
  critical: 6
  warning: 1
  info: 0
  total: 7
status: issues_found
---

# Phase 04: Code Review Report

**Reviewed:** 2026-05-03T06:55:39Z
**Depth:** standard
**Files Reviewed:** 11
**Status:** issues_found

## Summary

The reviewed implementation has correctness and security issues in the new service-backed core surfaces. The highest-risk problems are shell command injection in audio playback, read-only frontend capability bypass for service mutations, broken shell event routing, and a reactive equality shortcut that prevents same-length object lists from updating.

## Critical Issues

### CR-01: BLOCKER - Audio sound playback concatenates untrusted paths into a shell command

**File:** `packages/plugins/backend/core/pipewire-audio/src/main.luau:126` and `packages/plugins/backend/core/pulseaudio-audio/src/main.luau:77`

**Issue:** `on_command_play_sound()` builds `aplay ` by concatenating `payload.path` and passes it to `mesh.exec_shell`, which runs through `sh -lc`. Shell sound paths come from shell configuration before being sent as a backend command, so a value like `/tmp/sound.wav; touch /tmp/pwned` executes arbitrary shell commands.

**Fix:**
```luau
function on_command_play_sound()
    local payload = mesh.service.payload()
    local path = payload.path
    if type(path) ~= "string" or path == "" then
        return
    end
    mesh.exec("aplay", { path })
end
```

### CR-02: BLOCKER - Read-only service access exposes mutating proxy methods

**File:** `crates/core/runtime/scripting/src/context.rs:607`

**Issue:** `require("@mesh/audio")` allows access when the plugin has either read or control capability for the interface, but `create_service_proxy()` exposes every contract method unconditionally at lines 883-907. A frontend with only `service.audio.read` can still call `audio.set_volume()` or `audio.toggle_mute()`, which publishes a `ServiceCommand` and is dispatched without another authorization check.

**Fix:** Carry the caller capability set into proxy creation and expose command methods only when `service.<name>.control` is granted. Read-only callers should still get state-field reads.

```rust
let can_control = capabilities.is_granted(&Capability::new(format!(
    "service.{}.control",
    service_name
)));

if methods.iter().any(|m| m.name == key) && !can_control {
    return Err(mlua::Error::external(ScriptError::CapabilityDenied(format!(
        "{interface_name}.{key}"
    ))));
}
```

Also enforce the same capability gate before dispatching `CoreRequest::ServiceCommand`, so forged `mesh.events.publish("mesh.audio.set_volume", ...)` events cannot bypass the proxy.

### CR-03: BLOCKER - `set_muted` is in the audio contract but neither shipped provider implements it

**File:** `packages/plugins/backend/core/audio-interface/interface.toml:40`

**Issue:** The public audio interface declares a callable `set_muted(device_id, muted)` method, but both reviewed audio providers only implement `on_command_toggle_mute()`. Calls to `audio.set_muted(...)` are accepted by the proxy and routed to the backend, then silently ignored because `BackendScriptContext::run_command()` cannot find `on_command_set_muted`.

**Fix:**
```luau
function on_command_set_muted()
    local payload = mesh.service.payload()
    local muted = payload.muted == true
    -- PipeWire: resolve sink first, then:
    mesh.exec_shell(("wpctl set-mute %s %s"):format(sink_id, muted and "1" or "0"))
    emit_state()
end
```

Add the equivalent PulseAudio handler:

```luau
function on_command_set_muted()
    local payload = mesh.service.payload()
    mesh.exec_shell(("pactl set-sink-mute @DEFAULT_SINK@ %s"):format(payload.muted == true and "1" or "0"))
    emit_state()
end
```

### CR-04: BLOCKER - Quick Settings publishes unsupported shell event names, so close/toggle actions do not route

**File:** `packages/plugins/frontend/core/quick-settings/src/main.mesh:161`

**Issue:** The close handler publishes `shell.close-quick-settings`, and the docs teach `shell.toggle-quick-settings` / `shell.close-quick-settings` at `docs/plugins/frontend/core/README.md:172`. The shell routing code only recognizes generic events like `shell.toggle-surface` and `shell.hide-surface`; unknown dotted channels fall through as service commands. The close button therefore emits a command for interface `shell` instead of hiding the Quick Settings surface.

**Fix:**
```luau
function onClose()
    mesh.events.publish("shell.hide-surface", { surface_id = "@mesh/quick-settings" })
end
```

Update the docs examples to use `shell.toggle-surface` / `shell.hide-surface` with `surface_id = "@mesh/quick-settings"`, or add explicit router support for the named events.

### CR-05: BLOCKER - Same-length lists of objects do not update in reactive state

**File:** `crates/core/runtime/scripting/src/context.rs:126`

**Issue:** `reactive_values_equal()` treats any nested object or array entries as equal without comparing their contents. `wifi-section.mesh` assigns `wifi_networks = network.networks` at line 58; when the backend emits the same number of networks with changed names, strengths, or active flags, `ScriptState::set()` decides the new array is equal and leaves the old UI state in place. This also affects top-level service payloads containing same-length arrays.

**Fix:**
```rust
fn reactive_values_equal(previous: &Value, next: &Value) -> bool {
    previous == next
}
```

If a shallow optimization is still needed, only skip work for values that are actually equal; do not collapse all nested objects and arrays to "equal".

### CR-06: BLOCKER - Wi-Fi connect sends `connection_id`, but the current backend command reads `id`

**File:** `packages/plugins/frontend/core/quick-settings/src/components/wifi-item.mesh:38`

**Issue:** `network.connect(network_id)` is encoded by the service proxy using the interface argument name, so the command payload is `{ "connection_id": "<id>" }`. The NetworkManager backend command handler currently reads `payload.id`, so Quick Settings connect requests arrive without the ID the backend expects and fail to connect.

**Fix:** Align the contract and provider payload names. The least disruptive backend fix is:

```luau
local payload = mesh.service.payload()
local id = payload.connection_id or payload.id or ""
```

Alternatively, change the network interface argument to `id` and update all callers/tests to match.

## Warnings

### WR-01: WARNING - Active navigation classes are computed but never bound to the nav buttons

**File:** `packages/plugins/frontend/core/quick-settings/src/main.mesh:108`

**Issue:** `sync_nav_classes()` updates `wifi_nav_class`, `bt_nav_class`, and `audio_nav_class`, but the template hardcodes every nav button as `class="nav-btn"` at lines 14, 18, and 22. The `.nav-active` style is therefore dead and the selected section has no active visual state.

**Fix:** Bind the computed class variables in the template and initialize them before first render:

```xml
<button class="{wifi_nav_class}" onclick={onSelectWifi} title="Wi-Fi settings" aria-label="Wi-Fi settings">
```

```luau
wifi_nav_class = "nav-btn nav-active"
bt_nav_class = "nav-btn"
audio_nav_class = "nav-btn"
```

---

_Reviewed: 2026-05-03T06:55:39Z_
_Reviewer: the agent (gsd-code-reviewer)_
_Depth: standard_
