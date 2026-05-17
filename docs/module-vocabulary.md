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

The v1.1 backend provider behavior remains part of the canonical model.
frontend modules depend on interface contracts, backend modules contribute providers, and user configuration selects the active provider for each
interface. Provider selection is configuration over modules that implement an
interface, not a frontend dependency on a backend module.

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
