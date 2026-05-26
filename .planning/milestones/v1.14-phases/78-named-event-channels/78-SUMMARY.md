---
phase: 78
phase_name: named-event-channels
status: complete
completed: 2026-05-26
requirements:
  - LUAEVT-01
  - LUAEVT-02
  - LUAEVT-03
  - LUAEVT-04
  - LUAEVT-05
  - LUAEVT-06
---

# Phase 78: Named Event Channels - Summary

## Delivered

- Declared interface events are available directly on service proxies as named channels, for example `audio.VolumeChanged:on(fn)`.
- Event channels now support canonical `:on(fn)` and `:fire(payload)` aliases while preserving compatibility `:subscribe(fn)` and `:emit(payload)`.
- Frontend component `self` exposes local named event channels such as `self.Changed:on(fn)` and `self.Changed:fire(payload)`.
- Backend provider `self` exposes named event channels whose `:fire(payload)` path publishes typed backend interface events.
- Legacy `proxy.events.Name`, `module.events.Name`, and `mesh.service.emit_event(...)` paths remain working.

## Verification

Passed:

```bash
nix develop -c cargo test -p mesh-core-scripting
nix develop -c cargo fmt --check
```
