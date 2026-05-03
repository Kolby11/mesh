---
phase: 04-real-core-surfaces
verified: 2026-05-03T07:00:23Z
status: gaps_found
score: 2/5 must-haves verified
overrides_applied: 0
gaps:
  - truth: "Quick settings renders live audio and network state"
    status: failed
    reason: "Network list updates can be dropped when the emitted array length stays the same because nested objects/arrays compare equal by shape only."
    artifacts:
      - path: "crates/core/runtime/scripting/src/context.rs"
        issue: "reactive_values_equal() treats nested Object/Object and Array/Array entries as equal without comparing contents."
      - path: "packages/plugins/frontend/core/quick-settings/src/components/wifi-section.mesh"
        issue: "wifi_networks is assigned from network.networks, so same-length network changes can remain stale."
    missing:
      - "Compare nested JSON/Luau values by actual value, or otherwise dirty tracked service fields when nested payload contents change."
  - truth: "Quick settings can issue supported network commands through the service proxy"
    status: failed
    reason: "The public network contract publishes connect payload field connection_id, but the NetworkManager backend reads payload.id."
    artifacts:
      - path: "packages/plugins/backend/core/network-interface/interface.toml"
        issue: "connect/disconnect argument name is connection_id."
      - path: "packages/plugins/frontend/core/quick-settings/src/components/wifi-item.mesh"
        issue: "network.connect(network_id) publishes the contract argument name connection_id."
      - path: "packages/plugins/backend/core/networkmanager-network/src/main.luau"
        issue: "on_command_connect() and on_command_disconnect() read payload.id, so Quick Settings connect/disconnect IDs are ignored."
    missing:
      - "Align backend command handlers with the contract payload, or change the contract and all callers/tests consistently."
  - truth: "Panel opens quick settings for controls and quick settings close routes through shell surface APIs"
    status: failed
    reason: "The surfaces publish shell.toggle-quick-settings and shell.close-quick-settings, but the shell router only handles shell.toggle-surface and shell.hide-surface before falling through to service-command routing."
    artifacts:
      - path: "packages/plugins/frontend/core/panel/src/main.mesh"
        issue: "onVolumeClick publishes shell.toggle-quick-settings with an empty payload."
      - path: "packages/plugins/frontend/core/quick-settings/src/main.mesh"
        issue: "onClose publishes shell.close-quick-settings with an empty payload."
      - path: "crates/core/shell/src/shell/service.rs"
        issue: "script_events_to_requests() has no cases for those named events and maps unknown dotted channels as ServiceCommand."
      - path: "docs/plugins/frontend/core/README.md"
        issue: "Docs teach the unsupported named quick-settings events."
    missing:
      - "Use shell.toggle-surface/shell.hide-surface with surface_id, or add explicit router support and tests for the named events."
  - truth: "The surfaces exercise the same public APIs documented for external plugins"
    status: failed
    reason: "The public audio contract exposes set_muted(), but neither shipped audio provider implements on_command_set_muted(), so the documented/proxy-callable command is silently ignored."
    artifacts:
      - path: "packages/plugins/backend/core/audio-interface/interface.toml"
        issue: "Declares method set_muted(device_id, muted)."
      - path: "packages/plugins/backend/core/pipewire-audio/src/main.luau"
        issue: "Implements toggle_mute but no on_command_set_muted."
      - path: "packages/plugins/backend/core/pulseaudio-audio/src/main.luau"
        issue: "Implements toggle_mute but no on_command_set_muted."
    missing:
      - "Implement on_command_set_muted in both providers or remove/replace the method in the public contract and docs."
  - truth: "The finalized scripting contract enforces read versus control capabilities for service mutations"
    status: failed
    reason: "Requiring a service with read capability creates a proxy that exposes all contract methods, and shell dispatch does not re-check control capability before sending ServiceCommand."
    artifacts:
      - path: "crates/core/runtime/scripting/src/context.rs"
        issue: "create_service_proxy() creates command functions for all contract methods without knowing whether service.<name>.control is granted."
      - path: "crates/core/shell/src/shell/mod.rs"
        issue: "dispatch_service_command() forwards commands to service handlers without an authorization gate."
    missing:
      - "Pass caller capability context into proxy creation and enforce service.<name>.control before publishing/dispatching service commands."
  - truth: "Backend service commands are safe implementations of the finalized service contract"
    status: failed
    reason: "Audio play_sound command concatenates a payload path into mesh.exec_shell(), allowing shell injection through service command data."
    artifacts:
      - path: "packages/plugins/backend/core/pipewire-audio/src/main.luau"
        issue: "on_command_play_sound() runs mesh.exec_shell(\"aplay \" .. payload.path)."
      - path: "packages/plugins/backend/core/pulseaudio-audio/src/main.luau"
        issue: "on_command_play_sound() runs mesh.exec_shell(\"aplay \" .. payload.path)."
    missing:
      - "Validate non-empty string paths and call mesh.exec(\"aplay\", { path }) instead of shell concatenation."
---

# Phase 4: Real Core Surfaces Verification Report

