# Phase 62: Conflict And Invalid-Keybind Diagnostics - Context

**Gathered:** 2026-05-23
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 62 adds non-fatal, actionable diagnostics to the focused-surface keybind runtime. It does not add compositor-global shortcuts, a remapping UI, or new keybind declaration schema.

</domain>

<decisions>
## Implementation Decisions

### Diagnostics Path
- Use the existing component `Diagnostics` handle and degraded health for keybind declaration and override issues.
- Diagnostic messages must include module id, surface id, action id, and a concrete reason.
- Keep malformed runtime keybind data non-fatal; invalid actions resolve to no shortcut or fall back to the next safe default.

### Determinism
- Manifest and legacy declarations should resolve in stable action-id order before duplicate checks and dispatch matching.
- Duplicate effective bindings should be diagnosed while preserving a deterministic first-match dispatch order.

### Safety
- User overrides remain override-only and cannot create undeclared actions.
- Unsafe overrides for shell-owned traversal, close/cancel, activation, and copy chords are ignored with diagnostics.
- Focused text input ownership remains enforced by the existing input dispatch guard.
</decisions>

<code_context>
## Relevant Code

- `crates/core/shell/src/shell/component/input/keyboard.rs` owns focused-surface keybind declaration resolution, dispatch, subscriber lookup, and accessibility annotation.
- `crates/core/shell/src/shell/component/diagnostics.rs` already records component diagnostics for missing icons and focused-renderer proof issues.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` contains the Phase 60/61 keybind dispatch and resolution tests to extend.
</code_context>

<specifics>
## Phase Requirements

- KDIAG-01: malformed declarations diagnose module id, surface id, action id, and reason.
- KDIAG-02: duplicate effective bindings diagnose without ambiguous dispatch order.
- KDIAG-03: missing targets, unsupported trigger forms, and unresolved overrides diagnose instead of disappearing silently.
- KDIAG-04: unsafe overrides are ignored with diagnostics.
</specifics>

<deferred>
None.
</deferred>
