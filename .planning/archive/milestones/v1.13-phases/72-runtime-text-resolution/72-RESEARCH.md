---
phase: 72
name: Runtime Text Resolution
status: complete
created: 2026-05-24
---

# Phase 72 Research: Runtime Text Resolution

## Question

Where should shell runtime metadata resolve `LocalizedText`, and how can it preserve fallback behavior and diagnostics?

## Findings

### Script Descriptor Boundary

Frontend scripts read manifest metadata through the `this` global assembled in
`crates/core/shell/src/shell/component/runtime.rs`.

`module_descriptor_from_manifest()` currently serializes `LocalizedText` values
directly. Phase 72 should convert those values into resolved strings while
retaining additive metadata fields for translation keys and fallbacks.

### Debug Keybind Boundary

`crates/core/shell/src/shell/component/input/keyboard.rs` builds
`DebugKeybindEntry` values from resolved shortcuts. It has access to the
component manifest and locale engine through `FrontendSurfaceComponent`, so it
can attach resolved user-facing keybind text alongside existing shortcut data.

### Locale Resolution

`mesh_core_locale::LocaleEngine::translate()` already walks the active locale
and fallback chain. Resolution rule:

1. For `LocalizedText::Literal`, return the literal text.
2. For `LocalizedText::Translation`, call `LocaleEngine::translate(key)`.
3. If no translation exists, record a non-fatal diagnostic and return fallback.

### Diagnostic Shape

The component diagnostic sink can record degraded messages. A practical message
should include:

- module id
- field path, such as `mesh.keybinds.mute.label`
- missing key
- fallback text

## Validation Architecture

Required tests:

- script descriptor exposes resolved text and key/fallback metadata
- missing translation key falls back and records a diagnostic
- debug keybind metadata includes resolved label text and source key
- shell debug JSON includes the new text/key fields

Suggested commands:

```bash
cargo test -p mesh-core-shell manifest_descriptor_resolves_keybind_localized_text -- --nocapture
cargo test -p mesh-core-shell manifest_descriptor_missing_translation_uses_fallback_and_diagnostic -- --nocapture
cargo test -p mesh-core-shell keybind_debug_metadata_includes_resolved_manifest_text -- --nocapture
cargo check -p mesh-core-shell
```

## Plan Boundary

Do in Phase 72:

- add runtime localized-text resolver helpers in shell component code
- resolve keybind descriptor fields exposed through `this.keybinds`
- add resolved text/source fields to debug keybind metadata and JSON
- add focused tests and diagnostics proof

Defer:

- bundled manifest migration
- author documentation
- new settings UI/keybind UI surfaces
