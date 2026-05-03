---
phase: 02-service-proxy-delivery
plan: 02
subsystem: contracts
tags: [rust, luau, service-contracts, interface-model, backend-docs]

requires:
  - phase: 02-service-proxy-delivery
    plan: 01
    provides: reactive proxy runtime with field-level tracking and lookup diagnostics

provides:
  - Formal base interface plugin model with provider inheritance declarations
  - Command-only proxy method surface (no read-style helper methods)
  - Documented core state fields in [[state_fields]] contract TOML sections
  - NetworkManager backend emitting wifi_enabled, networks, source_plugin
  - Aligned LSP and backend docs describing base-core plus runtime-extras model

affects: [service-proxy-delivery, interface-contracts, backend-plugins, lsp-knowledge]

tech-stack:
  added: []
  patterns:
    - Interface contracts now separate [[state_fields]] (read) from [[methods]] (mutating commands)
    - ContractStateField struct captures documented portable baseline for each interface
    - Provider manifests formally declare base_plugin in their provides block
    - Dominant providers emit additive runtime extras alongside base contract fields

key-files:
  created:
    - .planning/phases/02-service-proxy-delivery/02-02-SUMMARY.md
  modified:
    - crates/core/extension/service/src/contract.rs
    - crates/core/extension/service/src/interface.rs
    - crates/core/runtime/scripting/src/context.rs
    - packages/plugins/backend/core/audio-interface/interface.toml
    - packages/plugins/backend/core/network-interface/interface.toml
    - packages/plugins/backend/core/power-interface/interface.toml
    - packages/plugins/backend/core/media-interface/interface.toml
    - packages/plugins/backend/core/networkmanager-network/src/main.luau
    - crates/tools/lsp/src/knowledge/mesh_api.rs
    - docs/plugins/backend/core/README.md

key-decisions:
  - "State fields are documented in [[state_fields]] contract TOML sections; they are never callable methods — only field access on the proxy."
  - "[[methods]] in interface contracts contain only mutating commands; read-helpers like default_output, connections, active_player were removed."
  - "Runtime-defined extras (richer provider fields beyond the base contract) are additive and emitted alongside base fields; NetworkManager emits networks, wifi_enabled, source_plugin."
  - "Legacy service.bind and service.on removed from LSP API knowledge to match Phase 01 runtime changes."

patterns-established:
  - "Interface TOML structure: [[state_fields]] for documented reads, [[methods]] for mutating commands only."
  - "ContractStateField: parsed name, field_type, optional description from interface TOML."
  - "Dominant provider pattern: emit all base contract fields plus additive extras that bundled UIs rely on."

requirements-completed: [PROXY-03, SURF-06]

duration: 23min
completed: 2026-05-02T09:43:48Z
---

# Phase 02 Plan 02: Interface Contract and Provider Alignment Summary

**Base interface plugin model is formalized: contracts separate readable state fields from mutating command methods, providers formally declare inheritance, and the dominant network backend emits the full state bundled UIs need.**

## Performance

- **Duration:** ~23 min
- **Started:** 2026-05-02T09:20:43Z
- **Completed:** 2026-05-02T09:43:48Z
- **Tasks:** 3
- **Files modified:** 10

## Accomplishments

### Task 1: Formal base-interface plugin and provider inheritance metadata (pre-committed)

The prior wave commit `64409e0` established formal base-interface plugin metadata:
- `ProvidedInterface.base_plugin: Option<String>` in `manifest.rs` for provider declarations
- `InterfaceSection.extends: Option<String>` in `manifest.rs` for interface inheritance
- All five provider manifests (pipewire-audio, pulseaudio-audio, networkmanager, upower, mpris) declare their `base_plugin`
- `InterfaceRegistry` tests confirm `base_plugin` metadata survives catalog construction

### Task 2: Reshape service contracts to commands-only proxy methods

