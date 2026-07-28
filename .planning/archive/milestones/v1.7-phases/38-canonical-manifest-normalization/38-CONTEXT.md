# Phase 38: Canonical Manifest Normalization - Context

**Gathered:** 2026-05-17T20:41:41+02:00
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 38 turns the Phase 37 vocabulary lock into runtime manifest behavior. It
defines and implements canonical `module.json` normalization, migrates the
checked-in module/root graph artifacts where needed, preserves existing v1.1
provider and v1.6 keybind behavior, and emits actionable diagnostics for old,
invalid, duplicate, or ambiguous manifest shapes.

This phase is about manifest schema, loader order, runtime normalization,
diagnostics, tests, and shipped artifact migration. It is not the phase for
typed contribution indexing, provider conflict policy beyond preserving current
active-provider behavior, installer UX, marketplace/distribution, or resumed
keybind dispatch/accessibility proof.

</domain>

<decisions>
## Implementation Decisions

### Canonical Manifest Shape
- **D-01:** The canonical author-facing module manifest file is `module.json`.
  This supersedes stale MAN-01 wording that still says `package.json`; planning
  must follow Phase 37 and `docs/module-vocabulary.md`.
- **D-02:** Canonical `module.json` should use the module schema currently
  represented by `ModulePackageManifest`: top-level `name`, `version`,
  optional metadata, and a `mesh` section. Top-level `name` is the canonical
  module id, not npm package vocabulary.
- **D-03:** The older `module.json` shape with top-level `id`, `type`, and
  `api_version` is a legacy runtime manifest shape, not the Phase 38 target.
  It may be normalized only through an internal migration path with diagnostics.
- **D-04:** Canonical public JSON field spelling should use the established
  camelCase module schema where it already exists: `apiVersion`, `baseModule`,
  `schemaVersion`, `modulesDir`, and `localizedTriggers`. Existing snake_case
  forms such as `api_version`, `base_module`, or `localized_triggers` are
  migration inputs, not canonical examples.

### Loader And Migration Behavior
- **D-05:** Loader precedence should prefer canonical `module.json`. A
  `package.json` module manifest is an internal-only migration loader that
  emits a warning telling authors to replace it with `module.json`.
- **D-06:** If a module directory contains multiple manifest files that could
  describe the same module, such as both `module.json` and `package.json`, the
  result should be a blocking ambiguity diagnostic. Do not silently prefer one
  when the author has supplied conflicting sources.
- **D-07:** `plugin.json` should not be accepted as a public or migration
  manifest name. If encountered, diagnostics should say to remove it or replace
  it with canonical `module.json`.
- **D-08:** `mesh.toml` may remain only as an internal legacy-normalization
  path when needed to preserve existing tests or artifacts. It should not be
  documented as an author-facing option, and diagnostics should point toward
  `module.json`.

### Root Graph And Shipped Artifact Migration
- **D-09:** The root installed-module graph should move from
  `config/package.json` toward `config/module.json`. The root graph is not an
  installable module, so its canonical shape may remain the root graph shape
  (`schemaVersion`, `modulesDir`, `modules`, `providers`, `layout`, `theme`)
  rather than pretending to be a module manifest.
- **D-10:** Phase 38 should migrate checked-in artifacts that the runtime loads,
  including `config/package.json`, bundled module `package.json` manifests, and
  legacy module manifests under `modules/**`, to canonical paths/shapes where
  practical. If a hard removal would break the repo, keep the loader as
  internal migration with a diagnostic and test coverage.
- **D-11:** The migration must preserve the current active provider mapping,
  enabled module graph, layout entrypoint, interface declarations, provider
  declarations, capabilities, dependencies, settings, resources, and v1.6
  keybind declarations.

### Diagnostics And Validation
- **D-12:** Diagnostics must teach replacement, not compatibility. Messages for
  old filenames or fields should use wording such as `replace with module.json`
  or `remove`, not `alias`, `synonym`, or `compatible name`.
- **D-13:** Invalid, duplicate, or ambiguous manifest data should be blocking.
  Deprecated-but-loadable migration inputs should be warnings with module id,
  source path, field path when available, replacement wording, and a removal
  target.
