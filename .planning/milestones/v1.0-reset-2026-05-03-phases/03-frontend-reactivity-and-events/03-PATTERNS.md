---
phase: 03
slug: frontend-reactivity-and-events
status: complete
created: 2026-05-02
---

# Phase 03 — Pattern Map

## Files To Modify

| Target | Role | Closest Existing Analog | Pattern To Reuse |
|--------|------|-------------------------|------------------|
| `crates/core/runtime/scripting/src/context.rs` | Runtime state sync and handler invocation | Existing `ScriptState::set()`, `sync_state_from_lua()`, `call_handler()`, and context tests | Keep Lua-to-JSON conversion centralized; add behavior tests at bottom of the file near existing handler/reactivity tests |
| `crates/core/shell/src/shell/component.rs` | Shell input routing, component dirty flag, diagnostics bridge, integration tests | Existing `handle_input()`, `update_slider_from_position()`, `call_namespaced_handler()`, service proxy tests | Keep input routing in `handle_input()`; keep service command translation through `script_events_to_requests()`; add tests in same module |
| `crates/core/shell/src/shell/layout.rs` | Event-handler lookup helpers | Existing `find_click_handler()`, `find_node_by_key()`, `is_slider_key()` | Generalize `find_click_handler()` into event-name lookup instead of adding four duplicated finders |
| `crates/core/foundation/diagnostics/src/lib.rs` | Visible diagnostics collector | Existing `DiagnosticsCollector::register()`, `Diagnostics::error()` | Extend collector minimally for handler errors and dedupe rather than inventing a second diagnostics store |
| `crates/core/ui/render/src/render.rs` | Event-name normalization and focusability | Existing `normalize_event_handler_name()` and `accessibility_for_tag()` | Event names are already normalized; only add assertions if needed |
| `packages/plugins/frontend/core/navigation-bar/src/components/volume-button.mesh` | End-to-end proof component | Existing volume button, audio `pcall(require, ...)`, tooltip/icon derivation | Add inline slider in this component or a sibling imported component; preserve existing button click behavior |
| `packages/plugins/backend/core/audio-interface/interface.toml` | Audio command contract reference | Existing `set_volume(device_id, volume)` method | Use command payload shape already declared by the interface; do not introduce a new Rust-specific shortcut |

## Code Excerpts

### Runtime Dirty Pattern

`ScriptState::set()` is already value-aware:

```rust
if self.variables.get(&name) == Some(&value) {
    return;
}
self.variables.insert(name, value);
self.dirty = true;
```

Use this as the base, but make shallow table comparison explicit so Phase 3 decisions are testable rather than accidental.

### Handler Dispatch Pattern

`call_namespaced_handler()` is the right shell boundary:

```rust
runtime
    .script_ctx
    .call_handler(&handler_name, args)
    .map_err(|source| ComponentError::Script { component_id, source })?;
self.dirty = true;
```

Replace the unconditional dirty assignment with state-driven dirty propagation and diagnostics handling.

### Event Lookup Pattern

`find_click_handler()` is specific but the tree stores generic normalized handlers:

```rust
find_node_by_key(tree, key)
    .and_then(|node| node.event_handlers.get("click"))
    .cloned()
```

Generalize this to `find_event_handler(tree, key, "change")` / `"release"` / `"focus"`.

### Existing Slider Pattern

`update_slider_from_position()` already computes a typed numeric value from pointer position and stores it in `slider_values`. Use that value for `on_change(value)` and `on_release(value)`.

## Data Flow

1. User input enters `FrontendSurfaceComponent::handle_input()`.
2. The shell finds a target node in the cached or freshly built `WidgetNode` tree.
3. The shell looks up the normalized handler name from `WidgetNode.event_handlers`.
4. The shell builds a typed argument: click event object, slider number, switch/checkbox boolean, or input string.
5. `call_namespaced_handler()` calls the correct root or embedded component runtime.
6. `ScriptContext::call_handler()` syncs Lua globals back into `ScriptState`.
7. Shell dirty state follows `ScriptState::is_dirty()` or explicit render-affecting shell state changes.
8. Next paint rebuilds the widget tree if dirty.

## PATTERN MAPPING COMPLETE
