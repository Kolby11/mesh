# Phase 63 Research

## Existing Accessibility Path

`annotate_surface_shortcuts` resolves focused-surface keybinds and writes formatted shortcuts to `WidgetNode.accessibility.keyboard_shortcut` for nodes with matching `keybind` attributes.

## Existing Debug Path

`Shell::debug_snapshot` builds `mesh_core_debug::DebugSnapshot` and serializes it through `debug_service_payload`. Profiling surfaces are already exposed under `profiling.surfaces`; a top-level keybind list can make resolved shortcut metadata available without changing the profiling accumulator.

## Existing Docs

- `docs/module-system.md` documents `mesh.keybinds`.
- `docs/settings/README.md` documents `keyboard.surface_shortcuts`.
- `docs/modules/frontend/core/navigation-bar/README.md` documents the shipped navigation keybind proof.
