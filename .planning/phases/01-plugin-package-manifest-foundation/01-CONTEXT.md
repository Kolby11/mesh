# Phase 1: Plugin Package Manifest Foundation - Context

**Gathered:** 2026-05-03
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase delivers the shell-owned package.json-like installed-plugin manifest and normalized plugin graph that later backend lifecycle and provider selection work will consume. It does not implement remote download, marketplace, signing, sandboxing, or full hot-install behavior.

</domain>

<decisions>
## Implementation Decisions

### Package Manifest Comes First
- **D-01:** The user pivoted Phase 1 away from backend lifecycle internals and toward plugin installation/package structure. The central package manifest is the first capability to implement because it gives the rest of backend MVP work one unified interface to work from.
- **D-02:** The manifest should be package.json-like: a single shell-owned file that lists user-selected frontend plugins and backend plugins.
- **D-03:** The exact filename is not locked. Planner should choose a name that fits existing config conventions, but it must be conceptually central and package-manifest-like rather than scattered per-plugin state.

### Plugin Dependency Model
- **D-04:** Frontend plugins should declare which underlying backend plugins or backend service categories they require.
- **D-05:** Installing or enabling a frontend plugin should make its backend dependencies visible to the shell package graph.
- **D-06:** Backend plugins are not expected to be manually installed by users as often as frontend plugins, but users must still be able to install backend-only plugins such as a shortcuts provider.

### Backend Categories and Provider Selection
- **D-07:** Backend plugins declare their own category/service, such as `audio`, `network`, `power`, `media`, or `shortcuts`.
- **D-08:** If the user has multiple backend plugins in the same category, the shell should be able to present or record which provider is active.
- **D-09:** Provider activation should remain compatible with the earlier hybrid decision: highest-priority/default provider can be used automatically, while an explicit user choice overrides it.
- **D-10:** The package graph should feed shell core settings/provider organization so the shell can reorganize installed plugins into active category choices.

### Scope Boundary
- **D-11:** Phase 1 should build the local installed-plugin/package graph foundation only. Remote download, dependency fetching from registries, package signing, sandboxing, and marketplace UX belong to later phases.
- **D-12:** Phase 2 backend lifecycle should consume the graph created here rather than continuing to rely only on implicit directory scanning and provider priority.

### the agent's Discretion
- Planner may choose the concrete manifest filename and Rust module boundaries after reading existing config/manifest patterns.
- Planner may choose the minimal schema shape, but it must represent frontend plugins, backend plugins, backend categories, dependencies, and active provider choices.

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
- `packages/plugins/frontend/core/panel/plugin.json` — Example frontend plugin manifest.
- `packages/plugins/frontend/core/quick-settings/plugin.json` — Example frontend plugin manifest with backend service usage.
- `packages/plugins/backend/core/pipewire-audio/plugin.json` — Example backend provider plugin.
- `packages/plugins/backend/core/pulseaudio-audio/plugin.json` — Alternative backend provider in same conceptual category.
- `packages/plugins/backend/core/shell-theme/plugin.json` — Simple backend provider example.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `Manifest` / `ProvidedInterface` / `DependenciesSection` in `crates/core/extension/plugin/src/manifest.rs`: use as source material for package graph schema and normalized plugin metadata.
- Existing `plugin.json` files under `packages/plugins/frontend/**` and `packages/plugins/backend/**`: use as fixtures for package graph parsing tests.
- Existing shell settings config loader patterns: use for locating and parsing the central shell-owned package manifest.

### Established Patterns
- Plugin manifests are JSON/TOML normalized into typed Rust structs before shell use.
- Backend providers currently declare interfaces through `provides` or legacy `service` sections.
- Shell currently scans plugin directories and selects highest-priority backend provider per service.
- Rust core should remain generic across services; service-specific behavior belongs in Luau backend plugins.

### Integration Points
- Package graph parsing likely belongs near `mesh-core-plugin` or shell config code, depending on whether it is treated as extension metadata or shell-owned installed state.
- Shell startup should eventually consume the package graph before backend lifecycle spawning.
- Later provider selection should connect package graph category choices to `spawn_backend_plugins()`.

</code_context>

<specifics>
## Specific Ideas

- The user described the target file as "a central package.json or something like that."
- The shell package manifest should contain user-specified frontend plugins.
- Frontend plugins specify underlying backend dependencies.
- Backend plugins specify their own category, such as `audio`.
- If two backend plugins exist in the same category, the user can choose which one to use.
- Backend-only plugins should still be installable for cases like shortcuts.

</specifics>

<deferred>
## Deferred Ideas

- Remote plugin download/install workflow.
- Dependency fetching from external package registries.
- Package signing and sandboxing.
- Marketplace or plugin browser UI.
- Full hot-install/reload UX after file download.

</deferred>

---

*Phase: 1-Plugin Package Manifest Foundation*
*Context gathered: 2026-05-03*
