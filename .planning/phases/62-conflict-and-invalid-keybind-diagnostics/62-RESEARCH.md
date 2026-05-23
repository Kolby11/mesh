# Phase 62 Research

## Existing Runtime Path

- `resolved_surface_shortcuts` merges manifest actions and legacy settings shortcuts, applies user overrides and locale defaults, then returns runtime shortcuts.
- `dispatch_surface_shortcut` picks the first matching resolved shortcut and calls runtime `keybind` subscribers.
- Phase 60 already lets matched shortcuts with no runtime subscribers fall through as unhandled.
- Phase 61 already prevents user overrides from creating undeclared action ids.

## Existing Diagnostics Path

- `FrontendSurfaceComponent` stores an optional `mesh_core_diagnostics::Diagnostics` handle after mount.
- Runtime diagnostics currently use `diagnostics.degraded(...)` for non-fatal conditions and `diagnostics.error(...)` for runtime failures.
- Keybind diagnostics should be degraded because malformed or conflicting shortcuts should not crash the surface.

## Implementation Notes

- The resolution path can diagnose unresolved override ids before filtering declarations.
- Declaration order should be stable before duplicate detection and dispatch matching.
- Unsafe override rejection belongs in resolution so annotation and dispatch share the same effective shortcut list.
