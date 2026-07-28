# Phase 3: Frontend Reactivity and Events - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-02
**Phase:** 03-frontend-reactivity-and-events
**Areas discussed:** Dirty marking semantics, on_change value contract, Handler failure visibility, End-to-end proof component

---

## Dirty Marking Semantics

| Option | Description | Selected |
|--------|-------------|----------|
| Always after any handler | Any handler call always marks dirty regardless of whether globals changed | |
| Only when a value actually changes | Compare old vs. new value before marking dirty | ✓ |

**User's choice:** Change-based dirty marking

**Notes:** Follow-up questions established that table comparison is shallow key-value (not reference identity or JSON serialization), and the `__mesh_request_redraw` escape hatch is kept for scripts that need to force a rebuild without changing globals.

---

## on_change Value Contract

| Option | Description | Selected |
|--------|-------------|----------|
| Typed value directly | Native value for the element type: slider → number, toggle → boolean, string → string | ✓ (first Q) |
| Event table | Table: {value, type, key} | |
| No argument | Handler reads proxy state directly | |

| Option | Description | Selected |
|--------|-------------|----------|
| Release only | on_change fires once on pointer up | |
| Continuous drag | on_change fires on every pointer move | ✓ |
| Both — on_input + on_change | Separate events for drag vs. release | |

| Option | Description | Selected |
|--------|-------------|----------|
| No throttle — script decides | Fire every drag event; script debounces if needed | ✓ |
| Built-in throttle at ~16ms | Runtime caps at ~60fps | |
| Configurable per-element | throttle-ms attribute | |

| Option | Description | Selected |
|--------|-------------|----------|
| Replace bespoke slider with generic on_change | Remove active_slider_key / last_audio_slider_percent | |
| Keep bespoke, add generic alongside | Keep existing audio slider path; add on_change | ✓ (with addition) |

**User's choice:** Keep existing bespoke audio slider; add generic on_change alongside. Also add on_click, on_release, on_focus as standard events on all basic elements.

**Notes:** User specified that on_click, on_release, on_focus should be added universally across basic elements. Full event set applies to interactive elements (button, input, slider, switch, checkbox); non-interactive layout elements get on_click only.

---

## Handler Failure Visibility

| Option | Description | Selected |
|--------|-------------|----------|
| Log + DiagnosticsCollector | tracing::warn! + debug overlay record | ✓ |
| Log only | Current behavior — tracing::warn! only | |
| Log + DiagnosticsCollector + visual error state | All of above plus visible component error banner | |

| Option | Description | Selected |
|--------|-------------|----------|
| Last-good frame persists | Component continues showing last rendered frame | ✓ |
| Surface goes dark on error | Component clears frame until reloaded | |

| Option | Description | Selected |
|--------|-------------|----------|
| Deduplicate same handler+message | Skip repeated identical errors in DiagnosticsCollector | ✓ |
| Report every occurrence | Record every failure regardless | |

**User's choice:** Log + DiagnosticsCollector, last-good frame persists, deduplicate identical errors.

---

## End-to-End Proof Component

| Option | Description | Selected |
|--------|-------------|----------|
| New plugin under packages/plugins/ | New runnable base-surface plugin | |
| Test fixture only | .mesh file exercised by integration tests | |
| Unit tests in scripting crate | Extend #[cfg(test)] in context.rs | |
| Implement in navigation-bar | Use the existing navigation-bar plugin | ✓ |

**User's choice:** Implement proof in the existing navigation-bar plugin — add an inline volume slider that uses on_change to call audio.set_volume().

**Notes:** User chose to add a volume slider inline (in volume-button.mesh or a new sibling component) as the proof. This exercises on_change with a typed numeric value, reactive globals, and a real service command in a single observable path.

---

## Claude's Discretion

- Exact Rust implementation of shallow table comparison in sync_state_from_lua
- Wiring of on_release and on_focus through handle_input in component.rs
- DiagnosticsCollector deduplication strategy (HashSet, ring buffer, or last-error-only per component)
- Whether to add the slider directly in volume-button.mesh or extract to a new volume-slider-inline.mesh component

## Deferred Ideas

None — discussion stayed within phase scope.
