---
phase: 71
name: Contribution Propagation
status: complete
created: 2026-05-24
---

# Phase 71 Research: Contribution Propagation

## Question

What must change so localized manifest text metadata survives installed-graph
contribution indexing without breaking string fallback consumers?

## Findings

### Current Flattening Point

`crates/core/extension/module/src/package/installed_graph.rs` builds
`ModuleContributionIndex` from canonical `ModuleManifest` values.

Keybind actions currently flatten Phase 70 `LocalizedText` values immediately:

```rust
label: action.label.as_ref().map(|text| text.fallback_text().to_string())
```

That satisfies Phase 70 compatibility, but it erases the translation key before
Phase 72 can resolve against the active locale. Phase 71 should move the rich
metadata into `ContributedKeybindAction` while keeping deterministic fallback
helpers for existing consumers.

### Layout Labels

Canonical layout contributions are modeled in
`crates/core/extension/module/src/package/module_manifest.rs` as:

```rust
pub struct LayoutContribution {
    pub id: String,
    pub entrypoint: String,
    pub label: Option<String>,
}
```

Phase 71 should migrate this field to `Option<manifest::LocalizedText>` using
the same raw-string compatibility behavior Phase 70 already established. The
graph record `ContributedLayout` should retain the rich value and expose a
fallback accessor.

Legacy manifest conversion in `ModuleManifest::from_legacy_manifest()` should
wrap old package names in `LocalizedText::Literal` so old manifests still load.

### Settings Schema Descriptions

Settings contributions currently store arbitrary JSON schema as
`serde_json::Value`:

```rust
pub struct SettingsContribution {
    pub namespace: String,
    pub schema: serde_json::Value,
}
```

Because this field is untyped JSON, object-shaped descriptions such as
`{ "t": "...", "fallback": "..." }` already survive graph propagation if the
schema is cloned unchanged. Phase 71 should add explicit regression coverage
for that behavior instead of redesigning the settings schema model.

### Compatibility Strategy

Recommended pattern:

- Keep rich fields named `label`, `description`, and `category` where possible
  so graph records expose source metadata.
- Add fallback helpers such as `label_text()`, `description_text()`,
  `category_text()`, and `ContributedLayout::label_text()` returning
  `Option<&str>`.
- Update tests and any code that directly expects `Option<String>` to use the
  helpers.
- Do not perform locale lookup in Phase 71.

## Validation Architecture

Focused module crate tests are sufficient.

Required coverage:

- `ContributedKeybindAction.label`, `description`, and `category` preserve
  `LocalizedText::Translation` values.
- Keybind fallback helpers return the translation fallback text for compatibility.
- `LayoutContribution.label` parses both raw strings and localized objects.
- `ContributedLayout.label` preserves localized text and its fallback helper
  returns deterministic fallback text.
- Settings schema `description` JSON objects shaped like localized text survive
  graph indexing unchanged.

Suggested command:

```bash
cargo test -p mesh-core-module contribution_index_preserves -- --nocapture
```

Full package safety command:

```bash
cargo test -p mesh-core-module package -- --nocapture
```

## Plan Boundary

Do in Phase 71:

- migrate installed graph contribution records for keybind and layout text to
  preserve `LocalizedText`
- preserve compatibility through fallback helper methods
- add regression tests for keybind, layout, and settings schema propagation

Defer:

- resolving localized graph text with active locale
- shell runtime metadata display changes
- bundled manifest migration and author docs
