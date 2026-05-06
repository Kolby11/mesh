# Phase 1: Plugin Package Manifest Foundation - Context

**Gathered:** 2026-05-03
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase delivers the shell-owned `~/.mesh/package.json` installed-module manifest and normalized module graph that later backend lifecycle, provider selection, and base shell layout work will consume. It also establishes the user-facing `~/.mesh` configuration shape. It does not implement remote download, marketplace, signing, sandboxing, or full hot-install behavior.

</domain>

<decisions>
## Implementation Decisions

### Package Manifest Comes First
- **D-01:** The user pivoted Phase 1 away from backend lifecycle internals and toward plugin installation/package structure. The central package manifest is the first capability to implement because it gives the rest of backend MVP work one unified interface to work from.
- **D-02:** The central shell-owned manifest filename is now locked as `~/.mesh/package.json`.
- **D-03:** The shell package manifest should include dependency buckets for installed frontend modules, backend modules, icon packs, fonts, and i18n/language support. User wording: fields like `frontendDependencies`, `backendDependencies`, `icons`, `fonts`, and i18n support.
- **D-04:** The shell-owned package manifest is installed state, not a replacement for each module's own manifest. It should reference installed module IDs, enabled state, provider choices, and local module paths while reusing the existing manifest model for package metadata, dependencies, capabilities, entrypoints, and provider declarations.
- **D-05:** The package graph should be normalized from two inputs: the shell-owned `~/.mesh/package.json` installed-state manifest and each referenced module's existing normalized `Manifest`.
- **D-06:** The manifest should be designed so the shell can find an entrypoint that defines the base shell layout from installed frontend modules. The exact key name and entrypoint representation are planner/researcher decisions, but the concept is locked.

### Module Naming Convention
- **D-07:** User-facing naming should change from "plugins" to "modules" because modules better describes the extensibility model. Downstream work should research and plan the migration impact rather than continuing to expand product-facing "plugin" terminology.
- **D-08:** Existing Rust/code internals may still use `Plugin*` names during a compatibility transition if a full rename is too broad for Phase 1, but new user-visible config structure, docs, and schema concepts should point toward modules.
- **D-09:** Installed modules live under `~/.mesh/modules/`.
- **D-10:** Each installed module should have its own `package.json` manifest. This replaces `plugin.json` as the target user-facing package name, though compatibility with existing `plugin.json` fixtures may be needed during migration.

### Module Dependency Model
- **D-11:** Frontend modules should declare which underlying backend modules or backend service categories they require.
- **D-12:** Installing or enabling a frontend module should make its backend dependencies visible to the shell package graph.
- **D-13:** Backend modules are not expected to be manually installed by users as often as frontend modules, but users must still be able to install backend-only modules such as a shortcuts provider.
- **D-14:** Existing dependency declarations such as `dependencies.plugins` on interface packages and `provides` on backend packages should be treated as source material for dependency/provider resolution rather than inventing an unrelated dependency vocabulary in Phase 1. Research/planning should decide the compatibility bridge to module naming.

### Backend Categories and Provider Selection
- **D-15:** Backend modules declare their own category/service, such as `audio`, `network`, `power`, `media`, or `shortcuts`.
- **D-16:** If the user has multiple backend modules in the same category, the shell should be able to present or record which provider is active.
- **D-17:** Provider activation should remain compatible with the earlier hybrid decision: highest-priority/default provider can be used automatically, while an explicit user choice overrides it.
- **D-18:** The package graph should feed shell core settings/provider organization so the shell can reorganize installed modules into active category choices.
- **D-19:** The current priority-based provider fallback in shell startup is a compatibility behavior to preserve, but Phase 1 should make the selected provider visible in the normalized package graph for later lifecycle work.

