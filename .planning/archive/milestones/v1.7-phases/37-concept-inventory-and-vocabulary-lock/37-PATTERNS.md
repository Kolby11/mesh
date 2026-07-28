# Phase 37 Pattern Map

## Purpose

Map the Phase 37 planning targets to closest existing docs and code analogs so executors can make focused edits instead of rediscovering the module system.

## Files To Create

### `docs/module-vocabulary.md`

**Role:** Canonical vocabulary source for v1.7.

**Closest analogs:**

- `docs/module-system.md` - richest current author-facing model.
- `docs/extensibility.md` - strongest interface/provider extensibility framing.
- `docs/health.md` - best example of end-user and author diagnostic language.

**Patterns to copy:**

- Start with a short principle statement before detailed sections.
- Use tables for concept definitions and old-term inventory.
- Use concrete module examples like `@mesh/navigation-bar`, `@mesh/pipewire-audio`, `@mesh/audio-interface`, and `@mesh/material-symbols`.
- Keep service-specific behavior out of core examples.

**Required content patterns:**

- A canonical terms table with exact columns: `Term`, `Definition`, `Developer wording`, `End-user wording`, `Not this`.
- An old-term inventory table with exact columns: `Old term or shape`, `Found in`, `Canonical replacement`, `Disposition`, `Follow-up`.
- A concept-boundary section for module, interface, provider, contribution, dependency, capability, library, and resource pack.
- A future-phase handoff section for Phase 38, Phase 39, Phase 40, and Phase 41.

## Files To Modify

### `.planning/REQUIREMENTS.md`

**Role:** Requirements source for v1.7.

**Pattern:** Keep requirement ids stable. Do not renumber. Edit wording only where context supersedes stale alias language.

**Required changes:**

- CONC-01 should use module-first vocabulary and avoid `package/module identity`.
- CONC-02 should remove "explicit compatibility aliases" and require replacement or internal-only migration handling.
- MAN-01 should move toward `module.json` and module-centered schema wording if Phase 37 execution touches manifest naming.

### `.planning/ROADMAP.md`

**Role:** Phase sequencing source.

**Pattern:** Keep phase numbers and dependencies stable. Update stale goal/success wording without changing phase scope.

**Required changes:**

- Phase 37 should say old terms are inventoried with canonical replacements, not compatibility aliases.
- Phase 38/40 wording should avoid presenting compatibility as a permanent public model.

### `docs/module-system.md`

**Role:** Current primary author-facing module model.

**Patterns to preserve:**

- "A module is..." opening principle.
- Interface/provider/frontend/backend/library distinctions.
- Base/extension/independent interface relationship guidance.
- Capability versus provider identity separation.

**Required changes:**

- Replace package-centered wording with module-centered wording.
- Remove public compatibility-alias language.
- If legacy loader behavior is mentioned, label it as internal-only migration with a removal target.

### `docs/extensibility.md`

**Role:** Dynamic interface/provider model.

**Patterns to preserve:**

- Core knows the registry, not specific services.
- Any module can declare, implement, or consume an interface.
- Methods and events are typed interface primitives.

**Required changes:**

- Replace transition/synonym notes such as trait/interface synonym wording with hard replacement guidance.
- Replace contract package examples with module-centered examples where possible.

### `docs/modules/README.md`

**Role:** Overview of shipped modules.

**Patterns to preserve:**

- Default modules are ordinary modules.
- Frontends consume interface contracts, not provider modules.

**Required changes:**

- Remove package-shaped manifest wording that conflicts with module-centered naming.
- Ensure examples point at the canonical vocabulary doc.

### `docs/modules/backend/core/README.md`

**Role:** Backend author path.

**Patterns to preserve:**

- Backend modules implement interfaces.
- Consumer service capabilities are separate from backend host powers.
- Active provider failure is visible and deterministic.

**Required changes:**

- Replace `package id`, `package.json`, and legacy `provides` wording with Phase 37 target language or old-term inventory pointers.

### `docs/health.md`

**Role:** Health and diagnostic model.

**Patterns to preserve:**

- Plain-language user fix suggestions.
- Structured missing dependency records.
- Module and interface health channels.

**Required changes:**

- Distinguish OS package-manager wording from MESH module terminology.
- Align diagnostics terminology with module/interface/provider/resource concepts.

### `docs/theming/icons.md`

**Role:** Icon pack/resource model.

**Patterns to preserve:**

- Icon packs are XDG-compatible themes.
- Missing icons produce visible placeholders and diagnostics.

**Required changes:**

- Clarify that semantic icon resolver aliases are not old-name compatibility aliases.
- If the word `alias` remains, constrain it to resource resolution mechanics.

## Runtime Inventory Targets

These files should be inventoried in `docs/module-vocabulary.md`, not necessarily renamed in Phase 37:

- `crates/core/extension/module/src/package/module_manifest.rs`
- `crates/core/extension/module/src/package/installed_graph.rs`
- `crates/core/extension/module/src/manifest/model.rs`
- `crates/core/extension/module/src/manifest/json.rs`
- `crates/core/extension/service/src/interface.rs`
- `modules/frontend/navigation-bar/module.json`
- `config/package.json`
- `config/modules/@mesh/*/package.json`

## Verification Patterns

Use deterministic text checks:

- `test -f docs/module-vocabulary.md`
- `rg -n "public alias|compatibility alias|synonym" docs/module-vocabulary.md docs/module-system.md docs/extensibility.md`
- `rg -n "D-01|D-02|D-03|D-04|D-20|D-21" docs/module-vocabulary.md`
- `rg -n "CONC-01|CONC-02|CONC-03" .planning/phases/37-concept-inventory-and-vocabulary-lock/*-PLAN.md`
