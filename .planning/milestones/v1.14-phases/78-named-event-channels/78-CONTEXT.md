---
phase: 78
phase_name: named-event-channels
status: complete
created: 2026-05-26
requirements:
  - LUAEVT-01
  - LUAEVT-02
  - LUAEVT-03
  - LUAEVT-04
  - LUAEVT-05
  - LUAEVT-06
---

# Phase 78: Named Event Channels - Context

## Goal

Replace new author-facing string event APIs with named channel objects on service proxies and `self`, while preserving current compatibility paths.

## Existing Paths

- Frontend service proxies already expose legacy `proxy.events.Name:subscribe(fn)` and host-delivered interface events through `emit_interface_event`.
- Frontend component scripts already expose compatibility `module.events.Name:subscribe(fn)` and `:emit(payload)`.
- Backend providers already publish typed interface events through `mesh.service.emit_event(name, payload)`.
- Phase 74 established lifecycle `self` for frontend and backend runtimes.

## Design Notes

- Direct service events should appear as named channels, for example `audio.VolumeChanged:on(fn)`, without removing `audio.events.VolumeChanged:subscribe(fn)`.
- Local frontend component events should appear on `self` as `self.Changed:on(fn)` and `self.Changed:fire(payload)`.
- Backend provider events should use `self.EventName:fire(payload)` and feed the same typed event queue as `mesh.service.emit_event(...)`.
- Channel objects should keep legacy `subscribe`/`emit` names and add canonical `on`/`fire` aliases.
- Event names are limited to PascalCase-like identifiers to avoid accidental collision with fields such as `meta`.
