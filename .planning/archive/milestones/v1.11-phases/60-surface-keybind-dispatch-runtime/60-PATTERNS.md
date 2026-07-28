# Phase 60: Surface Keybind Dispatch Runtime - Patterns

## Closest Existing Analogs

| Target Work | Existing Analog | Pattern To Reuse |
|-------------|-----------------|------------------|
| Component key event ordering | `crates/core/shell/src/shell/component/input/mod.rs` | Keep special keyboard ownership checks explicit and early, then delegate to small helper methods. |
| Surface keybind dispatch | `crates/core/shell/src/shell/component/input/keyboard.rs` | Resolve shortcut, find runtime subscribers, build event JSON, call namespaced handlers, aggregate requests. |
| Manifest-first keybind resolution | `crates/core/shell/src/shell/component/input/keyboard.rs` | Manifest declarations are primary; legacy settings declarations append only for missing ids. |
| Keyboard regression tests | `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` | Use small synthetic components for focused behavior and real navigation-bar fixtures for shipped proof. |
| Manifest model | `crates/core/extension/module/src/manifest/model.rs` | Keep trigger/action semantics in canonical manifest structs; do not invent a second declaration model. |

## File Roles

- `crates/core/shell/src/shell/component/input/mod.rs`: top-level `ComponentInput::KeyPressed` precedence and focused text/default handling.
- `crates/core/shell/src/shell/component/input/keyboard.rs`: shortcut resolution, modifier matching, subscriber dispatch, and accessibility formatting.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs`: keybind behavior tests and shipped navigation proof.
- `modules/frontend/navigation-bar/module.json`: real manifest-owned `mesh.keybinds.mute` declaration.
- `modules/frontend/navigation-bar/src/components/volume-button.mesh`: real `keybind`/`onkeybind` subscriber.

## Data Flow

Keyboard event -> shell-global shortcut gate in shell runtime -> focused surface component input -> Tab/Escape and Ctrl+C ownership checks -> surface shortcut resolution -> runtime tree keybind subscribers -> Luau handler via existing component handler path -> `CoreRequest`s.

## Landmines

- Do not let bare printable keybinds preempt focused text input.
- Do not move dispatch into a new global keybind registry.
- Do not dispatch directly from manifest action ids to handler names; markup subscribers own targets.
- Do not remove legacy settings fallback behavior.
- Do not include duplicate-binding or missing-target diagnostics in Phase 60; those are Phase 62.
- Do not modify the pre-existing dirty audio popover files as part of this plan.
