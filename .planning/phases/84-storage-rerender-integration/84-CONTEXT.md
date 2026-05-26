# Phase 84 Context: Storage Rerender Integration

**Milestone:** v1.15 Persistent Storage System
**Date:** 2026-05-26
**Status:** Complete

## Goal

Track frontend render-time `self.storage` reads and reuse the existing script
state invalidation path when a watched storage key changes.

## Relevant Existing Context

- Service proxy reads already track field dependencies and shell service updates
  invalidate affected runtimes.
- Phase 82 exposed `self.storage`.
- Phase 83 added dirty storage writes and lifecycle persistence.
- Phase 84 should avoid adding a separate frontend invalidation mechanism.

## Implementation Notes

- `ScriptContext` tracks render-time storage key reads separately from storage
  writes.
- `call_render_lifecycle` clears and enables storage read tracking only while
  `render` or `onRender` is running.
- `self.storage` reads call a storage read sink; writes call a storage write
  sink.
- After handler calls, changed storage keys are compared to tracked render keys.
- Matching writes mark `ScriptState` dirty, which feeds the existing shell
  rebuild path.
- Unwatched writes do not mark script state dirty.
- Existing `mesh.ui.request_redraw()` remains the explicit escape hatch.
