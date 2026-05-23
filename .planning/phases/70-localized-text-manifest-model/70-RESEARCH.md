---
phase: 70
name: Localized Text Manifest Model
status: complete
created: 2026-05-24
---

# Phase 70 Research: Localized Text Manifest Model

## Question

What needs to be true in the module manifest layer before later phases can
preserve and resolve localized manifest text?

## Findings

### Current Manifest Shape

The normalized manifest model lives in
`crates/core/extension/module/src/manifest/model.rs`.

Relevant current types:

- `KeybindAction.label: Option<String>`
- `KeybindAction.description: Option<String>`
- `KeybindAction.category: Option<String>`
- `LayoutContribution.label: Option<String>` in
  `crates/core/extension/module/src/package/module_manifest.rs`
- `ContributedKeybindAction.label/description/category: Option<String>` in
  `crates/core/extension/module/src/package/installed_graph.rs`

Phase 70 should only establish the reusable manifest text schema and diagnostics
foundation. Phase 71 can propagate the richer type through installed graph
records; Phase 72 can resolve it against locale catalogs.

### Canonical Loader Path

Canonical `module.json` parsing goes through
`crate::package::ModuleManifest::from_path()` and then into
`ModuleManifest::validate()`. The package loader returns
`LoadedModuleManifest { diagnostics: Vec<ModuleManifestDiagnostic> }`, so it
already has a place for non-fatal manifest warnings.

The legacy runtime manifest loader in `manifest/load.rs` returns only
`LoadedManifest` without diagnostics. Phase 70 can still make legacy raw strings
load as literals, but the actionable diagnostic proof should target the
canonical package loader because that is the public author-facing manifest path.

### Accepted Shape

Spike 004 selects this authoring shape:

```json
{ "t": "keybind.mute.label", "fallback": "Mute" }
```

Recommended Rust model:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum LocalizedText {
    Literal(String),
    Translation { key: String, fallback: String },
}
```

Use custom `Deserialize` so raw JSON strings map to `Literal`, and objects with
`t` plus `fallback` map to `Translation`. Reject empty `t` or `fallback` in
normal manifest validation so malformed localized declarations fail early.

### Diagnostic Rule

Raw strings must remain valid literals for backwards compatibility. A raw string
that looks like a translation key should warn, not fail:

```text
mesh.keybinds.mute.label looks like an i18n key. Use
{ "t": "keybind.mute.label", "fallback": "Mute" } to localize this field.
```

A practical first heuristic is:

- value contains at least one `.`
- value has no whitespace
- value is not obviously prose
- field is one of `mesh.keybinds.<action>.label`,
  `description`, or `category`

This is enough to catch the current shipped `keybind.mute.label` shape without
diagnosing normal literal text such as `Navigation Bar`.

## Validation Architecture

Phase 70 can be verified with focused module crate tests.

Quick command:

```bash
cargo test -p mesh-core-module manifest_localized_text -- --nocapture
```

Full phase command:

```bash
cargo test -p mesh-core-module manifest -- --nocapture
```

Required automated coverage:

- raw string deserializes as `LocalizedText::Literal`
- `{ "t": "...", "fallback": "..." }` deserializes as
  `LocalizedText::Translation`
- empty `t` and empty `fallback` are rejected by manifest validation
- existing raw-string keybind display metadata still loads
- canonical package loader returns a warning diagnostic for raw dotted keybind
  text and the suggested action includes `{ "t": "...", "fallback": "..." }`

## Plan Boundary

Do in Phase 70:

- add the reusable localized-text model
- update keybind user-facing text fields to parse it
- provide fallback/literal helpers so current string consumers can keep working
- add manifest/package loader diagnostics for suspicious raw dotted keys
- add focused manifest tests

Defer to later phases:

- installed graph field type migration beyond compatibility fallback helpers
- shell locale catalog resolution
- bundled manifest migration
- author documentation updates
