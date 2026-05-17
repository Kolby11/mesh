# Phase 38: Canonical Manifest Normalization - Research

## Research Complete

Phase 38 is a runtime normalization phase, not a new feature surface. The
existing Rust code already contains most of the target schema under old
`package` names. Planning should preserve that code path, rename it toward
module vocabulary, then tighten loader behavior and diagnostics.

## Existing Implementation

- `crates/core/extension/module/src/package/module_manifest.rs` contains the
  target canonical schema shape: top-level `name`, `version`, optional metadata,
  and `mesh` with `apiVersion`, `kind`, capabilities, dependencies,
  entrypoints, `implements`, `provides`, keybinds, resources, and
  contributions.
- `ModulePackageManifest::into_runtime_manifest()` maps canonical manifest data
  to the normalized runtime `Manifest`.
- `ModulePackageManifest::from_legacy_manifest()` maps old runtime manifests
  into the newer module schema and preserves legacy `provides`, keybinds,
  settings, themes, icon packs, i18n, dependencies, and entrypoints.
- `crates/core/extension/module/src/package/root.rs` already parses the root
  installed graph either through a `mesh` wrapper or direct root graph fields.
  This can become the canonical root module graph manifest.
- `crates/core/extension/module/src/package/installed_graph.rs` currently
  prefers `package.json` over `module.json` and calls old `manifest::load_manifest`
  for legacy `module.json`.
- `crates/core/extension/module/src/manifest/load.rs` currently checks
  `package.json`, then old `module.json`, then `mesh.toml`.
- Shell startup reads `config/package.json` from
  `crates/core/shell/src/shell/discovery.rs` and
  `crates/core/shell/src/shell/backend/spawn.rs`.
- Tests already cover the root graph, loader precedence, keybind preservation,
  active providers, interface declarations, contribution indexing, and shipped
  module fixtures.

## Recommended Technical Approach

1. Rename the Rust manifest vocabulary first, without public compatibility
   aliases:
   - `ModulePackageManifest` -> `ModuleManifest`
   - `RootPackageManifest` -> `RootModuleGraphManifest`
   - `PackageManifestError` -> `ModuleManifestError`
   - `PackageSection` -> `ModuleSection`
   - source variants away from `PackageJson` toward `CanonicalModuleJson`,
     `LegacyPackageJson`, `LegacyModuleJson`, and `LegacyMeshToml`.
2. Add explicit diagnostics before changing loader behavior. A lightweight
   diagnostic record is sufficient:
   - module id when known
   - manifest path
   - field path when known
   - severity (`warning` or `error`)
   - suggested action
3. Make canonical `module.json` load through the current canonical schema.
   Treat old `module.json` as legacy only when it clearly contains old keys
   such as `id`, `type`, or `api_version`.
4. Reject ambiguity before parsing content when multiple manifest files exist
   in one module directory. `module.json` plus `package.json` should be an
   error, not a precedence rule.
5. Move the root graph default from `package.json` to `module.json`, while
   keeping old root `package.json` as an internal migration input with a
   diagnostic.
6. Migrate checked-in artifacts after the loaders can prove canonical behavior:
   `config/module.json`, bundled module `module.json` files, and tests/shell
   references.

## Implementation Notes

- `module.json` is overloaded today. Plans must distinguish canonical
  `module.json` (`name`, `version`, `mesh`) from legacy `module.json`
  (`id`, `type`, `api_version`).
- `provides` cannot disappear abruptly because existing legacy conversion uses
  it to preserve v1.1 backend providers. It should remain accepted as
  migration input with replacement guidance toward `implements`.
- `localizedTriggers` and `localized_triggers` are both currently supported by
  serde aliases in keybind metadata. Canonical output/examples should use
  `localizedTriggers`, but migration input must preserve the snake_case form.
- Operating-system package-manager names inside binary dependency suggestions
  are not MESH module vocabulary and should not be renamed.
- Broad documentation cleanup belongs to Phase 40. Phase 38 should only update
  docs/tests needed to make the canonical schema and runtime behavior clear.

## Validation Architecture

Use the existing Rust test infrastructure.

- Quick command: `cargo test -p mesh-core-module package::tests`
- Focused runtime loader command: `cargo test -p mesh-core-module manifest::tests`
- Shell integration command: `cargo test -p mesh-core-shell shell::tests`
- Full validation command: `cargo test --workspace`

Required automated coverage:

- canonical `module.json` parses as `ModuleManifest`.
- legacy `package.json` is accepted only as migration input and records a
  warning with replacement wording.
- legacy `module.json` is accepted only through the legacy path and records a
  warning.
- `module.json` plus `package.json` in one module directory is a blocking
  ambiguity error.
- `plugin.json` is rejected with replacement/removal guidance.
- root graph loads from `config/module.json`.
- old root `package.json` remains an internal migration input with a warning.
- v1.1 active provider selection still returns `@mesh/pipewire-audio`.
- v1.6 keybind declarations, including localized triggers, survive
  normalization.

## Open Risks

- Renaming exported Rust types can cause many imports to fail at once. Keep the
  rename atomic and update tests/imports in the same plan.
- Diagnostics may need to be threaded through return types instead of hidden in
  tracing logs. Prefer `diagnostics: Vec<ModuleManifestDiagnostic>` on loaded
  manifest structs so tests can assert migration warnings deterministically.
- If artifact migration happens before loader changes, the repo can lose the
  ability to compare old and new behavior. Loader/test work should happen
  before deleting or renaming checked-in old files.

