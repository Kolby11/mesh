# Coding Conventions

**Analysis Date:** 2026-05-06

## Naming Patterns

**Files:**
- Use Rust `snake_case.rs` module files under crate-local `src/` trees, for example `crates/core/extension/plugin/src/package.rs`, `crates/core/runtime/scripting/src/context.rs`, and `crates/core/ui/component/src/parser/markup.rs`.
- Use `mod.rs` only for directory modules that own child files, for example `crates/core/shell/src/shell/mod.rs` and `crates/tools/lsp/src/analyzer/mod.rs`.
- Use kebab-case crate package names in `Cargo.toml` and `mesh-core-*` internal crate prefixes, for example `mesh-core-plugin` in `crates/core/extension/plugin/Cargo.toml` and `mesh-tools-cli` in `crates/tools/cli/Cargo.toml`.
- Use `.mesh` for single-file frontend components and kebab-case component file names, for example `modules/frontend/navigation-bar/src/components/volume-button.mesh`.
- Use `package.json` for new module manifests under `config/modules/@mesh/*/package.json`. Legacy `package.json` remains present in `modules/frontend/navigation-bar/package.json` and legacy backend manifests remain present in `modules/backend/*/package.json`.

**Functions:**
- Use Rust `snake_case` for free functions, methods, and test names, for example `load_installed_module_graph`, `validate_relative_path`, `backend_launch_candidates_from_graph`, and `installed_module_graph_rejects_active_provider_interface_mismatch`.
- Use predicate-style names for boolean checks, for example `binary_exists`, `json_value_matches_contract_type`, and `is_runtime_metadata_state_field` in `crates/core/shell/src/shell/mod.rs`.
- Use descriptive Rust test names that state behavior, not implementation mechanics, for example `module_manifest_loader_prefers_package_json_over_plugin_json` and `installed_module_graph_keeps_multiple_audio_providers` in `crates/core/extension/plugin/src/package.rs`.
- Use Luau `local function snake_case(...)` for helpers and public lifecycle hooks named by runtime contract, for example `init`, `on_poll`, and `on_command_set_volume` in `modules/backend/pipewire-audio/src/main.luau`.
- Use frontend component handlers with `onXxx` names inside `.mesh` scripts, for example `onRender` and `onVolumeClick` in `modules/frontend/navigation-bar/src/components/volume-button.mesh`.

**Variables:**
- Use Rust `snake_case` for locals and struct fields. Public serialized Rust fields use serde attributes when external JSON requires camelCase or kebab-case, for example `schema_version` with `#[serde(rename_all = "camelCase")]` in `crates/core/extension/plugin/src/package.rs`.
- Use Rust `HashMap` keys and IDs as canonical strings for module IDs and interface names, for example `@mesh/pipewire-audio` and `mesh.audio` in `crates/core/extension/plugin/src/package.rs`.
- Use Luau `snake_case` for globals that are reactive template state, for example `icon_name`, `audio_tooltip`, and `audio_percent` in `modules/frontend/navigation-bar/src/components/volume-button.mesh`.
- Use all-caps Luau constants for fixed values, for example `SOUNDS_ROOT` in `modules/backend/pipewire-audio/src/main.luau`.

**Types:**
- Use Rust `PascalCase` for structs, enums, traits, and enum variants, for example `ModulePackageManifest`, `MeshModuleSection`, `InstalledModuleGraph`, `ModuleKind::IconPack`, and `PackageManifestError`.
- Use explicit error enums ending in `Error` with `thiserror::Error`, for example `PackageManifestError` in `crates/core/extension/plugin/src/package.rs`, `ManifestError` in `crates/core/extension/plugin/src/manifest.rs`, and `ParseError` in `crates/core/ui/component/src/parser.rs`.
- Use domain node/record suffixes for graph output types, for example `InstalledModuleNode`, `BackendProviderNode`, `InterfaceGuidanceRecord`, and `ContributedLibrary` in `crates/core/extension/plugin/src/package.rs`.

## Code Style

**Formatting:**
- Use standard Rust formatting from `rustfmt`; no repository `rustfmt.toml` is present. The Nix dev shell provides `rustfmt` through `flake.nix`.
- Use 4-space indentation in Rust, Luau, `.mesh` templates, and `.mesh` styles as shown in `crates/core/ui/component/src/parser.rs`, `modules/backend/pipewire-audio/src/main.luau`, and `modules/frontend/navigation-bar/src/main.mesh`.
- Use 2-space indentation in JSON manifests, themes, settings, and docs examples, for example `config/package.json` and `config/modules/@mesh/pipewire-audio/package.json`.
- Prefer trailing commas in multi-line Rust struct literals, enum variants, arrays, and match arms, for example `MeshModuleSection` literals in `crates/core/extension/plugin/src/package.rs`.
- Keep `.mesh` files ordered as `<template>`, `<script lang="luau">`, then `<style>`, matching `modules/frontend/navigation-bar/src/main.mesh` and parser expectations in `crates/core/ui/component/src/parser.rs`.