- **D-14:** Diagnostics should distinguish MESH module manifests from
  operating-system package names and resource lookup aliases. OS package names
  in dependency suggestions and icon resolver aliases are not vocabulary
  aliases and should not be renamed by this phase.

### Runtime Type Names And API Surface
- **D-15:** Runtime/public Rust names should move toward module vocabulary:
  `ModulePackageManifest` -> `ModuleManifest`, `RootPackageManifest` ->
  `RootModuleGraphManifest` or equivalent, `PackageManifestError` ->
  `ModuleManifestError`, and `PackageSection` -> `ModuleSection`.
- **D-16:** Do not add compatibility type aliases for the old Rust names as a
  public API. If an immediate rename is too wide for one plan, leave explicit
  inventory and follow-up, but do not present old and new type names as
  supported synonyms.
- **D-17:** Any runtime normalization refactor must preserve `implements`,
  legacy `provides` migration, `localizedTriggers`/localized keybind data,
  `settings.keyboard.shortcuts` migration input, and `MeshModuleSection`
  validation behavior.

### Folded Todos
- **Create unified package and module manifest phase:** Folded into Phase 38 as
  the canonical `module.json` schema and shipped artifact migration work.
- **Define module install requirement resolution:** Folded only where it
  affects manifest normalization: preserve declared requirements,
  contributions, capabilities, providers, resources, and settings data so
  Phase 39 can index/validate them. Detailed resolution policy remains Phase
  39.

### Planner Discretion
The planner may decide the exact plan split and whether type renames and
artifact migration happen in separate plans. The planner should bias toward
small, verifiable steps that keep the repo building after each plan.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone And Phase Context
- `.planning/PROJECT.md` - v1.7 milestone goal and Phase 37 completion state.
- `.planning/REQUIREMENTS.md` - MAN-01 through MAN-03 requirements. Treat the
  stale MAN-01 `package.json` wording as superseded by Phase 37 and this
  context.
- `.planning/ROADMAP.md` - Phase 38 goal now references canonical
  `module.json` manifest normalization.
- `.planning/STATE.md` - Prior provider/keybind/runtime decisions that must be
  preserved.
- `.planning/phases/37-concept-inventory-and-vocabulary-lock/37-CONTEXT.md` -
  Locked vocabulary and no-public-alias decisions.
- `.planning/phases/37-concept-inventory-and-vocabulary-lock/37-VERIFICATION.md`
  - Confirms Phase 37 passed and identifies accepted residual runtime risk.
- `docs/module-vocabulary.md` - Canonical vocabulary, runtime inventory, and
  Phase 38-41 handoff.

### Runtime Manifest Code
- `crates/core/extension/module/src/package/module_manifest.rs` - Current
  module schema, conversion to runtime manifest, legacy conversion path,
  provider declarations, and keybind preservation.
- `crates/core/extension/module/src/package/root.rs` - Root installed graph
  parser and current `RootPackageManifest` shape.
- `crates/core/extension/module/src/package/installed_graph.rs` - Installed
  module graph loader, current module manifest loader precedence, active
  provider validation, and contribution index.
- `crates/core/extension/module/src/package/error.rs` - Current
  `PackageManifestError` messages that need module vocabulary.
- `crates/core/extension/module/src/package/paths.rs` - Current default
  `~/.mesh/package.json` root path.
- `crates/core/extension/module/src/manifest/load.rs` - Legacy runtime
  manifest loader for `package.json`, old `module.json`, and `mesh.toml`.
- `crates/core/extension/module/src/manifest/model.rs` - Compatibility
  normalized `Manifest`, `PackageSection`, `ProvidedInterface`, keybinds, and
  source enum.
- `crates/core/extension/module/src/manifest/json.rs` - Old JSON
  `module.json` parser shape.
- `crates/core/extension/module/src/manifest/tests.rs` - Existing keybind,
  module JSON, and package JSON normalization tests to preserve or update.
- `crates/core/extension/module/src/package/tests.rs` - Existing package/root
  graph tests, including loader precedence tests that Phase 38 should update.

### Shell Integration And Fixtures
- `crates/core/shell/src/shell/discovery.rs` - Runtime discovery checks for
  manifest files and root graph path.
- `crates/core/shell/src/shell/backend/spawn.rs` - Backend startup root graph
  path.
