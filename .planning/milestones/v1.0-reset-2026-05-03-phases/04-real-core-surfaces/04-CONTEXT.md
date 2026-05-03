# Phase 4: Real Core Surfaces - Context

**Gathered:** 2026-05-03
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase connects the shipped top panel and quick settings surfaces to real backend service data. The top panel should prove live service-backed display without becoming a control-heavy surface. Quick settings should provide the primary audio and Wi-Fi controls through the finalized `require("@mesh/<service>")` proxy state-and-command contract. The phase should exercise the same public APIs external plugin authors will use: field reads on service proxies, named proxy command methods, reactive rerendering, and visible disabled states when providers are unavailable or lack permission.

</domain>

<decisions>
## Implementation Decisions

### Top Panel Proof
- **D-01:** The top panel proof stays minimal: it should show live service-backed indicators and route users into quick settings for control.
- **D-02:** Direct panel controls for audio or network state are not required in Phase 4. The panel may keep existing affordances that open or position richer controls, but Phase 4 success does not depend on changing service state directly from the panel.
- **D-03:** The top panel should derive displayed state from live proxy fields during rerender, following the callback-free model locked in Phase 2.

### Quick Settings Audio
- **D-04:** Quick settings audio should provide the full primary control set: live volume percent, mute state, backend/source label, a volume slider, and mute/step controls where supported.
- **D-05:** The volume slider should use the finalized proxy command path, not the legacy event-channel path. It should call `audio.set_volume(...)` through `require("@mesh/audio")`.
- **D-06:** Mute and step controls should use service proxy command methods such as `audio.toggle_mute()`, `audio.volume_up()`, and `audio.volume_down()` where the active provider supports them.
- **D-07:** Audio display state should be derived from proxy fields such as `audio.percent`, `audio.muted`, `audio.available`, and `audio.source_plugin` on rerender.

### Quick Settings Network
- **D-08:** Quick settings network should implement core Wi-Fi controls, not a full network manager suite.
- **D-09:** Required network behavior is live Wi-Fi enabled state, an available networks list when the active provider emits one, and Wi-Fi on/off through `network.set_wifi_enabled(...)`.
- **D-10:** Connect and disconnect controls are in scope only when provider data is sufficient and safe. The planner may keep these limited or conditional rather than forcing a polished network management UI.
- **D-11:** Network state should be read from proxy fields such as `network.available`, `network.wifi_enabled`, `network.connections`, `network.devices`, and provider-emitted `network.networks`.

### Unavailable and Permission-Denied States
- **D-12:** Surfaces should show visible disabled states for unavailable services or permission-denied controls.
- **D-13:** Affected sections should use concise user-facing copy such as unavailable or permission denied, disable controls that cannot work, and rely on diagnostics/logs for technical provider details.
- **D-14:** The UI should not silently hide failed service state, and it should not expose raw developer-level command failures as the primary user experience.

### Carry-Forward Decisions
- **D-15:** Service proxies remain read-and-command surfaces only. Do not reintroduce `proxy.on_change(...)`, `mesh.service.bind(...)`, or service subscription APIs in shipped surfaces.
- **D-16:** Frontend writes happen through named proxy command methods, never by mutating proxy state fields directly.
- **D-17:** Service-specific logic stays in Luau backend providers. Rust core changes, if needed, should remain generic wiring/runtime support.

### the agent's Discretion
- The planner may choose the exact visual composition of the top panel indicators and quick settings sections as long as the decisions above and Phase 4 success criteria are met.
- The planner may decide whether connect/disconnect controls appear initially, are disabled until sufficient provider data exists, or are deferred inside Phase 4 planning if the current provider payload cannot support them safely.
- The planner may choose the exact copy for disabled/unavailable states, keeping it concise and user-facing.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning Scope
- `.planning/PROJECT.md` — milestone goal, external-developer target, and requirement that real surfaces validate the public scripting contract.
- `.planning/REQUIREMENTS.md` — Phase 4 requirement IDs `SURF-01` through `SURF-05`.
- `.planning/ROADMAP.md` — Phase 4 goal, success criteria, dependency on Phase 3, and UI hint.
- `.planning/STATE.md` — current project position and carry-forward decisions.

### Prior Phase Decisions
- `.planning/phases/03-frontend-reactivity-and-events/03-CONTEXT.md` — reactive globals, typed `on_change`, handler behavior, and navigation-bar audio proof.
- `.planning/phases/02-service-proxy-delivery/02-CONTEXT.md` — read-and-command proxy model, command methods, callback removal, field-level invalidation, and dominant-provider extras.
- `.planning/phases/01-backend-host-api-contract/01-CONTEXT.md` — backend Luau host API contract and rule that service-specific logic stays in Luau providers.

### Codebase Maps
- `.planning/codebase/STRUCTURE.md` — plugin directory layout, where frontend surfaces and backend service providers live.
- `.planning/codebase/STACK.md` — Rust/Luau/mlua/Tokio and Wayland constraints.
- `.planning/codebase/ARCHITECTURE.md` — shell, service, render, and backend data flow; no-service-logic-in-core rule.

