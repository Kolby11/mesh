# Phase 3: Frontend Reactivity and Events - Context

**Gathered:** 2026-05-02
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase makes frontend script reactivity and element event handlers predictable and reliable: global assignments mark dirty correctly (change-based, not unconditional), `on_click`/`on_change`/`on_release`/`on_focus` handlers fire with typed arguments on all interactive elements, handler failures are visible through logs and diagnostics without crashing the surface, and the end-to-end event → state → render pipeline is proven in the existing navigation-bar plugin with an inline volume slider.

</domain>

<decisions>
## Implementation Decisions

### Dirty Marking Semantics
- **D-01:** Dirty marking is change-based — `ScriptState.dirty` is set only when a synced global's value actually changed compared to its previous value. Handlers that run without changing any global do not trigger a rebuild.
- **D-02:** Table-typed globals use shallow key-value comparison: a table is dirty if any top-level entry was added, removed, or changed to a different primitive value. Deep nesting is not compared.
- **D-03:** The `__mesh_request_redraw = true` escape hatch is kept. Scripts that need to force a rebuild without changing any global can set this flag; it unconditionally marks dirty and clears after sync.

### Event Contract
- **D-04:** `on_change` fires continuously on every drag event for sliders — no built-in throttle. Scripts that need debouncing implement it themselves.
- **D-05:** `on_change` passes a typed value directly as the first argument: slider → `number` (0.0–1.0), toggle/checkbox → `boolean`, text input → `string`. No event table wrapper.
- **D-06:** The existing bespoke audio slider handling in `component.rs` (`active_slider_key`, `last_audio_slider_percent`) is kept as-is. The generic `on_change` contract is added alongside it, not as a replacement.
- **D-07:** Four events are standardized: `on_click`, `on_change`, `on_release`, `on_focus`. All four are added to the core element event dispatch path in this phase.
- **D-08:** The full event set (`on_click`, `on_change`, `on_release`, `on_focus`) is supported on interactive elements: `button`, `input`, `slider`, `switch`, `checkbox`. Non-interactive layout elements (`box`, `row`, `column`, `text`, `icon`, `separator`) support `on_click` only.

### Handler Failure Visibility
- **D-09:** Handler errors are reported to both `tracing::warn!` (log) and `DiagnosticsCollector` (visible in the debug overlay). They do not propagate as fatal errors.
- **D-10:** A handler failure does not interrupt rendering. The component continues displaying the last successfully rendered frame while the error is reported.
- **D-11:** Repeated identical errors (same handler name + same error message) are deduplicated in `DiagnosticsCollector` to avoid flooding the overlay during continuous events like slider drag.

### End-to-End Proof Component
- **D-12:** The proof is implemented inside the existing navigation-bar plugin — add an inline volume slider (in `volume-button.mesh` or a new sibling component) that uses `on_change(value)` to call `audio.set_volume()`. This exercises `on_change` with a typed numeric value, reactive global updates (icon, tooltip), and a real service command in a single, observable path.
- **D-13:** No new plugin or separate test fixture is needed. The navigation-bar is the validation vehicle for Phase 3 reactivity.

### Claude's Discretion
- The planner may choose the exact Rust implementation for shallow table comparison (e.g., serde_json::Value comparison vs. a custom diff function).
- The planner may decide how `on_release` and `on_focus` events are routed through `handle_input` in `component.rs` — the contract is standardized, the wiring is implementation detail.
- The planner may decide the exact DiagnosticsCollector deduplication strategy (ring buffer, HashMap keyed on handler+message, or last-error-only per component).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning Scope
- `.planning/PROJECT.md` — milestone goal, external-developer target, and the core value that plugins work reliably end-to-end.
- `.planning/REQUIREMENTS.md` — Phase 3 requirement IDs `FRONT-01` through `FRONT-05`.
- `.planning/ROADMAP.md` — Phase 3 goal, success criteria, and dependency on Phase 2.
- `.planning/STATE.md` — current project position.

### Prior Phase Decisions (carry-forward)
- `.planning/phases/02-service-proxy-delivery/02-CONTEXT.md` — service proxy model (read-and-command only, no `proxy.on_change`), field-level dependency tracking, and rerender invalidation path. Phase 3 dirty marking extends this path for handler-triggered globals.
- `.planning/phases/01-backend-host-api-contract/01-CONTEXT.md` — backend Luau API shape; Rust core is wiring only; service logic stays in Luau.