### User Config Layout
- **D-20:** The shell's general user-owned files should live under `~/.mesh`, not the current documented `~/.config/mesh` shape.
- **D-21:** `~/.mesh/settings.json` should hold shell settings such as active theme, locale/i18n, and other global shell settings.
- **D-22:** `~/.mesh/themes/` should hold specific color theme files that users can switch between.
- **D-23:** The installed package graph should account for icon packs, fonts, and i18n assets as first-class package dependencies or resources, not as unrelated ad hoc files.
- **D-24:** Each module should record a Git origin in its own package metadata or installed-state entry. Phase 1 may store and validate this origin as metadata, but it must not implement Git download/install yet.

### Scope Boundary
- **D-25:** Phase 1 should build the local installed-module/package graph foundation only. Remote download, dependency fetching from registries, package signing, sandboxing, and marketplace UX belong to later phases.
- **D-26:** Phase 2 backend lifecycle should consume the graph created here rather than continuing to rely only on implicit directory scanning and provider priority.
- **D-27:** The module rename is a strategic naming direction. If the full codebase rename is too large for Phase 1, Phase 1 should still establish schema/config compatibility boundaries so later phases can complete the rename without breaking the package graph.

### the agent's Discretion
- Planner may choose Rust module boundaries after reading existing config/manifest patterns.
- Planner may choose the minimal schema shape, but it must represent frontend modules, backend modules, backend categories, dependencies, active provider choices, explicit backend-only installs, Git origin metadata, and a base shell layout entrypoint.
- Planner may choose the exact error type and validation layering, but invalid installed package entries should become typed package-graph errors suitable for later diagnostics rather than silent skips.
- Researcher/planner should decide whether existing `plugin.json` support remains as a legacy alias, whether module manifests are accepted from both `package.json` and `plugin.json`, and how much naming migration belongs in Phase 1.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone Scope
- `.planning/PROJECT.md` — Active v1.1 milestone intent and package-manifest-first pivot.
- `.planning/REQUIREMENTS.md` — PINST requirements and traceability.
- `.planning/ROADMAP.md` — Phase 1 scope and phase ordering.

### Existing Codebase Maps
- `.planning/codebase/ARCHITECTURE.md` — Shell/plugin/runtime layering and current backend event flow.
- `.planning/codebase/INTEGRATIONS.md` — Existing backend plugin categories and providers.
- `.planning/codebase/STACK.md` — Rust/Luau/Tokio/mlua stack constraints.

### Existing Plugin Manifests and Runtime Code
- `crates/core/extension/plugin/src/manifest.rs` — Current normalized plugin manifest model, `PluginType`, `ProvidedInterface`, `ServiceSection`, dependencies, entrypoints.
- `crates/core/shell/src/shell/mod.rs` — Current backend plugin discovery/spawn path and provider selection.
- `crates/core/runtime/backend/src/lib.rs` — Current backend service task loop.
- `crates/core/foundation/config/src/lib.rs` — Current config path loading for shell settings; conflicts with the new `~/.mesh` target and must be reviewed.
- `crates/core/foundation/theme/src/lib.rs` — Current theme directory loading; conflicts with the new `~/.mesh/themes/` target and must be reviewed.
- `crates/core/shell/src/shell/component.rs` — Current per-plugin settings and `config/i18n` loading; relevant to module naming and i18n package graph support.
- `packages/plugins/frontend/core/panel/plugin.json` — Example frontend plugin manifest.
- `packages/plugins/frontend/core/quick-settings/plugin.json` — Example frontend plugin manifest with backend service usage.
- `packages/plugins/backend/core/pipewire-audio/plugin.json` — Example backend provider plugin.
- `packages/plugins/backend/core/pulseaudio-audio/plugin.json` — Alternative backend provider in same conceptual category.
- `packages/plugins/backend/core/shell-theme/plugin.json` — Simple backend provider example.

