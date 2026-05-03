---
phase: 04-real-core-surfaces
verified: 2026-05-03T12:26:55Z
status: human_needed
score: 5/5 must-haves verified
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 2/5
  gaps_closed:
    - "Quick settings renders live audio and network state: nested reactive equality now compares full serde_json::Value payloads."
    - "Quick settings can issue supported network commands: NetworkManager handlers consume connection_id with legacy id fallback."
    - "Panel opens quick settings and quick settings close routes through supported shell surface APIs."
    - "Public audio set_muted contract has concrete PipeWire and PulseAudio provider handlers."
    - "Read versus control capabilities are enforced for proxy command methods and shell service-command dispatch."
    - "Audio play_sound no longer concatenates payload path into a shell command."
  gaps_remaining: []
  regressions: []
human_verification:
  - test: "Run MESH with bundled core panel, quick-settings, audio provider, and NetworkManager provider in a Wayland/dev-shell session."
    expected: "Top panel shows live audio/network/power state, quick settings opens/closes through shell surface routing, and audio/network controls mutate host services and rerender from provider emissions."
    why_human: "This depends on live Wayland UI behavior and host audio/NetworkManager services, which automated grep/unit checks cannot fully prove."
---

# Phase 4: Real Core Surfaces Verification Report

**Phase Goal:** Connect top panel and quick settings to real backend service data, with interactive audio and network controls using the finalized scripting contract.  
**Verified:** 2026-05-03T12:26:55Z  
**Status:** human_needed  
**Re-verification:** Yes - after gap closure

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Top panel renders at least one real backend service value. | VERIFIED | `panel/src/main.mesh` requires audio/power/network proxies and `onRender()` reads `audio.percent`, `audio.muted`, `network.connections`, and `power.level`; `real_core_surfaces_panel_render_state_changes_with_seeded_service_payloads` passes in `nix develop -c cargo test`. |
| 2 | Quick settings renders live audio and network state. | VERIFIED | `audio-section.mesh` reads `audio.percent`, `audio.muted`, and `audio.source_plugin`; `wifi-section.mesh` reads `network.available`, `network.wifi_enabled`, and `network.networks`; `reactive_values_equal()` is now full `previous == next`, and `reactive_table_compares_nested_values` passes. |
| 3 | Quick settings can change audio volume and mute state through service proxy commands. | VERIFIED | `audio-section.mesh` calls `audio.set_volume`, `audio.volume_down`, `audio.volume_up`, and `audio.toggle_mute`; both bundled audio providers implement `on_command_set_volume`, `on_command_toggle_mute`, and `on_command_set_muted`; command tests pass. |
| 4 | Quick settings can issue supported network commands through the service proxy. | VERIFIED | `quick-settings/src/main.mesh` calls `network.set_wifi_enabled`; `wifi-item.mesh` calls guarded `network.connect(network_id)`; NetworkManager handlers read `payload.connection_id or payload.id`, reject empty IDs, and run structured `mesh.exec` commands. |
| 5 | The surfaces exercise the same public APIs documented for external plugins. | VERIFIED | Docs show `audio.set_volume`, `network.set_wifi_enabled`, `shell.toggle-surface`, and `shell.hide-surface` with `surface_id = "@mesh/quick-settings"`; legacy service callback APIs and unsupported quick-settings shell events are absent from shipped surfaces/docs/tests. |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `packages/plugins/frontend/core/panel/src/main.mesh` | Compact top panel live-service proof and quick-settings entry | VERIFIED | Reads live proxy fields and publishes `shell.toggle-surface` with `surface_id = "@mesh/quick-settings"`. |
| `packages/plugins/frontend/core/quick-settings/src/main.mesh` | Quick-settings root state and Wi-Fi/shell handlers | VERIFIED | Guards network availability/control state, calls `network.set_wifi_enabled`, and publishes `shell.hide-surface`. |
| `packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh` | Audio live state and controls | VERIFIED | Renders proxy state and uses finalized named proxy command methods. |
| `packages/plugins/frontend/core/quick-settings/src/components/wifi-section.mesh` | Network live state/list surface | VERIFIED | Renders provider state, unavailable/control-denied copy, disabled state, and scanning state. |
| `packages/plugins/frontend/core/quick-settings/src/components/wifi-item.mesh` | Guarded Wi-Fi row connect behavior | VERIFIED | Rejects empty IDs and unavailable/control-denied providers before calling `network.connect`. |
| `packages/plugins/backend/core/networkmanager-network/src/main.luau` | Network command provider behavior | VERIFIED | Consumes `connection_id`, preserves legacy `id`, validates IDs, and uses structured `mesh.exec`. |
| `packages/plugins/backend/core/pipewire-audio/src/main.luau` | PipeWire audio provider commands | VERIFIED | Implements volume, toggle mute, set muted, and safe structured sound playback. |
| `packages/plugins/backend/core/pulseaudio-audio/src/main.luau` | PulseAudio provider commands | VERIFIED | Implements volume, toggle mute, set muted, and safe structured sound playback. |
| `crates/core/runtime/scripting/src/context.rs` | Proxy runtime reactivity and command authorization | VERIFIED | Deep value equality and `service.<name>.control` checks for proxy command methods are implemented and tested. |
| `crates/core/shell/src/shell/service.rs` / `mod.rs` / `types.rs` | Shell event routing and dispatch authorization | VERIFIED | Service commands carry source identity/capabilities and are checked before routing and backend dispatch. |
| `docs/plugins/frontend/core/README.md` | Public API alignment | VERIFIED | Documents the same named proxy methods and shell surface events used by shipped surfaces. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `panel/src/main.mesh` | shell router | `shell.toggle-surface` + `surface_id` | WIRED | `real_core_surfaces_panel_volume_click_publishes_quick_settings_toggle` passes. |
| `quick-settings/src/main.mesh` | shell router | `shell.hide-surface` + `surface_id` | WIRED | `real_core_surfaces_quick_settings_close_publishes_hide_surface` passes. |
| `audio-section.mesh` | audio providers | `audio.set_volume` -> `on_command_set_volume` | WIRED | Proxy publication, backend payload preservation, and provider handlers are present; tests pass. |
| `audio-section.mesh` | audio providers | `audio.toggle_mute` / step methods -> provider handlers | WIRED | Both providers implement `on_command_toggle_mute`, `on_command_volume_up`, and `on_command_volume_down`. |
| `wifi-section.mesh` / `wifi-item.mesh` | NetworkManager provider | `network.networks`, `network.set_wifi_enabled`, `network.connect` | WIRED | Provider emits state and consumes `connection_id`; shell service-command tests pass. |
| read-only service proxy | capability gate | missing `service.<name>.control` denies mutation | WIRED | Runtime and shell gates both check source capabilities; denial tests pass. |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|--------------------|--------|
| `panel/src/main.mesh` | `volumeLevel`, `volumeIcon`, `networkStatus`, `batteryText` | audio/network/power service proxy fields | Yes | FLOWING |
| `audio-section.mesh` | `audio_percent`, `audio_label`, `audio_backend`, `icon_name` | audio provider state fields | Yes | FLOWING |
| `wifi-section.mesh` | `wifi_networks`, `wifi_enabled`, `network_status` | NetworkManager `mesh.service.emit` payloads | Yes | FLOWING |
| `wifi-item.mesh` | connect command payload | `network.connect(network_id)` -> interface arg `connection_id` -> provider payload | Yes | FLOWING |
| audio providers | host mute/volume/playback commands | proxy command payloads -> Luau handlers -> `wpctl`/`pactl`/`aplay` | Yes | FLOWING |
| shell routing | quick-settings open/close | frontend events -> `CoreRequest::ToggleSurface` / `HideSurface` | Yes | FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full automated test suite | `nix develop -c cargo test` | Passed: backend 4, scripting 43, shell 39, service 11, and remaining workspace tests/doc-tests passed. | PASS |
| Gap-closure artifacts | `gsd-sdk query verify.artifacts` for plans 04, 05, 06 | All passed: 3/3, 3/3, 4/4. | PASS |
| Shell routing key links | `gsd-sdk query verify.key-links 04-06-PLAN.md` | All verified: 2/2. | PASS |
| Unsupported quick-settings shell events absent | `grep -R "shell\\.(toggle|close)-quick-settings" ...` | No matches. | PASS |
| Unsafe audio playback shell concatenation absent | `grep -R "exec_shell(...aplay|aplay "` in audio providers | No matches. | PASS |
| Service-specific command logic remains outside Rust core | `grep -R "wpctl|pactl|nmcli|bluetoothctl" crates/core` | No matches. | PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| SURF-01 | `04-03-PLAN.md`, `04-06-PLAN.md` | Top panel renders live data from at least one real backend service. | SATISFIED | Panel reads audio/network/power proxy fields and routes quick-settings entry through supported shell events. |
| SURF-02 | `04-02-PLAN.md`, `04-03-PLAN.md`, `04-04-PLAN.md`, `04-06-PLAN.md` | Quick settings renders live audio state from a real backend provider. | SATISFIED | Audio section reads live proxy fields; nested reactive update coverage passes. |
| SURF-03 | `04-01-PLAN.md` through `04-06-PLAN.md` | Quick settings can change audio volume and mute state through service commands. | SATISFIED | UI calls named proxy commands; proxy/backend/provider tests pass; `set_muted` handlers exist. |
| SURF-04 | `04-02-PLAN.md`, `04-03-PLAN.md`, `04-04-PLAN.md`, `04-06-PLAN.md` | Quick settings renders live network state from a real backend provider. | SATISFIED | Wi-Fi section reads `network.networks` and `network.wifi_enabled`; provider emits those fields; deep equality prevents stale same-length arrays. |
| SURF-05 | `04-02-PLAN.md` through `04-06-PLAN.md` | Quick settings can toggle or command network state through the service proxy contract where supported. | SATISFIED | `set_wifi_enabled` and `connect` paths are wired through proxy methods and provider payload handlers. |