- `crates/core/shell/src/shell/tests.rs` - Shell tests referencing
  `config/package.json` and package manifest paths.
- `config/package.json` - Current root installed-module graph to migrate or
  support as internal migration.
- `modules/frontend/navigation-bar/module.json` - Existing legacy module JSON
  frontend manifest that must be migrated or normalized without losing
  keybind/settings/capability data.
- `modules/backend/pipewire-audio/package.json` - Existing package-shaped
  backend manifest to migrate to canonical `module.json` while preserving
  provider/capability data.
- `modules/backend/pulseaudio-audio/module.json` - Existing module manifest
  fixture to inspect during migration.

### Folded Inputs
- `.planning/todos/pending/2026-05-08-create-unified-package-and-module-manifest-phase.md`
  - Original manifest unification todo; folded into canonical manifest work.
- `.planning/todos/pending/2026-05-15-define-module-install-requirement-resolution.md`
  - Requirement/contribution/resource resolution todo; folded only for data
  preservation, not full Phase 39 resolution policy.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ModulePackageManifest`, `MeshModuleSection`, `ModuleKind`, and related
  structs already represent most of the target canonical module schema.
- `RootPackageManifest` already supports both a `mesh` wrapper and a direct
  root graph shape; this can be reused while renaming the root graph concept.
- `ModulePackageManifest::from_legacy_manifest()` already maps old runtime
  manifests into the newer module schema and should be the main preservation
  path for legacy data.
- `InstalledModuleGraph::from_parts()` already validates active providers,
  frontend requirements, interface declarations, layout entrypoints, and
  contribution indexing.
- Existing tests cover package-shaped manifests, old module JSON manifests,
  localized keybind triggers, active provider graph loading, and loader
  precedence.

### Established Patterns
- Rust core must stay generic; no service-specific branches should be added
  while normalizing manifests.
- Frontends consume interfaces, not backend module ids.
- Provider selection is explicit through the installed module graph and should
  not silently fallback after provider launch failure.
- Capabilities, dependencies, and contributions remain separate concepts.
- Diagnostics should be visible and actionable rather than hidden compatibility
  behavior.

### Integration Points
- `crates/core/extension/module/src/package/installed_graph.rs` has the
  current `load_module_manifest()` precedence that prefers `package.json` over
  `module.json`; Phase 38 should change this.
- `crates/core/extension/module/src/manifest/load.rs` currently treats old
  `module.json` as a runtime manifest shape; Phase 38 must not confuse this
  with canonical `module.json`.
- `crates/core/shell/src/shell/discovery.rs` and
  `crates/core/shell/src/shell/backend/spawn.rs` currently hard-code
  `config/package.json`; these are root graph migration points.
- `crates/core/extension/module/src/package/paths.rs` still defaults to
  `~/.mesh/package.json`; this should move toward `~/.mesh/module.json` with
  migration behavior.

</code_context>

<specifics>
## Specific Ideas

- The user explicitly corrected Phase 37 toward hard replacement: old names
  should be replaced, with no compatibility aliases.
- Phase 38 should be strict enough that downstream authors see `module.json`
  as the only public manifest name, while the repo remains migratable through
  internal loaders and visible diagnostics.
- The biggest implementation trap is current naming: the repo already has
  `module.json` files, but several use the legacy `id/type/api_version` shape.
  Planning must call this out instead of assuming those files are canonical.

</specifics>

<deferred>
## Deferred Ideas

- Full typed contribution indexing, provider/interface conflict policy,
  resource cascade resolution, and settings materialization belong to Phase 39.
- Broad docs/examples migration and author-facing migration guide polish belong
  to Phase 40.
- Shipped end-to-end proof on one module/provider path belongs to Phase 41.
- Completing paused v1.6 keybind dispatch, conflict diagnostics, and
  accessibility proof remains out of scope until after v1.7 stabilizes the
  module model.

### Reviewed Todos (not folded)
- **Audio popover transition delay polish** - False-positive todo match from
  generic "phase" wording; remains unrelated Phase 31 polish debt.

</deferred>

---

*Phase: 38-Canonical Manifest Normalization*
*Context gathered: 2026-05-17T20:41:41+02:00*