### Config and Theme Docs to Reconcile
- `docs/settings/README.md` — Current settings design documents `~/.config/mesh/settings.json`; Phase 1 should research/update this against the locked `~/.mesh/settings.json` direction.
- `docs/theming/themes.md` — Current theme-package model and mode switching; Phase 1 should reconcile this with `~/.mesh/themes/` color theme files.
- `config/shell-settings.json` — Current shell settings example.
- `config/themes/mesh-default-dark.json` — Current default theme token file.
- `config/themes/mesh-default-light.json` — Current default theme token file.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `Manifest` / `ProvidedInterface` / `DependenciesSection` in `crates/core/extension/plugin/src/manifest.rs`: use as source material for package graph schema and normalized plugin metadata.
- Existing `plugin.json` files under `packages/plugins/frontend/**` and `packages/plugins/backend/**`: use as fixtures for package graph parsing tests.
- Existing shell settings config loader patterns: use for locating and parsing the central shell-owned package manifest.
- Existing `dependencies.plugins` entries in frontend manifests such as `packages/plugins/frontend/core/quick-settings/plugin.json`: reuse as examples of frontend-to-interface dependency declarations.
- Existing `provides` entries in backend manifests such as `packages/plugins/backend/core/pipewire-audio/plugin.json` and `packages/plugins/backend/core/pulseaudio-audio/plugin.json`: reuse as examples of multiple providers for the same backend category.
- Existing config docs and loaders already separate shell settings, per-plugin settings, themes, icon packs, and i18n. Phase 1 should consolidate these into the `~/.mesh` package/config layout instead of inventing every piece from scratch.

### Established Patterns
- Plugin manifests are JSON/TOML normalized into typed Rust structs before shell use.
- Backend providers currently declare interfaces through `provides` or legacy `service` sections.
- Shell currently scans plugin directories and selects highest-priority backend provider per service.
- Rust core should remain generic across services; service-specific behavior belongs in Luau backend plugins.
- Missing native binaries are currently skipped during backend spawn; package graph validation should stay generic and leave runtime availability handling to later lifecycle/diagnostics phases unless a manifest entry itself is invalid.
- Current docs use `~/.config/mesh` while the new desired product convention is `~/.mesh`. This is a deliberate direction change, not an accidental mismatch.
- Current source paths and types use "plugin" heavily. The module rename is a cross-cutting migration risk that needs explicit planning.

### Integration Points
- Package graph parsing likely belongs near `mesh-core-plugin` or shell config code, depending on whether it is treated as extension metadata or shell-owned installed state.
- Shell startup should eventually consume the package graph before backend lifecycle spawning.
- Later provider selection should connect package graph category choices to `spawn_backend_plugins()`.
- Current `spawn_backend_plugins()` grouping by normalized service name is the immediate consumer that Phase 2 should replace or feed from the installed package graph.
- Shell settings/theme loading should eventually read from `~/.mesh/settings.json` and `~/.mesh/themes/` rather than only repo-local `config/` or documented `~/.config/mesh` paths.
- Frontend composition/catalog code is the likely integration point for the base shell layout entrypoint because it already compiles and embeds frontend surfaces/widgets from manifests.

</code_context>

<specifics>
## Specific Ideas

- The user described the target file as "a central package.json or something like that."
- The user later locked the general shell config root as `~/.mesh`.
- The central installed manifest should be `~/.mesh/package.json`.
- The user wants "modules" terminology instead of "plugins".
- Installed modules should live in `~/.mesh/modules/`.
- Each module should specify its own `package.json`.
- Each module should specify a Git origin from which it can eventually be downloaded.
- `~/.mesh/settings.json` should specify shell settings.
- `~/.mesh/themes/` should contain switchable color themes.
- The package/config shape needs an entrypoint that defines the base shell layout from installed modules.
- The shell package manifest should contain user-specified frontend plugins.
- Frontend plugins specify underlying backend dependencies.
- Backend plugins specify their own category, such as `audio`.
- If two backend plugins exist in the same category, the user can choose which one to use.
- Backend-only plugins should still be installable for cases like shortcuts.

</specifics>

<deferred>
## Deferred Ideas

- Remote plugin download/install workflow.
- Git clone/fetch/install behavior for module origins.
- Dependency fetching from external package registries.
- Package signing and sandboxing.
- Marketplace or plugin browser UI.
- Full hot-install/reload UX after file download.

</deferred>

---

*Phase: 1-Plugin Package Manifest Foundation*
*Context gathered: 2026-05-03*