All Phase 04 requirement IDs in `.planning/REQUIREMENTS.md` are accounted for by PLAN frontmatter. No additional Phase 04 requirement IDs were found beyond SURF-01 through SURF-05.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/core/shell/src/shell/component.rs` | 1354 | `WidgetNode::new("box") // placeholder, takes no space` | INFO | Intentional invisible portal placeholder; not user-facing and not a Phase 04 stub. |
| `packages/plugins/frontend/core/quick-settings/src/components/wifi-section.mesh` | 31, 46 | `wifi_networks = {}` defaults | INFO | Initial/fallback state is overwritten from `network.networks` in `onRender()`; not hollow data. |
| `packages/plugins/backend/core/networkmanager-network/src/main.luau` | multiple | local empty tables | INFO | Parser/cache accumulator initialization before real command output parsing; not rendered stub data. |

No blocker or warning anti-patterns were found in Phase 04 touched files.

### Human Verification Required

1. **Live host surface flow**

**Test:** Start MESH with bundled core frontend/backend plugins in a Wayland/dev-shell environment with audio and NetworkManager available. Click the panel audio indicator, close quick settings, move the volume slider, toggle mute/step controls, toggle Wi-Fi, and connect/disconnect a safe test network entry where available.  
**Expected:** Panel and quick settings render live provider values; open/close routes through shell surfaces; service controls mutate host services and rerender from emitted state; unavailable/control-denied states remain visible when providers or permissions are absent.  
**Why human:** This verifies live UI, real-time provider updates, and host service integration outside deterministic unit tests.

### Gaps Summary

No blocking gaps remain. The six gaps from the previous verification are closed by code evidence and passing tests. Automated verification confirms the Phase 04 roadmap success criteria are met in the codebase. Final status is `human_needed` only because the workflow requires live UI and external service validation for this class of phase.

---

_Verified: 2026-05-03T12:26:55Z_  
_Verifier: the agent (gsd-verifier)_
