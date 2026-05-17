# MESH Module Vocabulary

This document is the canonical vocabulary source for the v1.7 module and
extensibility model.

> A module is the installable MESH unit.

MESH uses one public vocabulary for author docs, diagnostics, manifests, and
planning. Old terms are tracked here so later phases can replace or remove
them without presenting them as supported synonyms.

## Canonical Terms

| Term | Definition | Developer wording | End-user wording | Not this |
| ---- | ---------- | ----------------- | ---------------- | -------- |
| module | Installable and configurable MESH unit. A module can contribute UI, backend behavior, interfaces, resources, libraries, settings, or data. | `module`, `module id`, `module.json` | Theme module, Audio provider, Icon pack | package, plugin, addon |
| module kind | The primary role a module declares, such as frontend, backend, interface, library, theme, icon pack, font pack, language pack, or another resource pack kind. | `module kind`, `kind` | Module type | package type, service category |
| interface | Named contract for state, methods, events, types, and capability requirements. | `interface`, `interface contract` | Audio interface, Missing interface | provider identity, Rust service logic, trait |
| provider | Backend module implementation of an interface. | `provider`, `active provider`, `provider module` | Audio provider, Network provider | frontend dependency, interface contract |
| contribution | Something a module adds to the installed graph. | `contribution`, `contributes` | Added shortcut, Theme, Icon pack | dependency, capability |
| dependency | Something a module needs to run or build against. | `dependency`, `requires`, `needs` | Required module, Missing resource | contribution, capability |
| capability | Host power granted to a module. | `capability`, `capabilities.required` | Permission, Access requirement | dependency, provider selection |
| resource pack | Module kind that contributes resource files such as icons, fonts, translations, sounds, or future typed resources. | `resource pack`, `icon pack`, `font pack`, `language pack` | Icon pack, Font pack, Language pack | compatibility alias |
| library | Module that contributes importable Luau code. | `library module`, `libraries` | Shared module library | provider, permission grant |
| settings | User or module configuration values. | `settings`, `settings schema`, `configuration` | Preferences, Settings | capability, dependency |
| entrypoint | Named launch or UI entry contributed by a module. | `entrypoint`, `entrypoints.main`, `surface entrypoint` | Panel item, Widget, Surface | package main field |

## Public Naming Rules

Old names are replacement debt, not public aliases.

Temporary migration loaders are internal implementation details, not author-facing vocabulary.

Use the canonical term in new public docs, examples, diagnostics, and plans.
When old syntax still exists during migration, diagnostics should say what to
replace it with, name the exact module id and field path, and avoid claiming
that the old and new names are interchangeable.

## Old-Term Inventory

| Old term or shape | Found in | Canonical replacement | Disposition | Follow-up |
| ----------------- | -------- | --------------------- | ----------- | --------- |
| package | Docs, Rust type names, manifest loader names, planning artifacts | module | replace | Rename public docs and diagnostics first; later phases can rename runtime types when practical. |
| plugin | Older planning language and some developer-facing descriptions | module | replace | Remove from public author docs unless describing historical artifacts. |
| package.json | Current author docs, root installed graph config, bundled backend manifests | module.json | internal-only migration | Phase 38 defines and implements the canonical manifest path, then migrates shipped artifacts before old loader removal. |
| plugin.json | Older concept drafts or third-party vocabulary risk | module.json | remove | Reject or document only as an unsupported historical name if encountered. |
| trait | Earlier extensibility docs and `mesh-core-service` framing | interface | replace | Public docs should say interface; runtime/code names can be inventoried for later rename. |
| service category | Earlier backend/service grouping language | interface domain or module kind | replace | Use interface domain when grouping contracts; use module kind when describing installable roles. |
| provides | Legacy provider declaration field and examples | implements or contributes.providers | internal-only migration | Phase 38/39 should normalize to provider/interface contributions with diagnostics. |
| compatibility alias | Stale planning language | replacement or internal-only migration | remove | Do not document old names as synonyms; diagnostics should say replace with the canonical term. |

## Prior Decision Reconciliation

### v1.1 Provider Selection

The v1.1 backend provider behavior remains part of the canonical model. The
rule is: frontend modules depend on interface contracts, backend modules
contribute providers, and user configuration selects the active provider for
each interface. Provider selection is configuration over modules that implement
an interface, not a frontend dependency on a backend module.

When the selected provider is missing, disabled, or unhealthy, diagnostics
should name the interface, active provider module id, and field path that
selected it. They should not imply that a frontend can fix the problem by
depending on a provider module directly.

### v1.6 Keybind Declarations

The paused v1.6 keybind model remains valid as a module contribution model:
keybind actions are contributions, localized triggers are contribution metadata, and user overrides are settings/configuration.

Keybind identity comes from the declaring module and action id. Locale-specific
trigger defaults help the shell choose a default binding, but they do not
change the contribution identity or turn keybinds into provider behavior.

## Innovation Rules

MESH should support new module ideas without adding service-specific Rust
branches. Authors may add new interfaces, providers, libraries, resource
packs, and typed contributions when those concepts are declared through the
module model.

