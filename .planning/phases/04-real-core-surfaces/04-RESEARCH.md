# Phase 4: Real Core Surfaces - Research

**Date:** 2026-05-03
**Status:** Complete

## Objective

Research how to implement Phase 4: Real Core Surfaces.

Phase 4 connects the shipped top panel and quick settings to real audio and network backend service data. It must prove the finalized scripting contract in user-visible surfaces: frontend scripts read service proxy fields on rerender, invoke named proxy commands from element handlers, and show clear disabled/unavailable states when providers cannot satisfy a control.

## Key Findings

### Current Surface State

- `packages/plugins/frontend/core/panel/src/main.mesh` already reads `@mesh/audio`, `@mesh/power`, and `@mesh/network` via `pcall(require, ...)`.
- The panel already derives `batteryText`, `volumeLevel`, `volumeIcon`, and `networkStatus` in `onRender()`.
- `packages/plugins/frontend/core/quick-settings/src/main.mesh` already uses proxy fields for the audio nav icon and Wi-Fi enabled state, and calls `network.set_wifi_enabled(...)`.
- `packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh` already displays audio percent, mute icon, backend/source label, and step/mute buttons.
- `packages/plugins/frontend/core/quick-settings/src/components/wifi-section.mesh` already reads `network.wifi_enabled` and provider-extra `network.networks`.
- `packages/plugins/frontend/core/quick-settings/src/components/wifi-item.mesh` already calls `network_iface.connect(network_id)`, but it currently assumes a result table and does not explicitly guard unavailable/permission-denied display state.

### Contract Mismatch To Resolve

The audio interface contract documents `set_volume(device_id, volume)` and `set_muted(device_id, muted)`, while existing bundled providers implement `on_command_set_volume()` by reading `payload.percent`. Existing runtime tests in `crates/core/runtime/scripting/src/context.rs` prove `audio:set_volume("default", 0.5)` publishes:

```json
{ "device_id": "default", "volume": 0.5 }
```

Existing shell service routing tests in `crates/core/shell/src/shell/service.rs` still accept the legacy event-channel payload `{ "percent": 55 }`.

Phase 4 should reconcile this before relying on quick-settings slider behavior:

- Frontend controls should call the finalized proxy method: `audio.set_volume(...)` or `audio:set_volume(...)`.
- Backend audio providers should accept contract-shaped payloads (`volume` as a `0.0..1.0` float with `device_id`) and may preserve legacy `percent` payload compatibility.
- Tests should cover both provider command payload normalization and quick-settings command publication.

### Network Provider Data

`packages/plugins/backend/core/networkmanager-network/src/main.luau` emits:

- `available`
- `wifi_enabled`
- `connections`
- `devices`
- provider-extra `networks`
- `source_plugin`

It implements `connect`, `disconnect`, `wifi_scan`, and `set_wifi_enabled`. The Phase 4 UI should treat `network.networks` as a richer-provider extra and keep connect/disconnect conditional on reliable identifiers and active state. A full network-manager UI is out of scope.

### Existing Test Anchors

Useful existing tests and patterns:

- `crates/core/runtime/scripting/src/context.rs` has proxy command tests for `audio:set_volume("default", 0.5)`.
- `crates/core/shell/src/shell/service.rs` maps event channels like `mesh.audio.set_volume` and `mesh.network.set_wifi_enabled` to `CoreRequest::ServiceCommand`.
- `crates/core/shell/src/shell/component.rs` contains bundled-style integration tests for proxy command dispatch and missing-service fallback copy.

## Recommended Plan Shape

1. **Runtime/provider command compatibility**
   - Normalize audio command payload support so frontend proxy calls and current providers agree.
   - Add focused runtime/backend tests around audio `set_volume`.

2. **Quick settings controls and disabled states**
   - Update `audio-section.mesh`, `wifi-section.mesh`, and `wifi-item.mesh`.
   - Add slider `onchange` to call `audio.set_volume(...)`.
   - Add visible unavailable/permission-denied copy and guarded handlers.

3. **Panel proof and end-to-end verification**
   - Keep panel compact.
   - Ensure panel displays live real service fields and opens quick settings for control.
   - Add/extend shell tests proving panel/quick-settings render state and command dispatch.

## Validation Architecture

### Automated Validation

- `cargo test -p mesh-core-scripting`
  - Covers proxy command payload publication and frontend Luau command shape.
- `cargo test -p mesh-core-backend`
  - Covers backend command dispatch to `on_command_set_volume`, if provider command normalization touches backend runtime tests.
- `cargo test -p mesh-core-shell`
  - Covers service event delivery, bundled-style frontend render state, command routing, and missing-service fallback copy.

### Static Validation

- `rg -n "audio\\.set_volume|audio:set_volume|onchange=\\{onVolumeChange\\}|network\\.set_wifi_enabled|Network unavailable|Audio unavailable" packages/plugins/frontend/core/quick-settings`
- `rg -n "mesh\\.service\\.bind|mesh\\.service\\.on|\\.on_change\\(" packages/plugins/frontend/core/panel/src/main.mesh packages/plugins/frontend/core/quick-settings/src`
- `rg -n "set_volume|device_id|volume|percent" packages/plugins/backend/core/*audio*/src/main.luau packages/plugins/backend/core/audio-interface/interface.toml`

### Manual Validation

The final surface behavior should be checked in a running shell session when available:

- Panel shows at least one live backend service value.
- Quick settings audio shows percent, mute icon, source label, and allows volume/mute controls.
- Quick settings Wi-Fi shows enabled state, networks if emitted, and guarded connect/toggle behavior.
- Unavailable provider paths show concise disabled text instead of blank UI.

## Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Audio command contract mismatch causes quick-settings slider to publish a command providers ignore | high | Start with audio command compatibility and tests before UI slider work. |
| UI shows controls as active when providers are unavailable or permission denied | medium | Add explicit `audio_status` / `wifi_status` style reactive state and guard handlers. |
| Network rows call `connect` with unstable identifiers | medium | Only enable row commands when `network.id` is non-empty and provider data is trusted; otherwise show display-only rows or `Connection details unavailable`. |
| Panel grows beyond compact status role | low | Preserve panel as status/entry surface; keep direct controls in quick settings. |

## Research Complete

The phase is ready for planning.