**Phase Goal:** Connect top panel and quick settings to real backend service data, with interactive audio and network controls using the finalized scripting contract.  
**Verified:** 2026-05-03T07:00:23Z  
**Status:** gaps_found  
**Re-verification:** No - initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Top panel renders at least one real backend service value | VERIFIED | `packages/plugins/frontend/core/panel/src/main.mesh:21-23` requires audio/power/network proxies; `:92-97` reads `audio.percent`, `audio.muted`, `network.connections`, and `power.level` during `onRender()`. |
| 2 | Quick settings renders live audio and network state | FAILED | Audio state is read from proxy fields, but network list liveness is not reliable: `context.rs:126-154` shallow-compares nested arrays/objects, and `wifi-section.mesh:57-58` assigns `wifi_networks = network.networks`, so same-length list content changes can be ignored. |
| 3 | Quick settings can change audio volume and mute state through service proxy commands | VERIFIED | `audio-section.mesh:90-123` calls `audio.set_volume`, `audio.volume_down`, `audio.volume_up`, and `audio.toggle_mute`; providers implement `on_command_set_volume` and `on_command_toggle_mute`. Separate public contract gaps are listed below. |
| 4 | Quick settings can issue supported network commands through the service proxy | FAILED | Wi-Fi toggle publishes `network.set_wifi_enabled`, but supported `network.connect(network_id)` publishes `connection_id` from the contract while `networkmanager-network/src/main.luau:134-141` reads `payload.id`. |
| 5 | Surfaces exercise the same public APIs documented for external plugins | FAILED | Docs and shipped surfaces use unsupported shell events; read-only service proxies expose mutating methods; audio contract method `set_muted` has no provider handler. |

**Score:** 2/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `packages/plugins/frontend/core/panel/src/main.mesh` | Compact top panel live-service proof | PARTIAL | Reads real service fields, but its quick-settings toggle event is not routed by the shell. |
| `packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh` | Audio control surface | VERIFIED | Reads proxy state and calls named audio methods. |
| `packages/plugins/frontend/core/quick-settings/src/components/wifi-section.mesh` | Wi-Fi state and network list surface | PARTIAL | Reads provider state, but nested network list changes can be lost by runtime equality logic. |
| `packages/plugins/frontend/core/quick-settings/src/components/wifi-item.mesh` | Guarded Wi-Fi row command behavior | PARTIAL | Guards empty IDs, but valid connect IDs are sent under the contract field `connection_id` and ignored by the backend. |
| `packages/plugins/backend/core/pipewire-audio/src/main.luau` | PipeWire audio provider command behavior | PARTIAL | Implements set_volume/toggle_mute but not set_muted; play_sound has shell injection risk. |
| `packages/plugins/backend/core/pulseaudio-audio/src/main.luau` | PulseAudio audio provider command behavior | PARTIAL | Implements set_volume/toggle_mute but not set_muted; play_sound has shell injection risk. |
| `packages/plugins/backend/core/networkmanager-network/src/main.luau` | Network provider command behavior | PARTIAL | Implements set_wifi_enabled, but connect/disconnect payload names drift from the contract. |
| `crates/core/runtime/scripting/src/context.rs` | Public service proxy runtime | PARTIAL | Publishes named commands, but command methods are exposed without control-capability enforcement and nested reactive equality is shallow. |
| `crates/core/shell/src/shell/component.rs` | Shell-facing regressions | PARTIAL | Tests exist, but at least one asserts the unsupported `shell.toggle-quick-settings` event instead of routed surface APIs. |
| `docs/plugins/frontend/core/README.md` | Public frontend docs alignment | PARTIAL | Named proxy command docs exist, but shell event examples still teach unsupported quick-settings event names. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `audio-section.mesh` | audio providers | `audio.set_volume` -> `on_command_set_volume` | WIRED | `audio-section.mesh:96` calls `audio.set_volume`; both audio providers implement `on_command_set_volume` and normalize `payload.volume`. |
| `audio-section.mesh` | audio providers | mute/step named methods | WIRED | `audio-section.mesh:105-123` calls `volume_down`, `volume_up`, and `toggle_mute`; providers implement those handlers. |
| `wifi-section.mesh` | network provider state | `network.networks`, `network.wifi_enabled` | PARTIAL | Reads real fields, but nested same-length list updates can be skipped by `reactive_values_equal()`. |
| `wifi-item.mesh` | network provider connect | `network.connect(network_id)` | NOT_WIRED | Contract argument maps to `connection_id`; provider reads `payload.id`. |
| `panel/src/main.mesh` | quick-settings surface | panel opens quick settings for controls | NOT_WIRED | Publishes `shell.toggle-quick-settings`; router only recognizes `shell.toggle-surface` with `surface_id`. |
| `quick-settings/src/main.mesh` | shell surface routing | close quick settings | NOT_WIRED | Publishes `shell.close-quick-settings`; router only recognizes `shell.hide-surface` with `surface_id`. |