Interface relationships may be `base`, `extension`, or `independent`.
Independent interfaces are allowed, but docs and diagnostics should guide
authors toward extending a base interface when interoperability benefits.

Consistency comes from typed registries, strict field names, validation,
diagnostics, and author docs. Defaults have no privileged conceptual status;
they are reference modules that use the same model third-party authors use.

## Runtime Inventory

| Location | Current term | Target term | Disposition | Follow-up phase | Behavior to preserve |
| -------- | ------------ | ----------- | ----------- | --------------- | -------------------- |
| `crates/core/extension/module/src/package/module_manifest.rs` | `ModulePackageManifest` | `ModuleManifest` | replace | Phase 38 | Parse and validate module manifests without losing v1.1 provider or v1.6 keybind data. |
| `crates/core/extension/module/src/package/module_manifest.rs` | `RootPackageManifest` | `RootModuleManifest` or installed module graph manifest | replace | Phase 38 | Keep root enabled-module graph, active providers, and layout entrypoint selection. |
| `crates/core/extension/module/src/manifest/model.rs` | `PackageSection` | `ModuleSection` | replace | Phase 38 | Preserve normalized identity, version, module kind, API version, and metadata. |
| `crates/core/extension/module/src/package/*.rs` | `PackageManifestError` | `ModuleManifestError` | replace | Phase 38 | Keep actionable validation, JSON, and IO diagnostics with module id and field path where available. |
| author docs and shipped manifests | `package.json` | `module.json` | internal-only migration | Phase 38 | Load old shipped artifacts only as an internal migration path until they are migrated. |
| author docs and future manifests | `module.json` | `module.json` | already canonical | Phase 38 | Make this the author-facing manifest name for new examples and diagnostics. |
| historical manifest risk | `plugin.json` | none | remove | Phase 40 | Do not present plugin naming as supported vocabulary. |
| legacy provider declarations | `provides` | `implements` or `contributes.providers` | internal-only migration | Phase 38 and Phase 39 | Preserve backend provider declarations while diagnostics guide authors to canonical fields. |
| module manifests | `implements` | `implements` | already canonical | Phase 38 and Phase 39 | Keep backend provider declarations tied to named interface contracts. |
| `crates/core/extension/service/src/interface.rs` | `InterfaceRegistry` | `InterfaceRegistry` | already canonical | Phase 39 | Keep generic interface/provider registration and resolution without service-specific Rust branches. |
| `crates/core/extension/module/src/package/installed_graph.rs` | `BackendProviderNode` | `ProviderContributionNode` or `InterfaceProviderNode` | replace | Phase 39 | Preserve active-provider validation and provider ordering by interface. |
| `crates/core/extension/module/src/package/installed_graph.rs` | `ModuleContributionIndex` | `ModuleContributionIndex` | already canonical | Phase 39 | Preserve typed indexing for layout, themes, icons, fonts, i18n, libraries, settings, interfaces, and providers. |
| v1.6 keybind manifest data | `localized_triggers` | keybind contribution metadata | already canonical | Phase 40 | Preserve locale-specific default trigger resolution and user override precedence. |
| v1.6 compatibility settings | `settings.keyboard.shortcuts` | keybind contributions plus settings overrides | internal-only migration | Phase 40 | Preserve existing shortcuts as migration input to the contribution model. |
| `config/package.json` | root package manifest file | root module graph manifest | internal-only migration | Phase 38 | Preserve active module graph, active providers, enabled flags, paths, and layout entrypoint. |

## Future-Phase Handoff

### Phase 38: Manifest Normalization

Phase 38 should define `module.json` as the canonical author-facing manifest
and move runtime normalization toward module-named structs and diagnostics.
Any old manifest loader should be described as internal-only migration loaders,
not public compatibility aliases. The normalization path must preserve active
provider declarations, interface declarations, keybind declarations,
capabilities, dependencies, entrypoints, settings, and resource requirements.

### Phase 39: Contribution And Interface Index

Phase 39 should make extension behavior inspectable through typed contribution indexes for providers, interfaces, libraries, settings, keybinds, resources,
and frontend entrypoints. Interface/provider validation must keep dependency,
capability, and contribution separate while allowing base, extension, and
independent interface relationships. New extension behavior should route
through manifests, contracts, libraries, providers, and resource packs without
service-specific Rust branches.

### Phase 40: Migration Diagnostics And Docs

Phase 40 should migrate bundled docs, examples, and diagnostics away from old
public names. Diagnostics should say `replace with` or `remove`, not `alias`.
If old loaders or fields still exist internally for sequencing, they should be
visible as migration warnings with removal targets and exact field paths.
Resource lookup aliases and operating-system package names remain separate
mechanics, not vocabulary aliases.

### Phase 41: Shipped Proof

Phase 41 should prove the model on a bundled module/provider path using
canonical module vocabulary end to end. The proof should include a canonical
manifest, interface/provider behavior, typed contributions, diagnostics, and
tests without adding service-specific Rust APIs.