**Linting:**
- Rust linting is via `clippy` from `flake.nix`; no workspace-level custom `clippy.toml` is present.
- No ESLint, Prettier, Biome, Jest, Vitest, or Node package manager lockfile is present. Do not introduce JavaScript tooling for the current Rust/Luau codebase without a concrete requirement.
- Run quality commands from the Nix shell when Wayland-related crates are involved because `smithay-client-toolkit` needs `xkbcommon.pc`: `nix develop -c cargo fmt --check`, `nix develop -c cargo clippy --workspace --all-targets`, and `nix develop -c cargo test --workspace`.

## Import Organization

**Order:**
1. External and internal crate imports at the top of Rust files, grouped by crate and alphabetized loosely by origin, as in `crates/core/shell/src/shell/mod.rs`.
2. Standard library imports after crate imports, for example `std::collections::HashMap` and `std::path::{Path, PathBuf}` in `crates/core/extension/plugin/src/package.rs`.
3. Local module declarations with `mod ...;`, then local `use` imports from those modules, as in `crates/core/shell/src/shell/mod.rs`.
4. Test modules import parent scope with `use super::*;` and add test-only imports inside `#[cfg(test)] mod tests`, as in `crates/core/extension/plugin/src/package.rs`.

**Path Aliases:**
- Rust uses workspace crate names rather than path aliases, for example `mesh_core_plugin::package::load_installed_module_graph` in `crates/core/shell/src/shell/mod.rs`.
- `.mesh` scripts use component imports relative to the file, for example `import VolumeButton from "./components/volume-button.mesh"` in `modules/frontend/navigation-bar/src/main.mesh`.
- Luau service consumers require interface proxies by interface alias and version, for example `pcall(require, "@mesh/audio@>=1.0")` in `modules/frontend/navigation-bar/src/components/volume-button.mesh`.
- Luau backend scripts access host APIs through the `mesh` global only, for example `mesh.exec`, `mesh.service.payload`, `mesh.service.set_poll_interval`, and `mesh.log` in `modules/backend/pipewire-audio/src/main.luau`.

## Error Handling

**Patterns:**
- Return `Result<T, DomainError>` in Rust APIs that read files, parse manifests, validate package graphs, compile components, or interact with runtime state. Use typed `thiserror` enums with source fields for I/O and parse failures, for example `PackageManifestError::Io` and `PackageManifestError::Json` in `crates/core/extension/plugin/src/package.rs`.
- Use explicit validation errors with actionable strings, for example `mesh.apiVersion cannot be empty`, `modulesDir must be a relative path`, and `layout entrypoint must use <module-id>:<entrypoint-id>` in `crates/core/extension/plugin/src/package.rs`.
- Convert legacy manifest loader failures into package-layer errors instead of leaking lower-level details, using `PackageManifestError::LegacyManifest` in `crates/core/extension/plugin/src/package.rs`.
- Use `matches!` and `unwrap_err()` in tests for expected failures, for example `module_package_paths_reject_relative_mesh_home` and `require_missing_interface_emits_visible_diagnostic`.
- Use `unwrap()` and `expect()` freely in tests and setup helpers, but avoid them in production parsing/loading paths except for internal locks or runtime invariants already enforced by construction. Production examples with structured errors live in `crates/core/extension/plugin/src/package.rs` and `crates/core/ui/component/src/parser.rs`.
- Luau backend command handlers return `{ ok = false, error = "..." }` for expected service failures and refresh state before returning where state may have changed, as in `modules/backend/pipewire-audio/src/main.luau`.

## Logging

**Framework:** `tracing` in Rust; host-provided `mesh.log` in Luau; `eprintln!`/`println!` only for CLI user output.

**Patterns:**
- Use `tracing::info!`, `tracing::debug!`, `tracing::warn!`, and `tracing::error!` in long-running shell/runtime code, for example `crates/core/shell/src/shell/mod.rs` and `crates/core/frontend/compiler/src/compile.rs`.
- Initialize CLI logging once with `tracing_subscriber::fmt().with_env_filter(...)` in `crates/tools/cli/src/main.rs`.
- Use diagnostic collectors for user-visible plugin health rather than only logging, for example `Diagnostics::record_lifecycle_error` in `crates/core/foundation/diagnostics/src/lib.rs`.
- Luau backends should log through small local helpers that prefix the module ID, for example `debug_info` and `debug_warn` in `modules/backend/pipewire-audio/src/main.luau`.

## Comments

**When to Comment:**
- Add module-level comments for public crate responsibilities and format boundaries, for example `crates/core/extension/plugin/src/manifest.rs`, `crates/core/ui/component/src/parser.rs`, and `crates/core/foundation/diagnostics/src/lib.rs`.
- Add short comments before non-obvious compatibility or deduplication behavior, for example legacy service declarations in `crates/core/extension/plugin/src/manifest.rs` and lifecycle deduplication in `crates/core/foundation/diagnostics/src/lib.rs`.
- Avoid comments for straightforward field assignment or simple control flow.

