# Phase 72: Runtime Text Resolution - Context

**Gathered:** 2026-05-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Resolve preserved `LocalizedText` metadata at shell runtime boundaries that expose user-facing manifest text: script-facing `this.keybinds` descriptors and debug keybind metadata. Resolution must use active locale, fallback locale, then the required fallback string, and must produce diagnostics when a translation key is missing.

</domain>

<decisions>
## Implementation Decisions

### Runtime Descriptor Resolution
- `this.keybinds.<id>.label`, `description`, and `category` should expose resolved user-facing strings rather than raw translation keys or serialized `LocalizedText` objects.
- Source metadata should remain available through additive descriptor fields such as `label_key`, `description_key`, and fallback/source fields so diagnostics and future tooling can inspect origin.
- Missing translation keys must fall back to `LocalizedText::fallback_text()` without failing component creation.
- Phase 72 should not migrate bundled manifests; Phase 73 owns changing shipped `module.json` text values.

### Debug Metadata
- Debug keybind entries should include resolved label, description, and category text where manifest metadata exists.
- Debug keybind entries should include source translation keys where available so diagnostics and tooling can explain the resolved value.
- Existing shortcut, trigger, source, and accessibility shortcut behavior must remain unchanged.

### Diagnostics
- Missing translation diagnostics should be non-fatal and include module id, field path, translation key, and fallback text.
- Diagnostics should be emitted only for `LocalizedText::Translation` values whose key is absent from the loaded locale fallback chain.
- Raw literal `LocalizedText::Literal` values should not produce missing-key diagnostics.

### the agent's Discretion
The agent may choose the smallest helper/API shape that keeps resolution reusable across descriptors and debug metadata, provided it does not perform manifest migration or alter keybind trigger resolution.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `mesh_core_locale::LocaleEngine::translate()` already walks the active/fallback chain.
- `mesh_core_module::LocalizedText` exposes `fallback_text()` and `translation_key()`.
- `FrontendSurfaceComponent` owns both `self.locale` and diagnostics handles.

### Established Patterns
- Component diagnostics use degraded non-fatal messages for keybind/runtime metadata issues.
- Script descriptor data is assembled in `crates/core/shell/src/shell/component/runtime.rs`.
- Debug keybind metadata is assembled in `crates/core/shell/src/shell/component/input/keyboard.rs` and serialized in `crates/core/shell/src/shell/runtime/debug.rs`.

### Integration Points
- `module_descriptor_from_manifest` currently serializes `LocalizedText` into `this.keybinds`.
- `DebugKeybindEntry` currently contains shortcut metadata but no user-facing label fields.
- Navigation tests already cover descriptor and debug keybind behavior and should be extended.

</code_context>

<specifics>
## Specific Ideas

No extra user-specific requirements. Keep changes narrow and test runtime metadata resolution without changing shipped manifests yet.

</specifics>

<deferred>
## Deferred Ideas

- Bundled `module.json` migration and author docs are Phase 73 scope.
- Cross-module language-pack namespace syntax remains future scope.

</deferred>
