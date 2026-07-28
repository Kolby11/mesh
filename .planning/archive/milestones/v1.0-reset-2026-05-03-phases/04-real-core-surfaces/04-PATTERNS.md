# Phase 04 - Pattern Map

## Scope

Pattern map for Phase 4 plans. The phase modifies shipped `.mesh` frontend surfaces and narrowly scoped runtime/provider command handling.

## File Patterns

| Target | Closest Analog | Pattern To Preserve |
|--------|----------------|---------------------|
| `packages/plugins/frontend/core/panel/src/main.mesh` | Existing panel file | Use `pcall(require, "@mesh/<service>@>=1.0")`, derive compact labels in `onRender()`, keep panel as status/entry surface. |
| `packages/plugins/frontend/core/quick-settings/src/main.mesh` | Existing quick-settings root | Use left nav rail, one drawer section at a time, service state derived in `onRender()`, command handlers on buttons. |
| `packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh` | Current audio section + navigation-bar volume proof | Render source label, percent, slider/buttons; use `onchange` handler and proxy command methods. |
| `packages/plugins/frontend/core/quick-settings/src/components/wifi-section.mesh` | Current Wi-Fi section | Read `network.wifi_enabled` and provider-extra `network.networks`; show concise empty/disabled states. |
| `packages/plugins/frontend/core/quick-settings/src/components/wifi-item.mesh` | Existing Wi-Fi item | Keep row full-width, show name and strength, guard connect handlers when identifiers/services are unavailable. |
| `crates/core/runtime/scripting/src/context.rs` | Existing proxy command tests | Proxy methods publish `mesh.<interface>.<command>` with JSON payloads derived from contract arguments. |
| `crates/core/runtime/backend/src/lib.rs` | Existing backend command dispatch tests | Backend dispatch calls `on_command_<name>()` and exposes `mesh.service.payload()`. |
| `crates/core/shell/src/shell/service.rs` | `script_events_to_requests_maps_named_proxy_commands` | Convert published command channels into `CoreRequest::ServiceCommand`. |
| `crates/core/shell/src/shell/component.rs` | Phase 2/3 bundled-style integration tests | Seed proxy state, call handlers, route published events, assert fallback copy and diagnostics. |

## Reusable Snippets

### Proxy Lookup With Fallback

```luau
local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
if not audio_ok then audio = nil end
```

### Rerender-Derived State

```luau
function onRender()
    if not audio_ok or not audio then
        audio_label = "Audio unavailable"
        return
    end
    audio_label = string.format("%d%%", audio.percent or 0)
end
```

### Named Proxy Command

```luau
function onToggleWiFi()
    if network_ok and network then
        network.set_wifi_enabled(not (network.wifi_enabled or false))
    end
end
```

## Constraints

- Service-specific command-line logic remains in Luau backend plugins.
- Frontend service updates are callback-free; do not add `mesh.service.bind`, `mesh.service.on`, or proxy `on_change`.
- Use token-based `.mesh` styles only.
- Quick settings owns controls; panel stays compact.
