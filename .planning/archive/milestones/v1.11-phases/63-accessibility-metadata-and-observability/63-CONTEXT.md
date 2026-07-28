# Phase 63: Accessibility Metadata And Observability - Context

**Gathered:** 2026-05-23
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 63 surfaces already-resolved focused-surface keybind metadata to accessibility and debug consumers, then documents the author-facing keybind contract. It does not add a remapping UI or compositor-global shortcuts.

</domain>

<decisions>
## Implementation Decisions

### Accessibility
- Reuse the existing `AccessibilityInfo.keyboard_shortcut` annotation path for target controls.
- Preserve Phase 60/61 formatting behavior for resolved shortcuts and access keys.

### Debug Observability
- Add a structured keybind metadata list to the existing debug snapshot/service payload.
- Include surface id, module id, action id, resolved key, modifiers, trigger kind, source, and accessibility label.
- Continue exposing diagnostics through existing component health/diagnostics channels.

### Docs
- Update author docs where keybind declarations and settings overrides are already documented.
- Keep focused-surface scope explicit and keep compositor-global shortcuts out of the contract.
</decisions>

<code_context>
## Relevant Code

- `crates/core/shell/src/shell/component/input/keyboard.rs` already annotates accessibility metadata via `annotate_surface_shortcuts`.
- `crates/core/foundation/debug/src/lib.rs` defines `DebugSnapshot` and profiling/debug payload structs.
- `crates/core/shell/src/shell/runtime/debug.rs` builds the service payload consumed by the debug inspector.
- `docs/module-system.md`, `docs/settings/README.md`, and `docs/modules/frontend/core/navigation-bar/README.md` already describe keybind declarations and overrides.
</code_context>

<specifics>
## Phase Requirements

- KACC-01: target controls expose resolved shortcut/access-key metadata through existing accessibility annotations.
- KACC-02: debug/profiling payloads can show resolved keybind metadata and diagnostics for focused surfaces.
- KACC-03: author docs explain declaration, localized triggers, overrides, diagnostics, accessibility metadata, and focused-surface scope.
</specifics>

<deferred>
None.
</deferred>