### Frontend Surfaces
- `packages/plugins/frontend/core/panel/src/main.mesh` — existing top panel service-backed indicators and quick-settings opener.
- `packages/plugins/frontend/core/quick-settings/src/main.mesh` — quick settings root, section navigation, and current Wi-Fi toggle path.
- `packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh` — current audio display and button controls; Phase 4 should add/repair slider command behavior here.
- `packages/plugins/frontend/core/quick-settings/src/components/wifi-section.mesh` — current Wi-Fi state/list rendering from network proxy fields.
- `packages/plugins/frontend/core/quick-settings/src/components/wifi-item.mesh` — likely connection row integration point if provider data supports connect/disconnect.
- `packages/plugins/frontend/core/navigation-bar/src/components/volume-button.mesh` — Phase 3 proof of typed slider behavior and current legacy event-channel volume command path.

### Service Contracts and Providers
- `packages/plugins/backend/core/audio-interface/interface.toml` — audio state fields and command methods.
- `packages/plugins/backend/core/network-interface/interface.toml` — network state fields and command methods.
- `packages/plugins/backend/core/pipewire-audio/src/main.luau` — real audio provider command handlers and emitted state.
- `packages/plugins/backend/core/pulseaudio-audio/src/main.luau` — alternate audio provider command handlers.
- `packages/plugins/backend/core/networkmanager-network/src/main.luau` — real network provider state, Wi-Fi toggle, scan, connect, and disconnect command handlers.

### Runtime and Shell Integration
- `crates/core/runtime/scripting/src/context.rs` — service proxy field reads, command methods, reactive state sync, and prior proxy tests.
- `crates/core/shell/src/shell/component.rs` — frontend input handling, service event handling, and existing regression fixtures around audio/network commands.
- `crates/core/shell/src/shell/service.rs` — service command routing from frontend requests into backend provider commands.
- `crates/core/runtime/backend/src/lib.rs` — backend command dispatch to `on_command_*` handlers.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `packages/plugins/frontend/core/panel/src/main.mesh` already reads `@mesh/audio`, `@mesh/power`, and `@mesh/network` through `pcall(require, ...)`, derives display state in `onRender()`, and opens quick settings from the volume indicator.
- `packages/plugins/frontend/core/quick-settings/src/main.mesh` already derives the nav audio icon and Wi-Fi enabled state from proxy fields and calls `network.set_wifi_enabled(...)`.
- `packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh` already renders volume percent, mute icon, backend label, and button controls; it needs slider command behavior aligned with the finalized proxy contract.
- `packages/plugins/frontend/core/quick-settings/src/components/wifi-section.mesh` already derives `wifi_enabled` and `wifi_networks` from `network.wifi_enabled` and `network.networks`.
- Audio and network interface contracts already declare the state fields and mutating methods Phase 4 should exercise.

### Established Patterns
- Shipped surfaces use `pcall(require, "@mesh/<service>@>=1.0")` for graceful service lookup while diagnostics remain visible.
- Surface scripts derive reactive globals during `onRender()` from live proxy fields; no service callbacks are needed.
- Mutating service actions flow through named proxy methods into backend `on_command_*` handlers.
- Provider-specific shell commands such as `wpctl`, `pactl`, `nmcli`, and `bluetoothctl` belong in Luau backend plugins, not Rust core.

### Integration Points
- Audio slider behavior connects quick-settings `onchange` handling to `audio.set_volume(...)`, then to audio provider `on_command_set_volume()`, then back through emitted `audio.percent` and rerender.
- Wi-Fi toggling connects quick-settings button/switch handling to `network.set_wifi_enabled(...)`, then to NetworkManager provider `on_command_set_wifi_enabled()`, then back through emitted `network.wifi_enabled`.
- Available network rows may connect `network.networks`/`network.connections` data to `network.connect(...)` or `network.disconnect(...)` only when identifiers and active state are reliable enough.
- Disabled/unavailable UI should be driven by service lookup failure, `available = false`, missing command support, or permission-denied command results while keeping technical details in diagnostics/logs.

</code_context>

<specifics>
## Specific Ideas

- Keep the top panel as a live status and entry surface. Quick settings is the primary control surface.
- Bring the quick-settings audio slider up to the same standard as the Phase 3 navigation-bar slider, but use direct `audio.set_volume(...)` proxy commands rather than publishing a legacy `mesh.audio.set-volume` event.
- Treat the NetworkManager provider's `networks` field as a richer-provider extra that can power the Wi-Fi list, while keeping the base path focused on `wifi_enabled` and safe toggling.
- Permission-denied and unavailable states should be visible but calm: disabled controls plus concise copy, with detailed provider errors left to diagnostics/logs.

</specifics>

<deferred>
## Deferred Ideas

- A full network manager surface with polished scan, device detail, connection profiles, and exhaustive connect/disconnect flows is not required for Phase 4 unless the current provider data makes a narrow version safe.
- Direct top panel controls for audio or network state are not required in Phase 4.

</deferred>

---

*Phase: 4-Real Core Surfaces*
*Context gathered: 2026-05-03*