### Codebase Maps
- `.planning/codebase/ARCHITECTURE.md` — service/backend event flow, `FrontendSurfaceComponent` paint path, `ScriptState.dirty` and the `call_handler` → `sync_state_from_lua` pipeline.
- `.planning/codebase/STACK.md` — Rust/Luau/mlua/Tokio constraints.

### Scripting Runtime (dirty marking and handler dispatch)
- `crates/core/runtime/scripting/src/context.rs` — `ScriptContext`, `ScriptState`, `sync_state_from_lua()`, `call_handler()`, `__mesh_request_redraw` flag. Phase 3 changes dirty marking logic here.
- `crates/core/shell/src/shell/component.rs` — `FrontendSurfaceComponent`, `call_namespaced_handler()`, `handle_input()`, bespoke slider state (`active_slider_key`, `last_audio_slider_percent`). Phase 3 adds generic event dispatch and failure reporting here.

### Diagnostics
- `crates/core/foundation/diagnostics/src/lib.rs` — `DiagnosticsCollector` — Phase 3 routes handler failures here.
- `crates/core/shell/src/shell/types.rs` — `ComponentError` enum — source of handler error types that need to reach diagnostics.

### Proof Component (navigation-bar)
- `packages/plugins/frontend/core/navigation-bar/src/main.mesh` — root component.
- `packages/plugins/frontend/core/navigation-bar/src/components/volume-button.mesh` — existing `on_click` handler and audio reactive globals. Phase 3 adds an inline slider here or in a sibling component.
- `packages/plugins/frontend/core/navigation-bar/plugin.json` — surface manifest; already declares `service.audio.control` capability.

### Interface Contracts (for audio.set_volume command)
- `packages/plugins/backend/core/audio-interface/interface.toml` — audio command/field contract; `set_volume` command shape.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ScriptContext::sync_state_from_lua()` in `crates/core/runtime/scripting/src/context.rs` — current unconditional dirty marking; Phase 3 changes this to change-based. Existing tests at bottom of file can be extended for new behavior.
- `call_namespaced_handler()` in `crates/core/shell/src/shell/component.rs` — existing handler dispatch that sets `self.dirty = true` unconditionally after every call; Phase 3 lets `sync_state_from_lua` own dirty decisions and removes the unconditional override here.
- `DiagnosticsCollector` in `crates/core/foundation/diagnostics/src/lib.rs` — already used in the shell (`self.diagnostics`); Phase 3 routes `ComponentError::Script` into it.
- `VolumeButton.mesh` — has `onVolumeClick`, audio proxy reads, and reactive globals; natural home for the inline slider proof.

### Established Patterns
- Handler dispatch: `handle_input` → `find_click_handler` → `call_namespaced_handler` → `call_handler` → `sync_state_from_lua` → ScriptState updated → `self.dirty = true` → next paint rebuilds tree.
- Reactive globals: any non-`__`, non-`local`, non-function global in `<script>` is synced to `ScriptState` after each handler call and exposed to the template via `{variable_name}`.
- Plugin isolation: one plugin's handler error does not crash others; the shell logs and continues.

### Integration Points
- `sync_state_from_lua()` is the single location where global sync and dirty marking happen. Change-based dirty goes here.
- `call_namespaced_handler()` is where `ComponentError::Script` is created — this is where diagnostics recording should be added.
- `handle_input()` needs to route `on_release` and `on_focus` events in addition to the existing `on_click` path.
- The audio slider in navigation-bar connects `on_change(value)` → `audio.set_volume(math.floor(value * 100))` → service command → backend update → rerender.

</code_context>

<specifics>
## Specific Ideas

- Inline volume slider added to `volume-button.mesh` (or a new `volume-slider-inline.mesh` sibling component imported there) — the slider should show the current audio percent as its initial value and call `audio.set_volume()` on each drag event.
- The `on_change` handler in the slider proof can be as simple as: `function onVolumeChange(value) audio.set_volume(math.floor(value * 100)) end` — proves the full chain without ceremony.
- Deduplication for diagnostics: simplest viable approach is a `HashSet<(handler_name, error_message)>` cleared on successful render or component reload. The planner can choose a more sophisticated ring-buffer approach if needed.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 03-frontend-reactivity-and-events*
*Context gathered: 2026-05-02*
