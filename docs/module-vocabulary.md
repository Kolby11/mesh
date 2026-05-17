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