- Added `ContractStateField` struct to `InterfaceContract` in `contract.rs`
- Added `[[state_fields]]` TOML section parser to `ContractToml`
- Restructured all four core interface contracts:
  - **audio-interface**: removed `output_devices`, `input_devices`, `streams`, `default_output` as methods; documented `available`, `percent`, `muted`, `source_plugin` as state fields; kept `set_volume`, `set_muted`, `volume_up`, `volume_down`, `toggle_mute` as commands
  - **network-interface**: removed `connections`, `devices`, `active_connection`, `wifi_scan` as methods; documented `available`, `wifi_enabled`, `connections`, `devices`, `networks`, `source_plugin` as state fields; kept `connect`, `disconnect`, `set_wifi_enabled` as commands
  - **power-interface**: removed `battery`, `profile` as methods; documented `available`, `level`, `charging`, `time_remaining_minutes`, `time_to_full_minutes`, `source_plugin` as state fields; kept `set_profile` as command
  - **media-interface**: removed `players`, `active_player` as methods; documented `available`, `title`, `artist`, `album`, `state`, `source_plugin` as state fields; kept `play`, `pause`, `next`, `previous` as commands
- Updated test fixtures in `interface.rs` and `context.rs` to remove read-helper methods

### Task 3: Align dominant provider and docs with base-core plus runtime-extras model

- Updated `networkmanager-network/src/main.luau` to emit `wifi_enabled`, `networks`, and `source_plugin` on every normal poll cycle (base contract fields plus runtime-defined extras)
- Added `fetch_wifi_enabled()` function using `nmcli radio wifi`
- Added `last_networks` cache so scan results persist across poll cycles
- Updated `crates/tools/lsp/src/knowledge/mesh_api.rs`:
  - Removed legacy `service.bind` and `service.on` entries (removed in Phase 01 runtime)
  - Rewrote `service.use` description to document the state-field reads + mutating commands model
  - Added inline comments explaining base interface contract and runtime-defined extras
- Updated `docs/plugins/backend/core/README.md`:
  - Added "Base interface plugin model" section documenting `[[state_fields]]` vs `[[methods]]`
  - Added "State fields vs command methods" section with code examples
  - Added "Runtime-defined extras" section documenting the dominant provider pattern
  - Updated interface and backend tables to include base plugin column
  - Updated provider list to explicitly name base plugins

## Task Commits

1. **Task 1: Add base interface manifest metadata** - `64409e0` (pre-committed by prior wave)
2. **Task 2: Reshape service contracts to commands-only proxy methods** - `b26aaaf` (feat)
3. **Task 3: Align dominant provider and docs with base-core plus runtime-extras** - `d57177e` (feat)

## Verification

- `cargo test -p mesh-core-service` — passed, 11 tests (up from 10; new state_fields test)
- `cargo test -p mesh-core-scripting context` — passed, 15 tests
- `rg -n "extends|base_plugin" manifest.rs */plugin.json` — shows formal base-interface metadata in all providers
- `rg -n "^name = \"(default_output|active_connection|players|wifi_scan)\"" *-interface/interface.toml` — returns empty (helpers removed from methods)
- `rg -n "wifi_enabled|networks|source_plugin" networkmanager-network/src/main.luau` — shows all three fields emitted during normal state path
- `rg -n "base interface|runtime-defined extras|state fields|command methods" mesh_api.rs README.md` — shows aligned wording

## Deviations from Plan

None - plan executed as written. The Task 1 acceptance criteria were fully satisfied by the prior-wave commit `64409e0`.

## Known Stubs

None - all base state fields documented in contracts correspond to real emitted fields from the providers.

## Threat Flags

None - no new network endpoints, auth paths, file access patterns, or schema changes at trust boundaries were introduced.

## Self-Check: PASSED

- Created files: `.planning/phases/02-service-proxy-delivery/02-02-SUMMARY.md`
- Modified code files: `contract.rs`, `interface.rs`, `context.rs`, all 4 `interface.toml` files, `main.luau`, `mesh_api.rs`, `README.md`
- Task commits exist: `64409e0` (Task 1, prior wave), `b26aaaf` (Task 2), `d57177e` (Task 3)
