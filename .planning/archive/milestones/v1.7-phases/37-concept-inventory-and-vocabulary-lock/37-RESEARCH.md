---
phase: 37
slug: concept-inventory-and-vocabulary-lock
status: complete
created: 2026-05-17
requirements: [CONC-01, CONC-02, CONC-03]
---

# Phase 37 Research: Concept Inventory and Vocabulary Lock

## RESEARCH COMPLETE

## Objective

Plan Phase 37 well by identifying the current vocabulary drift, the canonical concept model to lock, and the verification strategy needed before later manifest and contribution phases implement the model.

## Inputs Read

- `.planning/phases/37-concept-inventory-and-vocabulary-lock/37-CONTEXT.md`
- `.planning/REQUIREMENTS.md`
- `.planning/ROADMAP.md`
- `.planning/STATE.md`
- `.planning/research/SUMMARY.md`
- `.planning/codebase/STACK.md`
- `.planning/codebase/ARCHITECTURE.md`
- `.planning/codebase/CONVENTIONS.md`
- `docs/module-system.md`
- `docs/extensibility.md`
- `docs/modules/README.md`
- `docs/modules/backend/core/README.md`
- `docs/health.md`
- `docs/theming/icons.md`
- `crates/core/extension/module/src/package/module_manifest.rs`
- `crates/core/extension/module/src/package/installed_graph.rs`
- `crates/core/extension/module/src/manifest/model.rs`
- `crates/core/extension/module/src/manifest/json.rs`
- `crates/core/extension/service/src/interface.rs`
- `modules/frontend/navigation-bar/module.json`

## Key Findings

### 1. The module model already exists, but old names still frame it

The current codebase already has strong module-centered structures:

- `ModulePackageManifest`, `MeshModuleSection`, and `ModuleKind` parse module metadata.
- `InstalledModuleGraph` resolves installed modules, active providers, interface declarations, and contributions.
- `ModuleContributionIndex` indexes layout, themes, icons, fonts, i18n, libraries, and settings.
- `InterfaceRegistry` keeps interface/provider resolution generic.

However, many names still preserve package-era or compatibility-era vocabulary:

- Rust type names include `ModulePackageManifest`, `PackageSection`, `RootPackageManifest`, and `PackageManifestError`.
- Docs describe `package.json` plus `mesh` as the target manifest even though Phase 37 context now locks hard replacement toward module-centered naming.
- Some docs still say old terms are synonyms or transition aliases, especially `docs/extensibility.md` and `docs/module-system.md`.

Planning consequence: Phase 37 should not attempt all runtime renames. It should create a precise inventory that later phases can execute without ambiguity.

### 2. Current docs contain good concept boundaries worth preserving

Useful existing model statements:

- Frontend modules consume interfaces, not backend module ids.
- Backend modules implement interface contracts.
- Capabilities gate host power and are not provider identity.
- Libraries contribute importable Luau code and do not grant capabilities.
- Interface relationships can be `base`, `extension`, or `independent`.
- The Rust core remains generic and should not add service-specific branches.

Planning consequence: Phase 37 should preserve these concept boundaries while replacing old names and removing public synonym language.

### 3. Hard replacement conflicts with current roadmap and requirement wording

Current Phase 37 and CONC-02 wording still mention compatibility aliases. The new context explicitly supersedes that:

- Old names are replacement debt, not public aliases.
- Temporary loaders or migration paths may exist internally, but docs and diagnostics should not present old names as supported vocabulary.
- The inventory should classify old terms as `replace`, `remove`, or `internal-only migration`.

Planning consequence: at least one plan must update planning artifacts or a vocabulary source document so future phases do not plan from stale alias wording.

### 4. User-facing language needs a layered model

Developer docs need precise terms: module, module kind, interface, provider, contribution, capability, dependency, resource pack, library, settings, entrypoint.

End-user diagnostics should use concrete nouns: "Audio provider", "Icon pack", "Theme module", "Missing interface", "Missing resource". They should expose module ids and field paths for authors without making normal users parse graph internals first.

Planning consequence: the canonical vocabulary artifact should include both developer-facing and end-user-facing wording.

### 5. Resource wording is a gray area

`docs/theming/icons.md` currently uses semantic aliases as icon profile data. The no-public-alias rule is about old MESH terminology, not necessarily every resource resolver alias. Still, the term `alias` may confuse the model.

Planning consequence: Phase 37 should inventory and clarify whether resource resolver aliases are acceptable as implementation mechanics, while ensuring old terminology is never treated as a public compatibility alias.

## Recommended Plan Shape

### Plan 37-01: Canonical Vocabulary And Inventory Source

Create the canonical source artifact that defines the locked vocabulary and inventories old names.

Primary output candidates:

- `docs/module-vocabulary.md`
- Updates to `.planning/REQUIREMENTS.md` / `.planning/ROADMAP.md` to remove compatibility-alias language or explicitly point to the Phase 37 context.

### Plan 37-02: Author Docs Replacement Pass

Update high-value docs so author-facing docs stop teaching old names as canonical or equivalent.

Primary docs:

- `docs/module-system.md`
- `docs/extensibility.md`
- `docs/modules/README.md`
- `docs/modules/backend/core/README.md`
- `docs/health.md`
- `docs/theming/icons.md`

### Plan 37-03: Runtime And Future-Phase Handoff

Produce a runtime/docs handoff for Phase 38-41. This should identify code type names, loader paths, diagnostics, tests, and shipped manifests that later phases must change, without trying to complete every code rename in Phase 37.

Primary output candidates:

- Add a runtime mapping section to `docs/module-vocabulary.md`.
- Add a Phase 37 planning handoff artifact, or put the handoff in the vocabulary doc and `37-SUMMARY.md` after execution.

## Validation Architecture

Phase 37 is documentation and planning-artifact heavy. Validation should use deterministic grep/file checks rather than long Rust test suites unless a plan changes Rust code.

### Automated Checks

- `test -f docs/module-vocabulary.md`
- `rg -n "public alias|compatibility alias|treat.*synonym|synonym" docs/module-vocabulary.md docs/module-system.md docs/extensibility.md docs/modules/README.md docs/modules/backend/core/README.md docs/health.md docs/theming/icons.md` should not find old-name synonym guidance.
- `rg -n "D-01|D-02|D-03|D-04|D-08|D-09|D-10|D-11|D-12|D-13|D-16|D-20|D-21" docs/module-vocabulary.md` should show the locked context decisions are represented.
- `rg -n "CONC-01|CONC-02|CONC-03" .planning/phases/37-concept-inventory-and-vocabulary-lock/*-PLAN.md` should show requirements coverage in plan files.
- `rg -n "ModulePackageManifest|RootPackageManifest|PackageSection|package.json|module.json|provides|implements|plugin|trait|service category" docs/module-vocabulary.md` should show old terms are inventoried with replacement classes.

### Manual Checks

- Read the vocabulary artifact as a new module author and verify the concept boundaries answer: what is a module, interface, provider, contribution, dependency, capability, resource pack, and library?
- Read one end-user diagnostic wording section and verify it would be understandable without knowing Rust type names.

### Non-goals For Validation

- Full runtime code rename is not required in Phase 37.
- Full manifest loader behavior changes are Phase 38.
- Full contribution indexing implementation changes are Phase 39.

