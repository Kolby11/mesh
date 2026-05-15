# Feature Research: v1.7 Modularity and Extensibility

## Table Stakes

- One canonical vocabulary for module authors and core maintainers.
- One installable package shape with standard package metadata and MESH behavior under `mesh`.
- Typed contributions for frontend surfaces/widgets, backend providers, interface contracts, libraries, settings, keybinds, theme/icon/font/language resources, and layout entrypoints.
- Explicit capability and dependency vocabulary that distinguishes host permissions, package dependencies, interface provider requirements, native dependencies, and resource dependencies.
- Compatibility with existing package graph behavior, legacy manifest files, backend provider declarations, and v1.6 keybind declarations.
- Diagnostics that explain what is wrong in author language: invalid section, deprecated field, missing dependency, missing provider, incompatible interface version, capability mismatch, duplicate contribution, or migration warning.
- Proof that a new or refactored module path can declare its contract, provider, resources, settings, keybinds, and docs without core service-specific branches.

## Differentiators

- Interface relationship metadata (`base`, `extension`, `independent`) to guide ecosystem reuse without blocking independent experimentation.
- Contribution indexing as a first-class runtime concept, so shell surfaces, libraries, resources, keybinds, and provider choices can be inspected consistently.
- Contract-derived Luau/LSP metadata so author tooling follows interface contracts rather than whichever backend happens to be installed.
- Compatibility diagnostics that make old manifests safe to load while steering new authors toward the canonical schema.
- Cross-module resource rules for theme tokens, icons, slots, and libraries that make reuse explicit rather than ambient.

## Anti-Features

- Do not turn the manifest into a dumping ground for module-private state.
- Do not add Rust APIs such as `mesh.audio.get_volume()` for service-specific behavior.
- Do not privilege default modules over third-party modules in the model.
- Do not make library modules confer capabilities implicitly.
- Do not make the milestone a marketplace, signing, installer, or distribution milestone.
- Do not finish all paused v1.6 keybind runtime phases here; only carry forward the declaration/resolution concepts needed for the module model.

## Open Questions for Plan-Phase

- Whether `ModuleType::Surface` and `ModuleType::Widget` should become `frontend` kinds with contribution subtypes, or remain compatibility variants normalized to `frontend`.
- Whether `mesh.dependencies.backend` should replace, alias, or coexist with current interface dependency structures.
- Which diagnostics should be blocking load errors and which should be warnings during migration.
- How much contract-derived Luau/LSP metadata can be generated in this milestone without becoming a separate tooling phase.