### Data-Flow Trace

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|--------------------|--------|
| `panel/src/main.mesh` | `volumeLevel`, `networkStatus`, `batteryText` | `audio.percent`, `audio.muted`, `network.connections`, `power.level` | Yes | FLOWING for render state. |
| `audio-section.mesh` | `audio_percent`, `audio_label`, `icon_name` | `audio.percent`, `audio.muted`, `audio.source_plugin` | Yes | FLOWING. |
| `wifi-section.mesh` | `wifi_networks`, `wifi_enabled` | `network.networks`, `network.wifi_enabled` from NetworkManager provider | Partially | HOLLOW for same-length nested list changes due shallow equality. |
| `wifi-item.mesh` | service command payload | `network.connect(network_id)` -> contract args | No | DISCONNECTED at backend because provider reads `payload.id`. |
| `networkmanager-network/src/main.luau` | `networks`, `connections`, `wifi_enabled` | `nmcli` command output | Yes | FLOWING for state; command payload mismatch remains. |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Audio proxy publishes finalized set_volume payload | `cargo test -p mesh-core-scripting interface_proxy_method_publishes_service_command -- --nocapture` | 1 passed | PASS |
| Backend preserves normalized set_volume payload | `cargo test -p mesh-core-backend set_volume -- --nocapture` | 1 passed | PASS |
| Service contract registry tests | `cargo test -p mesh-core-service -- --nocapture` | 11 passed, 2 doctests ignored | PASS |
| Shell real_core_surfaces regressions | `cargo test -p mesh-core-shell real_core_surfaces -- --nocapture` | Blocked before tests by missing `xkbcommon.pc` required by `smithay-client-toolkit` | SKIP |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| SURF-01 | `04-03-PLAN.md` | Top panel renders live data from at least one real backend service. | SATISFIED | Panel reads audio/network/power proxy fields in `onRender()`. |
| SURF-02 | `04-02-PLAN.md`, `04-03-PLAN.md` | Quick settings renders live audio state from a real backend provider. | SATISFIED | Audio section reads `audio.percent`, `audio.muted`, and `audio.source_plugin`; audio providers emit those fields. |
| SURF-03 | `04-01-PLAN.md`, `04-02-PLAN.md`, `04-03-PLAN.md` | Quick settings can change audio volume and mute state through service commands. | PARTIAL | Quick settings calls set_volume/toggle_mute and providers implement those, but public `set_muted` is declared and not handled by providers. |
| SURF-04 | `04-02-PLAN.md`, `04-03-PLAN.md` | Quick settings renders live network state from a real backend provider. | BLOCKED | Network provider emits real state, but same-length nested list updates can be skipped by runtime equality. |
| SURF-05 | `04-02-PLAN.md`, `04-03-PLAN.md` | Quick settings can toggle or command network state through the service proxy contract where supported. | BLOCKED | `set_wifi_enabled` is wired, but supported connect/disconnect command payload names are mismatched. |

No additional Phase 4 requirement IDs were found in `.planning/REQUIREMENTS.md` beyond SURF-01 through SURF-05.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `packages/plugins/backend/core/pipewire-audio/src/main.luau` | 126 | `mesh.exec_shell("aplay " .. payload.path)` | BLOCKER | Shell injection through service command path. |
| `packages/plugins/backend/core/pulseaudio-audio/src/main.luau` | 77 | `mesh.exec_shell("aplay " .. payload.path)` | BLOCKER | Shell injection through service command path. |
| `crates/core/runtime/scripting/src/context.rs` | 148-154 | Shallow nested equality | BLOCKER | Same-length object/array payload changes can fail to dirty UI state. |
| `crates/core/runtime/scripting/src/context.rs` | 881-910 | Command methods exposed unconditionally | BLOCKER | Read-only service access can publish mutating service commands. |
| `packages/plugins/frontend/core/quick-settings/src/main.mesh` | 108-111 | Active nav class variables computed but unused | WARNING | Selected quick-settings section has no active visual state. |

### Human Verification Required

Runtime host verification remains required after blockers are fixed because real provider behavior depends on Wayland, audio services, and NetworkManager:

1. Start MESH with bundled core frontend/backend plugins in a Wayland/dev shell environment.
2. Confirm the top panel rerenders when audio, network, and power providers emit changed state.
3. Confirm panel audio click opens/toggles quick settings through the shell surface router.
4. Confirm quick-settings slider, mute/step, Wi-Fi toggle, and Wi-Fi connect/disconnect commands mutate the host service and rerender from emitted state.
5. Disable providers or permissions and confirm visible fallback copy remains non-empty.

### Gaps Summary

The phase has real implementation, not just stubs: panel state reads, quick-settings audio controls, Wi-Fi toggle, provider emissions, and several targeted tests exist. The goal is not achieved because multiple critical links in the finalized public contract are broken or unsafe:

- Network connect/disconnect commands are published with one payload shape and consumed with another.
- Shell quick-settings events used by shipped surfaces and docs are not routed by the shell.
- Network list liveness is undermined by shallow nested equality.
- Read-only service proxies can publish mutating commands.
- The public audio contract declares a command providers do not implement.
- Audio provider command handling includes shell injection risk.

These are BLOCKER gaps. Do not proceed as if Phase 4 is complete until they are fixed or explicitly overridden by a human decision.

---

_Verified: 2026-05-03T07:00:23Z_  
_Verifier: the agent (gsd-verifier)_
