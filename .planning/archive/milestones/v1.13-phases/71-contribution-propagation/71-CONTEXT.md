# Phase 71: Contribution Propagation - Context

**Gathered:** 2026-05-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Preserve Phase 70 `LocalizedText` metadata through installed-graph contribution records and compatibility consumers. This phase moves rich localized text beyond manifest parsing into the graph records that later shell/runtime code consumes, while keeping deterministic fallback strings available for existing callers.

</domain>

<decisions>
## Implementation Decisions

### Propagation Boundary
- `ContributedKeybindAction` should retain rich localized metadata for label, description, and category rather than flattening those fields at graph construction time.
- Compatibility consumers that still need strings should receive deterministic fallback text through explicit helper methods or compatibility fields, not by erasing the source metadata.
- Installed-graph tests should prove both raw literal strings and `{ "t": "...", "fallback": "..." }` declarations survive contribution propagation.
- Phase 71 should avoid locale resolution; active-locale lookup and catalog fallback remain Phase 72 scope.

### Contribution Coverage
- Keybind contribution labels, descriptions, and categories are mandatory Phase 71 coverage because Phase 70 already migrated those manifest fields.
- Layout labels should preserve localized metadata where the package graph currently exposes layout contribution text.
- Settings schema descriptions should preserve localized text only where the current manifest model has a localized-capable field; do not invent a broad settings schema redesign in this phase.
- Interface/provider/resource contribution records are out of scope unless they already carry user-facing localized-capable text.

### Compatibility And Diagnostics
- Existing callers that compare or display `Option<String>` contribution text should remain source-compatible where practical by using fallback text accessors or additive fields.
- Raw dotted i18n-key warnings remain non-fatal diagnostics from Phase 70 and should not become hard validation failures during graph propagation.
- Serialization/debug output for graph records should remain deterministic so tests and diagnostics do not depend on active locale.
- New graph APIs should prefer additive fields and helpers over destructive renames when that reduces churn for shell/runtime consumers.

### the agent's Discretion
Implementation details are left to the agent where they preserve the phase boundary: choose the least invasive graph representation that keeps rich `LocalizedText` available and keeps fallback consumers deterministic.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/extension/module/src/manifest/model.rs` now exposes `LocalizedText` with `fallback_text()`, `translation_key()`, validation, and suspicious-key helpers.
- `crates/core/extension/module/src/package/installed_graph.rs` is the graph indexing and contribution propagation path called out by Phase 70 as the next boundary.
- Phase 70 tests in `crates/core/extension/module/src/manifest/tests.rs` already cover literal and translated keybind display parsing.

### Established Patterns
- Module graph changes are verified with focused `mesh-core-module` unit tests.
- Compatibility is normally preserved through explicit fallback conversion rather than removing old consumers in the same phase.
- Loader diagnostics stay non-fatal for author migration hints.

### Integration Points
- `InstalledModuleGraph::from_parts()` indexes installed modules and contribution records.
- Keybind contribution records feed shell/runtime metadata and existing tests in `crates/core/shell/src/shell/component/tests/interaction/navigation.rs`.
- Phase 72 will consume the preserved metadata to resolve localized manifest text against the active shell locale.

</code_context>

<specifics>
## Specific Ideas

No extra user-specific requirements. Follow the ROADMAP success criteria and Phase 70 summary: preserve rich localized metadata in graph records, keep fallback compatibility, and do not resolve locale in this phase.

</specifics>

<deferred>
## Deferred Ideas

- Active-locale text resolution and runtime fallback diagnostics are Phase 72 scope.
- Bundled manifest migration, author docs, and shipped proof are Phase 73 scope.

</deferred>
