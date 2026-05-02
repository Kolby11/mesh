---
phase: 02-service-proxy-delivery
plan: 03
subsystem: frontend-surfaces
tags: [luau, frontend, service-proxy, bundled-surfaces, docs, shell]

requires:
  - phase: 02-service-proxy-delivery
    plan: 01
    provides: reactive proxy runtime with field-level tracking and lookup diagnostics
  - phase: 02-service-proxy-delivery
    plan: 02
    provides: formal base interface plugin model and commands-only proxy method surface

provides:
  - Bundled panel and quick-settings surfaces using direct proxy field reads on rerender
  - No legacy mesh.service.bind/on or on_change callback usage in bundled surfaces
  - Shell-facing integration tests for rerender reads, command dispatch, and degraded states
  - Frontend plugin documentation aligned to the shipped proxy authoring model

affects: [frontend-surfaces, service-proxy-delivery, shell-integration, frontend-docs]

tech-stack:
  added: []
  patterns:
    - onRender() pattern for deriving display state from live proxy fields on each rerender
    - pcall(require, ...) guard plus explicit fallback copy for degraded-state surfaces
    - Proxy command methods (audio.volume_up, network.set_wifi_enabled) for backend mutations
    - Shell surface events (mesh.events.publish) remain separate from service commands

key-files:
  created:
    - .planning/phases/02-service-proxy-delivery/02-03-SUMMARY.md
  modified:
    - packages/plugins/frontend/core/panel/src/main.mesh
    - packages/plugins/frontend/core/quick-settings/src/main.mesh
    - packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh
    - packages/plugins/frontend/core/quick-settings/src/components/wifi-section.mesh
    - packages/plugins/frontend/core/quick-settings/src/components/bluetooth-section.mesh
    - crates/core/shell/src/shell/component.rs
    - docs/plugins/frontend/core/README.md

key-decisions:
  - "Bundled surfaces derive all display state in onRender() from live proxy fields; no callbacks."
  - "wifi-section reads network.networks directly (runtime extra) instead of calling wifi_scan()."
  - "bluetooth-section reads network.devices directly and filters to bluetooth kind on rerender."
  - "Integration tests use ScriptContext + apply_service_payload to prove the full field-read path."
  - "Docs document onRender() as the canonical derivation point and pcall+fallback as the degraded pattern."

requirements-completed: [PROXY-01, PROXY-02, PROXY-03, PROXY-04, PROXY-05, PROXY-06, SURF-06]

duration: 5min
completed: 2026-05-02T09:52:30Z
---

# Phase 02 Plan 03: Bundled Surface Migration Summary

**Bundled panel and quick-settings surfaces now read service state through direct proxy field reads on rerender, with no legacy callback/bind APIs, proven by three new shell-facing integration tests and aligned frontend docs.**

## Performance

- **Duration:** ~5 min
- **Started:** 2026-05-02T09:47:27Z
- **Completed:** 2026-05-02T09:52:30Z
- **Tasks:** 3
- **Files modified:** 7

## Accomplishments

### Task 1: Migrate bundled surfaces to direct proxy field reads

All five bundled surface files were rewritten to remove legacy service update APIs:

- **panel/src/main.mesh**: replaced `power.battery()`, `audio.default_output()`, and
  `network.active_connection()` with direct reads from `power.level`, `audio.percent`,
  `audio.muted`, and `network.connections`. Added `onRender()` to derive `batteryText`,
  `volumeIcon`, `volumeLevel`, and `networkStatus` on each rerender.
- **quick-settings/src/main.mesh**: removed `mesh.service.bind("audio.muted", ...)`,
  `mesh.service.bind("audio.percent", ...)`, and `mesh.service.on("audio", ...)`. Now reads
  `audio.percent`, `audio.muted`, and `network.wifi_enabled` directly via `onRender()`.
  `onToggleWiFi` reads `network.wifi_enabled` and calls `network.set_wifi_enabled(not enabled)`.
- **audio-section.mesh**: removed `audio.on_change(...)` callback. Now reads `audio.percent`,
  `audio.muted`, and `audio.source_plugin` directly in `onRender()`. Added pcall guard.
- **wifi-section.mesh**: removed `mesh.service.on("network", ...)` and `wifi_scan()` call.
  Now reads `network.networks` and `network.wifi_enabled` directly on rerender.
- **bluetooth-section.mesh**: removed `mesh.service.on("network", ...)` and `devices()` call.
  Now reads `network.devices` and filters to `kind == "bluetooth"` on rerender.

