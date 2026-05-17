# Phase 40: Migration Diagnostics and Author Docs - Research

## Research Complete

Phase 40 is the migration-facing phase for the v1.7 module model. Phase 38
already made `module.json` the canonical runtime target and added explicit
loader diagnostics for legacy manifest files. Phase 39 already made typed
installed-graph records available to runtime code. Phase 40 should turn those
runtime facts into a coherent author migration path and close the remaining
keybind continuity gap before Phase 41 proves a real bundled path end to end.

No finalized `40-CONTEXT.md` exists. Planning is based on the roadmap,
requirements, Phase 37 handoff, Phase 38 manifest context, Phase 39 graph
summary, and source inspection.

## Existing Implementation

- `crates/core/extension/module/src/package/error.rs` defines
  `ModuleManifestDiagnosticSeverity`, `ModuleManifestDiagnostic`, and
  `ModuleManifestError`.
- `crates/core/extension/module/src/package/installed_graph.rs` rejects
  `plugin.json`, treats `package.json`, legacy `module.json`, and `mesh.toml`
  as migration inputs, and emits warning/error diagnostics with replacement
  actions such as `replace package.json with module.json`.
- `InstalledModuleGraph` stores compatibility diagnostics from loaded module
  manifests and graph-level diagnostics for resource/settings issues.
- `ModuleContributionIndex` indexes keybind actions, but
  `ContributedKeybindAction` currently exposes action identity and text
  metadata only. It does not expose the default trigger or localized trigger
  map, so future keybind dispatch work would still need to re-read manifests.
- Shell keyboard code in
  `crates/core/shell/src/shell/component/input/keyboard.rs` preserves
  manifest keybind declarations and uses legacy settings shortcuts only as a
  fallback when a manifest action does not already define the same id.
- `docs/module-system.md`, `docs/module-vocabulary.md`, and
  `docs/modules/README.md` already contain the new vocabulary.
- Several author docs still teach old manifest names or shapes, including
  `docs/installation.md`, `docs/font-system.md`, `docs/theming/themes.md`,
  `docs/theming/locales.md`, `docs/settings/README.md`, and
  `docs/llm-context.md`. Some references in `docs/icon-system.md` and
  `docs/theming/icons.md` may already be in user-modified files, so execution
  must read the current working tree before editing.

## Recommended Technical Approach

1. Harden the diagnostic contract first.
   - Keep blocking loader errors and migration warnings separate by severity.
   - Add tests that assert old manifest names use `replace` or `remove`
     wording, never public `alias` or `synonym` wording.
   - Add a small author-facing diagnostics table so module authors know which
     legacy inputs are accepted temporarily and which are hard errors.
2. Sweep author docs and examples toward canonical `module.json`.
   - Replace `package.json` as the normal manifest name with `module.json`.
   - Replace top-level `id`, `type`, and `api_version` examples with
     top-level `name`, `version`, and `mesh.apiVersion`/`mesh.kind`.
   - Preserve operating-system package-manager names and resource lookup
     aliases; these are not MESH module vocabulary aliases.
   - Leave explicit migration sections only where they teach replacement or
     removal.
3. Preserve v1.6 keybind data in the typed contribution model.
   - Extend `ContributedKeybindAction` with `trigger` and
     `localized_triggers` so installed-graph consumers can inspect all data
     needed by later dispatch/conflict phases.
   - Keep `settings.keyboard.surface_shortcuts` as user override data, not as
     the canonical declaration format.
   - Document the migration path from older `settings.keyboard.shortcuts` and
     surface shortcut fallback behavior to manifest keybind contributions.

## Validation Architecture

Use focused Rust tests plus grep-based docs checks.

- Quick module command: `cargo test -p mesh-core-module package::tests`
- Keybind graph command: `cargo test -p mesh-core-module package::tests keybind`
- Shell keybind command: `cargo test -p mesh-core-shell shell::component::tests::interaction::navigation`
- Documentation check command:
  `rg -n "package.json|mesh.toml|plugin.json|id/type/api_version|settings.keyboard.shortcuts" docs/installation.md docs/font-system.md docs/theming/themes.md docs/theming/locales.md docs/settings/README.md docs/llm-context.md docs/module-system.md docs/module-vocabulary.md`

Required automated coverage:

- legacy `package.json` diagnostics are warnings with `replace package.json with module.json`.
- `plugin.json` diagnostics are errors with `remove plugin.json or replace it with module.json`.
- ambiguous manifest files are errors, not migration warnings.
- migration docs distinguish blocking load errors from migration warnings.
- author docs use `module.json`, `name`, `mesh.apiVersion`, and `mesh.kind` as canonical examples.
- typed keybind contribution records expose action id, scope, label, description, category, default trigger, and localized triggers.
- shell shortcut resolution still lets user overrides win over manifest defaults and keeps legacy settings fallback only when manifest declarations are absent.

## Open Risks

- Broad docs sweeps can accidentally rewrite package-manager terminology or
  resource resolver aliases. Plans must name files and include grep checks that
  focus on MESH manifest vocabulary, not every occurrence of the word
  `package`.
- `docs/icon-system.md` and `docs/theming/icons.md` are currently modified in
  the working tree. Executors must read and preserve user edits in those files
  if they touch them.
- Adding trigger data to typed graph keybind records should clone existing
  manifest data, not invent a second keybind resolution path.

