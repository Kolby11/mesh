---
phase: 02-service-proxy-delivery
verified: 2026-05-02T11:00:00Z
status: human_needed
score: 5/5 must-haves verified
overrides_applied: 0
human_verification:
  - test: "Confirm REQUIREMENTS.md traceability table marks PROXY-03 and SURF-06 as Complete (not Pending)"
    expected: "Checkboxes and traceability rows for PROXY-03 and SURF-06 show complete status matching the implementation evidence"
    why_human: "REQUIREMENTS.md still shows both as Pending/unchecked despite code satisfying them. Automated verification cannot determine whether this is intentional deferral or a missed update."
  - test: "Confirm volume-slider/src/main.mesh and volume-bar/src/main.mesh calling legacy audio.on_change(...) and mesh.service.bind(...) is intentionally deferred to a later phase"
    expected: "These files are known to be out of Phase 2 scope and will be updated in Phase 3 or Phase 4"
    why_human: "These files call proxy APIs that no longer exist in the runtime. They will fail at runtime. Phase 2 scope only targeted panel and quick-settings, so this may be intentional, but needs confirmation that Phase 3/4 will address it."
---

# Phase 02: Service Proxy Delivery Verification Report

**Phase Goal:** Deliver a stable, documented service-proxy runtime so plugin authors can build backends and frontends against a known API without needing shell internals access. Lock down the proxy contract, validate it end-to-end from backend emission through the runtime proxy and into the frontend render/command loop, and ship production-usable documentation.
**Verified:** 2026-05-02T11:00:00Z
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths (ROADMAP Success Criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | A frontend `.mesh` script receives live state from a backend service proxy | VERIFIED | `create_service_proxy` in context.rs reads from `__mesh_svc_{service}` globals via proxy `__index`; `require("@mesh/<service>")` resolves to a live proxy table; test `interface_proxy_reads_latest_emitted_fields_after_repeated_updates` passes |
| 2 | Backend service emissions mark consuming frontend components dirty so rerender sees the latest proxy state without requiring service-specific callback APIs | VERIFIED | `handle_service_event` in component.rs compares tracked top-level field values using `tracked_service_fields_changed`; only marks dirty when a tracked field changed; test `service_update_marks_component_dirty_only_when_tracked_fields_change` passes |
| 3 | Service proxies stay a read-and-command surface; element event handlers such as `onclick` and `onchange` remain attached to template elements rather than service proxies | VERIFIED | `create_service_proxy.__index` has no `on_change`, `bind`, or subscription case; only contract `[[methods]]` become callable Lua functions; all other reads fall through to `__mesh_svc_{service}` state table |
| 4 | Service command methods declared by contracts are callable through the proxy | VERIFIED | `create_service_proxy` iterates `contract.methods` and registers Lua functions that publish `{interface}.{method}` events; `script_events_to_requests` in service.rs maps these to `CoreRequest::ServiceCommand`; tests prove `set_volume` and `set_wifi_enabled` round-trip |
| 5 | Missing or invalid service contracts produce visible diagnostics | VERIFIED | `record_lookup_diagnostic` records `ScriptDiagnostic` (plugin_id, interface, requested_version, reason) before returning `InterfaceUnavailable` or `CapabilityDenied`; test `pcall_require_still_emits_interface_diagnostic` proves `pcall(require, ...)` catches Lua error without suppressing the diagnostic side effect |

**Score:** 5/5 truths verified

### Plan-Level Must-Haves

#### Plan 01 Must-Haves

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| D-02–D-05 | Legacy callback/bind paths removed; tracked deps refreshed on rerender; invalidation only on value change | VERIFIED | `on_change`, `bind` absent from `create_service_proxy.__index` grep; `clear_tracked_service_fields` called on each render pass (`context.rs:269`); `tracked_service_fields_changed` compares values before dirty mark |
| D-06–D-09 | Frontend writes through named proxy command methods; unsupported commands fail with Lua error | VERIFIED | `create_service_proxy` only synthesizes Lua functions for contract methods; non-method non-field lookups return `nil` from `__mesh_svc_` table; `ScriptError::UnsupportedOperation` is the error variant |
| D-11–D-13 | `pcall(require, ...)` does not suppress diagnostics; failures reported with plugin, interface, reason | VERIFIED | `record_lookup_diagnostic_lua` records to diagnostics store before returning `mlua::Error::external(err)`; test `pcall_require_still_emits_interface_diagnostic` at context.rs:1187 proves this |
| Service proxies are read-and-command only | VERIFIED | See SC#3 above |
| Backend emissions only invalidate components whose tracked fields changed | VERIFIED | See SC#2 above |
| Missing/invalid require lookups emit visible diagnostics even under pcall | VERIFIED | See SC#5 above |
| Proxy state reads come from latest `__mesh_svc_<service>` payload | VERIFIED | `apply_service_payload` at context.rs:293 writes to Lua globals; proxy `__index` at context.rs:886 reads from `__mesh_svc_{service}` |

#### Plan 02 Must-Haves

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| D-14–D-18: real interface packages, formal provider declarations | VERIFIED | All four interface plugins have `interface.toml` with `[[state_fields]]` and `[[methods]]`; all five providers have `base_plugin` field in plugin.json |
| D-19–D-22: portable core contract plus additive extras | VERIFIED | `InterfaceContract.state_fields` stores documented fields; `[[methods]]` only has mutating commands; NetworkManager emits `wifi_enabled`, `networks`, `source_plugin` in poll path |
| Each service keeps a real interface package with formal provider declarations | VERIFIED | audio-interface, network-interface, power-interface, media-interface all exist with `interface.toml`; pipewire-audio, pulseaudio-audio, networkmanager-network, upower-power, mpris-media all declare `base_plugin` |
| Proxy methods are commands only; read access from emitted state fields | VERIFIED | `default_output`, `connections`, `devices`, `active_connection`, `wifi_scan`, `players`, `active_player`, `battery`, `profile` are absent from all four interface.toml `[[methods]]` sections |
| Runtime, backend docs, editor-facing API all describe same model | VERIFIED | `mesh_api.rs` describes state-field reads plus mutating command methods; `docs/plugins/backend/core/README.md` has base interface plugin model section |

#### Plan 03 Must-Haves

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| D-23–D-25: bundled surfaces migrated, legacy APIs removed, dominant-provider path functional | VERIFIED | panel, quick-settings/main.mesh, audio-section, wifi-section, bluetooth-section all use `pcall(require, ...)` + `onRender()` pattern; grep finds no `mesh.service.bind(`, `mesh.service.on(`, or `on_change(` in these five files |
| Bundled surfaces use same public read-fields / command-methods API | VERIFIED | All five files use `require("@mesh/<service>")`, read fields directly, call named command methods |
| Bundled surfaces no longer use legacy callback/bind APIs | VERIFIED | Confirmed no `mesh.service.bind`, `mesh.service.on`, or `on_change` in the five targeted files |
| Advanced dominant-provider state enriches surfaces, base path functional | VERIFIED | wifi-section reads `network.networks` (runtime extra); audio-section reads `audio.source_plugin` (runtime extra) |
| Degraded states remain visible when pcall fails | VERIFIED | All five files set explicit fallback copy in the pcall-failed branch; test `frontend_missing_service_keeps_visible_fallback_copy` proves non-blank fallback |

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/core/runtime/scripting/src/context.rs` | Proxy lookup diagnostics, field-read tracking, command-only proxy behavior | VERIFIED | Contains `create_service_proxy`, `record_lookup_diagnostic`, `tracked_service_fields`, 1513 lines, substantive |
| `crates/core/shell/src/shell/component.rs` | Service update comparison and dirty-state invalidation | VERIFIED | Contains `handle_service_event` with `tracked_service_fields_changed` value comparison, 2299 lines |
| `crates/core/shell/src/shell/service.rs` | Proxy command channel to CoreRequest translation | VERIFIED | Contains `script_events_to_requests` mapping `mesh.audio.set_volume` and `mesh.network.set_wifi_enabled` to `CoreRequest::ServiceCommand` |
| `crates/core/extension/plugin/src/manifest.rs` | Manifest fields for interface inheritance and base_plugin declarations | VERIFIED | Contains `ProvidedInterface.base_plugin: Option<String>` and `InterfaceSection.extends: Option<String>` |
| `crates/core/extension/service/src/contract.rs` | Parsed core state-field and command metadata | VERIFIED | Contains `ContractStateField` struct and `[[state_fields]]` TOML parser; test at line 313 proves round-trip |
| `packages/plugins/backend/core/network-interface/interface.toml` | Network core state fields and mutating commands | VERIFIED | Has `[[state_fields]]` for available, wifi_enabled, connections, devices, networks, source_plugin; has `[[methods]]` for connect, disconnect, set_wifi_enabled |
| `packages/plugins/backend/core/networkmanager-network/src/main.luau` | Runtime-authoritative dominant-provider network extras | VERIFIED | Emits `wifi_enabled`, `networks`, `source_plugin` in normal poll path at lines 119-125 |
| `packages/plugins/frontend/core/panel/src/main.mesh` | Read-first panel using direct proxy field reads | VERIFIED | Contains `pcall(require, "@mesh/network@>=1.0")`, `onRender()`, reads `audio.percent`, `network.connections` directly |
| `packages/plugins/frontend/core/quick-settings/src/main.mesh` | Quick-settings driven by proxy fields and commands | VERIFIED | Contains `onToggleWiFi`, reads `network.wifi_enabled`, calls `network.set_wifi_enabled(not enabled)` |
| `docs/plugins/frontend/core/README.md` | Frontend guidance for stabilized proxy contract | VERIFIED | Contains `require("@mesh/<service>")`, `onRender()`, `pcall(require, ...)` pattern, no legacy bind/on examples |
| `crates/core/shell/src/shell/component.rs` (integration tests) | Shell-facing integration coverage | VERIFIED | Three new tests: `frontend_proxy_update_reaches_panel_or_quick_settings_render_state`, `frontend_proxy_command_from_bundled_handler_becomes_service_command_request`, `frontend_missing_service_keeps_visible_fallback_copy` |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `context.rs` | `component.rs` | Tracked top-level service-field dependencies decide dirty marking | VERIFIED | `tracked_service_fields_changed` called inside `handle_service_event` using fields from `script_ctx.tracked_fields_for_service()` |
| `context.rs` | `service.rs` | Named proxy command methods publish `mesh.<service>.<command>` channels | VERIFIED | `create_service_proxy` pushes `PublishedEvent { channel: format!("{}.{}", iface, method.name) }`; `script_events_to_requests` maps these to `CoreRequest::ServiceCommand` |
| `manifest.rs` | `packages/plugins/backend/core/*/plugin.json` | Base interface plugin declarations | VERIFIED | All five provider plugin.json files contain `"base_plugin"` field; manifest test at line 965 proves round-trip |
| `interface.toml` files | `context.rs` | Only mutating command methods callable from proxy | VERIFIED | `create_service_proxy` only registers Lua functions for `contract.methods`; state_fields are never callable |
| `networkmanager-network/src/main.luau` | frontend surfaces | Runtime-emitted network state replaces read-style proxy helpers | VERIFIED | `wifi_enabled`, `networks`, `source_plugin` emitted in poll; wifi-section.mesh reads `network.networks` directly |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|--------------------|--------|
| `context.rs` `create_service_proxy.__index` | `key` from `__mesh_svc_{service}` | `apply_service_payload` writes backend emission to Lua globals | Yes — direct Lua globals read | FLOWING |
| `component.rs` `handle_service_event` | `tracked_fields` from `tracked_fields_for_service` | `ScriptContext.tracked_service_fields` (Arc<Mutex>) | Yes — live field set populated by proxy reads | FLOWING |
| `service.rs` `script_events_to_requests` | `PublishedEvent.channel` from proxy command calls | `create_service_proxy` pushes to `published_events` | Yes — command channel string is real method name | FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Contract state field test suite | `cargo test -p mesh-core-service` | 11 tests (per SUMMARY) | SKIP — can't run without nix develop in this env |
| Scripting proxy tests | `cargo test -p mesh-core-scripting context` | 15 tests (per SUMMARY) | SKIP — build env dependency |
| Shell component tests | `cargo test -p mesh-core-shell` | 13 tests (per SUMMARY) | SKIP — build env dependency |
| Grep: `record_lookup_diagnostic` exists in context.rs | Grep verified | Present at line 743 | PASS |
| Grep: `handle_service_event` uses tracked field comparison | Grep verified | `tracked_service_fields_changed` at line 1377 | PASS |
| Grep: no `on_change`/`bind` in five migrated surfaces | Grep verified | Zero matches in panel and quick-settings components | PASS |
| Grep: `wifi_enabled`, `networks`, `source_plugin` emitted in networkmanager | Grep verified | Lines 121, 124, 125 in main.luau | PASS |
| Grep: all four interface.toml have `[[state_fields]]` and no removed helpers | Grep verified | `default_output`, `active_connection`, `players`, `wifi_scan` absent from all `[[methods]]` | PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| PROXY-01 | 02-01, 02-03 | `require('@mesh/<service>')` receives a proxy table | SATISFIED | `require` function in context.rs resolves interface via catalog and returns `create_interface_proxy` result |
| PROXY-02 | 02-01, 02-03 | Proxy tables expose latest backend-emitted state fields | SATISFIED | Proxy `__index` reads from `__mesh_svc_{service}` which `apply_service_payload` keeps current |
| PROXY-03 | 02-02, 02-03 | Proxy tables expose command methods from service contract | SATISFIED | `create_service_proxy` registers Lua functions for `contract.methods`; `set_volume`, `toggle_mute`, `set_wifi_enabled` all present in interface.toml and callable; **NOTE: REQUIREMENTS.md traceability row still shows Pending — documentation gap only** |
| PROXY-04 | 02-01, 02-03 | Backend emissions invalidate consuming components; rerender sees latest proxy state without callbacks | SATISFIED | `handle_service_event` + `tracked_service_fields_changed` drives dirty marking; test proven |
| PROXY-05 | 02-01, 02-03 | Service updates separate from element events; latest proxy state on rerender without `on_<service>_update()` | SATISFIED | No `on_change` or callback-style subscription on proxy; field-level tracking drives invalidation |
| PROXY-06 | 02-01, 02-03 | Frontend scripts fail visibly with diagnostics when requiring missing/invalid service | SATISFIED | `record_lookup_diagnostic` + `ScriptDiagnostic` proven; pcall test proven |
| SURF-06 | 02-02 | Audio, network, power, and media service contracts document state fields and commands | SATISFIED | All four interface.toml have `[[state_fields]]` section; `[[methods]]` contain only mutating commands; backend README documents the model; **NOTE: REQUIREMENTS.md traceability row still shows Pending — documentation gap only** |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `packages/plugins/frontend/core/volume-slider/src/main.mesh` | 33 | `audio.on_change(function()...)` — calls proxy method that no longer exists | WARNING | Out-of-scope for Phase 2; `on_change` was removed from proxy `__index` in Phase 01. At runtime `audio.on_change` returns `nil` and calling it would error. Volume slider is broken at runtime until Phase 3/4 migration. |
| `packages/plugins/frontend/core/volume-bar/src/main.mesh` | 21-23 | `mesh.service.bind(...)`, `mesh.service.on(...)` — legacy APIs removed from runtime | WARNING | Same as above — these calls will silently fail or error since `mesh.service.bind` and `mesh.service.on` no longer exist in the runtime. |
| `packages/plugins/frontend/core/navigation-bar/src/components/battery-button.mesh` | 19-27 | Multiple `mesh.service.bind(...)` and `mesh.service.on(...)` calls | WARNING | Same as above — navigation-bar battery-button uses removed APIs. |
| `packages/plugins/frontend/core/navigation-bar/src/components/volume-button.mesh` | 16-18 | `mesh.service.bind(...)`, `mesh.service.on(...)` | WARNING | Same as above. |
| `.planning/REQUIREMENTS.md` | 21, 41, 97, 111 | PROXY-03 and SURF-06 still marked `[ ]` Pending in traceability table | WARNING | Documentation inconsistency — implementation exists and satisfies both requirements, but REQUIREMENTS.md was not updated after plan completion. |
| `.planning/ROADMAP.md` | Phase 2 Progress field | "Progress: 1/3 plans complete" — not updated after phase 03 completion | INFO | ROADMAP was not updated to reflect final completion of all three plans. |

### Human Verification Required

#### 1. REQUIREMENTS.md Traceability Update

**Test:** Open `.planning/REQUIREMENTS.md` and check whether PROXY-03 and SURF-06 should be marked as complete.
**Expected:** Both should show `[x]` in their checkbox and "Complete" in the traceability table, matching the implementation evidence in the codebase.
**Why human:** Can't determine policy: the REQUIREMENTS.md might be intentionally updated only at milestone end, or this might be a missed step. The code satisfies both requirements.

#### 2. Out-of-Scope Legacy API Users

**Test:** Review `volume-slider/src/main.mesh`, `volume-bar/src/main.mesh`, `navigation-bar/src/components/battery-button.mesh`, and `navigation-bar/src/components/volume-button.mesh`. Confirm these are deliberately left unmigrared for Phase 3 or 4.
**Expected:** There should be a documented decision that these four files are NOT Phase 2 scope and will be migrated in a later phase (Phase 3 or Phase 4). Otherwise, these files represent broken bundled plugins that call removed APIs.
**Why human:** The Phase 2 PLAN.md only listed five specific files for migration. These four files were not listed. But the phase goal claims to deliver a stable proxy runtime — if bundled plugins call APIs that no longer exist, the shell has broken surfaces. Only a human can decide whether this is acceptable for the current phase completion gate.

### Gaps Summary

No hard blockers were found in the five specifically targeted implementation areas. All five ROADMAP success criteria are verifiably met by the codebase.

Two items require human judgment:
1. REQUIREMENTS.md and ROADMAP.md were not updated to reflect completion — this may be intentional workflow or a missed step.
2. Four bundled frontend plugins outside Phase 2 scope (`volume-slider`, `volume-bar`, `navigation-bar/battery-button`, `navigation-bar/volume-button`) still use legacy APIs that were removed in this phase. These will fail at runtime until migrated. Confirmation that they are intentionally deferred is needed before marking the phase complete.

---

_Verified: 2026-05-02T11:00:00Z_
_Verifier: Claude (gsd-verifier)_