All five files preserve `pcall(require, ...)` guards and explicit unavailable/fallback copy.

### Task 2: Shell-facing integration tests

Added three new tests to `crates/core/shell/src/shell/component.rs`:

1. `frontend_proxy_update_reaches_panel_or_quick_settings_render_state` — proves that a
   panel-style script reading `audio.percent` and `audio.muted` in `onRender()` reflects
   the correct values after `apply_service_payload("audio", ...)`, without any callback
   registration. Also verifies that both fields appear in `tracked_fields_for_service("audio")`.
2. `frontend_proxy_command_from_bundled_handler_becomes_service_command_request` — proves
   that `onToggleWiFi()` calling `network.set_wifi_enabled(not enabled)` publishes a
   `CoreRequest::ServiceCommand` with `interface = "mesh.network"`,
   `command = "set_wifi_enabled"`, and `enabled = true` through `script_events_to_requests`.
3. `frontend_missing_service_keeps_visible_fallback_copy` — proves that when `pcall(require,
   "@mesh/audio@>=1.0")` fails (empty catalog), `onRender()` still sets `batteryText = "N/A"`,
   `volumeLevel = "0"`, and `volumeIcon = "audio-volume-muted"` rather than blank or nil.

Total: 13 tests pass (10 pre-existing + 3 new).

### Task 3: Frontend docs aligned to shipped proxy model

Rewrote `docs/plugins/frontend/core/README.md`:

- Removed all `mesh.service.bind(...)`, `mesh.service.on(...)`, and `audio:on_change(...)` examples.
- Added a "Reading service state" section with a full `onRender()` example showing pcall guard,
  direct `audio.percent`/`audio.muted` reads, and explicit fallback copy.
- Added a "Issuing backend commands" section with `audio.volume_up()` and
  `network.set_wifi_enabled()` examples and a numbered command lifecycle explanation.
- Added a "Degraded-state pattern" section showing `pcall(require, ...)` with visible
  `wifi_networks = {}` and `batteryText = "N/A"` fallbacks.
- Added a "Shell surface events" section distinguishing `mesh.events.publish(...)` (surface
  toggles) from proxy command methods (backend mutations).

## Task Commits

1. **Task 1: Migrate bundled surfaces to direct proxy field reads** - `76c554b` (feat)
2. **Task 2: Add shell integration tests for bundled proxy semantics** - `32838e4` (test)
3. **Task 3: Rewrite frontend plugin docs** - `f62c80f` (docs)

## Verification

- `cargo test -p mesh-core-shell` — 13 tests pass.
- `rg -n "mesh\.service\.bind|mesh\.service\.on|on_change\(" packages/plugins/frontend/core/panel/src/main.mesh packages/plugins/frontend/core/quick-settings/src/main.mesh packages/plugins/frontend/core/quick-settings/src/components/*.mesh docs/plugins/frontend/core/README.md` — returns empty.
- `rg -n "require\(\"@mesh/|pcall\(require" packages/plugins/frontend/core/panel/src/main.mesh packages/plugins/frontend/core/quick-settings/src/main.mesh packages/plugins/frontend/core/quick-settings/src/components/*.mesh` — shows finalized proxy pattern in all five files.
- `rg -n "mesh\.events\.publish|shell\\.toggle-quick-settings" packages/plugins/frontend/core/panel/src/main.mesh packages/plugins/frontend/core/quick-settings/src/main.mesh` — confirms surface toggles still route through events rather than proxy commands.

## Deviations from Plan

None — plan executed as written. All acceptance criteria passed without modification.

## Known Stubs

None — all proxy field reads correspond to real fields emitted by the backends (audio.percent,
audio.muted, power.level, network.connections, network.wifi_enabled, network.networks,
network.devices are documented state fields in the interface contracts from Plan 02).

## Threat Flags

None — no new network endpoints, auth paths, file access patterns, or schema changes at trust
boundaries were introduced. The bundled surface files are authoring artifacts, not runtime
infrastructure changes.

## Self-Check: PASSED

- Created files: `.planning/phases/02-service-proxy-delivery/02-03-SUMMARY.md`
- Modified code files: all five `.mesh` surface files, `component.rs`, docs `README.md`
- Task commits exist: `76c554b` (Task 1), `32838e4` (Task 2), `f62c80f` (Task 3)

---
*Phase: 02-service-proxy-delivery*
*Completed: 2026-05-02T09:52:30Z*
