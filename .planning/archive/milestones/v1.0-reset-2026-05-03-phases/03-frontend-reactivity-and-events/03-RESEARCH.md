---
phase: 03
slug: frontend-reactivity-and-events
status: complete
created: 2026-05-02
---

# Phase 03 — Research

## Question

What do we need to know to plan predictable frontend script reactivity and element events?

## Current Architecture Findings

### Script Runtime

- `crates/core/runtime/scripting/src/context.rs` owns `ScriptContext`, `ScriptState`, `call_handler()`, `sync_state_from_lua()`, service proxies, published events, and script lookup diagnostics.
- `ScriptState::set()` is already value-based for scalar JSON values: if the stored value equals the new value, it returns without setting `dirty`.
- `sync_state_from_lua()` currently iterates non-builtin, non-`__`, non-function Lua globals and calls `ScriptState::set()` for each. It also honors `__mesh_request_redraw` by setting `self.state.dirty = true` and clearing the Lua global.
- `call_handler()` already accepts `serde_json::Value` arguments and converts them to Lua values. A test proves an `on_click(event)` handler receives nested event payload data.
- Table comparison is currently whatever `serde_json::Value` equality produces after Lua-to-JSON conversion. The locked phase decision wants shallow top-level table comparison and no deep diff semantics.

### Shell Event Flow

- `crates/core/shell/src/shell/component.rs` owns `FrontendSurfaceComponent::handle_input()`.
- The shell currently routes click handlers only through `find_click_handler()` and `call_namespaced_handler()`.
- `call_namespaced_handler()` calls the target runtime's `script_ctx.call_handler()` and then unconditionally sets `self.dirty = true`. Phase 3 needs this to become state-driven: handler calls with no changed reactive global should not force rebuilds unless `__mesh_request_redraw` is set.
- Slider handling is split between generic local slider state and bespoke `mesh-action="audio-volume"` service command emission through `active_slider_key` and `last_audio_slider_percent`. The context explicitly keeps this existing bespoke path and adds generic `on_change` alongside it.
- The render layer already normalizes attributes like `onclick` to `click`, and `WidgetNode.event_handlers` can hold arbitrary normalized event names. The missing piece is shell-side lookup and routing for `change`, `release`, and `focus`.

### Diagnostics

- `mesh_core_diagnostics::DiagnosticsCollector` currently aggregates `Diagnostics` handles with health and error counts. It does not yet provide a first-class deduplicated handler-error API.
- `FrontendSurfaceComponent` already has access to shell diagnostics through `ComponentContext` during mount and stores runtime-level script diagnostics in `ScriptContext`.
- Phase 3 should record handler failures close to `call_namespaced_handler()` because that function has the component id, handler name, and `ComponentError::Script` source.

### Proof Component

- `packages/plugins/frontend/core/navigation-bar/src/components/volume-button.mesh` already reads audio state through `require("@mesh/audio@>=1.0")`, derives `icon_name` and `audio_tooltip`, and handles `onVolumeClick(event)`.
- `packages/plugins/backend/core/audio-interface/interface.toml` declares `set_volume(device_id, volume)` with `volume` as float. Existing runtime tests use `audio:set_volume("default", 0.5)`, while shell service routing also tolerates published payloads such as `{ "percent": 55 }`.
- The plan should not invent a new plugin. The navigation bar is the proof vehicle.

## Recommended Plan Shape

1. Runtime reactivity first: make dirty semantics explicit, test change/no-change/table/redraw behavior, and provide a way for the shell to observe whether a handler dirtied script state.
2. Shell event dispatch second: add generic event lookup, typed `on_change`, `on_release`, and `on_focus` routing, diagnostics reporting, and tests.
3. Proof component last: add the inline navigation-bar volume slider and an end-to-end test proving event -> handler -> reactive state/service command -> next paint.

## Validation Architecture

| Area | Automated Signal | Command |
|------|------------------|---------|
| Runtime dirty semantics | Unit tests in `mesh-core-scripting` for global assignment, shallow table comparison, and redraw escape hatch | `cargo test -p mesh-core-scripting context` |
| Shell event routing | Unit/integration tests in `mesh-core-shell` for click/change/release/focus dispatch and diagnostics | `cargo test -p mesh-core-shell` |
| Render event names | Existing render tests plus any new parse assertions for `onchange`, `onrelease`, and `onfocus` | `cargo test -p mesh-core-render` |
| Proof component | Grep checks plus shell test proving slider handler publishes `mesh.audio.set_volume` or changes reactive state | `cargo test -p mesh-core-shell` |

## Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| Removing unconditional `self.dirty = true` after handler calls could suppress valid rebuilds | High | Runtime plan must expose/consume `script_ctx.state().is_dirty()` after handler sync and keep `__mesh_request_redraw` as explicit override |
| Slider bespoke audio path and generic `on_change` both fire duplicate commands | Medium | Event plan must define ordering and allow one service command per value change; proof component should use generic handler while preserving existing `mesh-action` compatibility only where intentionally left |
| Diagnostics flood during slider drag | Medium | Deduplicate by component id + handler name + error message |
| Lua table equality gets mistaken for deep diff semantics | Medium | Implement or isolate shallow top-level comparison and tests that nested-only changes do not count as changed unless the top-level value differs under the chosen representation |

## Planning Inputs

- Phase requirements: `FRONT-01`, `FRONT-02`, `FRONT-03`, `FRONT-04`, `FRONT-05`
- Locked decisions: `.planning/phases/03-frontend-reactivity-and-events/03-CONTEXT.md`
- UI contract: `.planning/phases/03-frontend-reactivity-and-events/03-UI-SPEC.md`
- Prior dependency: `.planning/phases/02-service-proxy-delivery/02-CONTEXT.md`

## RESEARCH COMPLETE