**JSDoc/TSDoc:**
- Not applicable. The codebase is Rust, Luau, JSON, TOML, and `.mesh`, with no TypeScript source.
- Rust doc comments (`///`) are used for public data types and APIs that define contracts, for example `Manifest::declared_provides`, `Diagnostics`, and `HealthStatus`.

## Function Design

**Size:** Keep new Rust functions focused and testable. Large existing orchestration files such as `crates/core/shell/src/shell/mod.rs` should be extended by adding targeted helper functions near related logic, not by burying new behavior in the main loop.

**Parameters:** Prefer typed domain structs and borrowed paths/strings over ad hoc JSON. Use `&Path` for filesystem inputs, `&str` for IDs and interface names, and `serde_json::Value` only at manifest, settings, and runtime payload boundaries, as in `RootPackageManifest::from_path`, `load_module_manifest`, and `dispatch_service_command`.

**Return Values:** Use typed structs for graph/query outputs, for example `ResolvedLayoutEntrypoint`, `ContributedTheme`, `FrontendRequirementSet`, and `BackendProviderNode` in `crates/core/extension/plugin/src/package.rs`. Return `Option<T>` for missing optional graph entries such as `active_provider` and `layout_entrypoint`.

## Module Design

**Exports:** Rust crates expose public APIs from `src/lib.rs` or module files. Keep public structs and enums near their validation/conversion implementations, as in `crates/core/extension/plugin/src/package.rs` and `crates/core/extension/plugin/src/manifest.rs`.

**Barrel Files:** Crate `lib.rs` files act as public module surfaces, for example `crates/core/extension/plugin/src/lib.rs`, `crates/core/shell/src/lib.rs`, and `crates/core/ui/component/src/lib.rs`. Do not add broad re-export barrels unless the crate already exposes that abstraction.

**Module Manifest Conventions:**
- New modules must use npm-compatible `package.json` with ordinary package metadata at the top level and all MESH-specific declarations under `mesh`, as documented in `docs/module-system.md`, `README.md`, and `docs/plugins/README.md`.
- Use top-level `name`, not top-level `id`, for module identity. Use `mesh.kind`, not top-level `type`, for module role. Use `mesh.dependencies`, not top-level `dependencies`, for MESH dependency objects. These conventions are documented in `docs/module-system.md`.
- Use the allowed `mesh.kind` values defined by `ModuleKind` in `crates/core/extension/plugin/src/package.rs`: `frontend`, `backend`, `theme`, `icon-pack`, `font-pack`, `language-pack`, `interface`, and `library`.
- Root installed-module graphs live in `config/package.json` with `mesh.schemaVersion`, `mesh.modulesDir`, `mesh.modules`, `mesh.providers`, and `mesh.layout`. This root graph selects active providers such as `mesh.audio` and layout entrypoints such as `@mesh/navigation-bar:main`.
- Module package fixtures live under `config/modules/@mesh/*/package.json`. Keep these fixtures in sync with `docs/module-system.md` examples and graph tests in `crates/core/extension/plugin/src/package.rs`.
- Legacy `package.json`, `plugin.json`, and `mesh.toml` are compatibility inputs only. Loader precedence is encoded in `load_module_manifest` in `crates/core/extension/plugin/src/package.rs`: `package.json` compatibility path, then `package.json`, then `plugin.json`. Tests also assert `package.json` wins over `plugin.json` when both are present.
- Frontends consume interfaces through `mesh.dependencies.backend` and Luau `require("@mesh/<service>@<version>")`; they must not depend on backend module IDs. Backend providers declare `mesh.implements` entries with `interface`, `provider`, `label`, and `priority`, as in `config/modules/@mesh/pipewire-audio/package.json`.
- Libraries are modules with `mesh.kind = "library"` and `mesh.contributes.libraries`. Validate contribution paths as relative paths without `..`, matching `installed_module_graph_rejects_library_path_escape` in `crates/core/extension/plugin/src/package.rs`.

**Component/Script Conventions:**
- Backend `main.luau` files expose `init()` explicitly and register polling inside `init()` rather than relying on top-level side effects, as required by `CLAUDE.md` and implemented in `modules/backend/pipewire-audio/src/main.luau`.
- Frontend `.mesh` components should compute display-specific labels/icons in the component script, not in Rust service payloads. This is documented in `docs/llm-context.md` and visible in `modules/frontend/navigation-bar/src/components/volume-button.mesh`.
- `.mesh` templates use MESH elements such as `row`, `box`, `button`, `icon`, and `text`; removed HTML compatibility tags such as `<div>` are rejected by tests in `crates/core/ui/component/src/parser.rs`.

---

*Convention analysis: 2026-05-06*
