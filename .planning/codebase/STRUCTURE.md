# Codebase Structure

**Analysis Date:** 2026-05-06

## Directory Layout

```text
mesh/
├── Cargo.toml                    # Rust workspace manifest for core crates and tools
├── Cargo.lock                    # Rust dependency lockfile
├── README.md                     # Project overview and package-system summary
├── CLAUDE.md                     # Agent/developer guidance
├── ARCHITECTURE_REFACTORING.md   # Architecture refactoring notes
├── TEXT_RENDERING_TODO.md        # Text rendering work notes
├── config/                       # Local runtime config, package graph, themes, catalog manifests
│   ├── package.json              # Root installed-module graph
│   ├── shell-settings.json       # User-facing shell settings used in repo/dev runs
│   ├── settings-default.json     # Bundled default shell settings
│   ├── icons.toml                # Icon configuration
│   ├── themes/                   # Theme token JSON files
│   └── modules/@mesh/*/          # Package-shaped bundled/catalog module manifests
├── crates/                       # Rust workspace crates
│   ├── core/extension/           # Module/plugin manifests and service contracts
│   ├── core/foundation/          # Capability, config, diagnostics, events, locale, theme, debug
│   ├── core/platform/            # Wayland platform abstraction
│   ├── core/runtime/             # Luau runtime, backend runtime, host runtime
│   ├── core/shell/               # Main shell orchestration and component host
│   ├── core/ui/                  # Component parser, element model, icons, renderer
│   └── tools/                    # CLI and LSP binaries
├── docs/                         # Architecture, module-system, frontend, plugin, settings, theming docs
├── modules/                      # Source modules loaded by `config/package.json`
│   ├── backend/                  # Backend provider modules
│   ├── frontend/                 # Frontend surface/widget modules
│   └── interfaces/               # Interface contract TOML files
├── spec/                         # Pluggable backend spec
├── tools/                        # Miscellaneous repo tooling
└── .planning/                    # GSD project planning and codebase maps
```

## Directory Purposes

**`crates/core/extension/plugin`:**
- Purpose: Own module/package and compatibility manifest loading.
- Contains: `Manifest`, legacy `plugin.json`/`mesh.toml` normalization, `RootPackageManifest`, `ModulePackageManifest`, `InstalledModuleGraph`.
- Key files: `crates/core/extension/plugin/src/package.rs`, `crates/core/extension/plugin/src/manifest.rs`, `crates/core/extension/plugin/src/lifecycle.rs`.

**`crates/core/extension/service`:**
- Purpose: Own interface contracts, interface/provider registry, and compatibility typed service registry.
- Contains: TOML contract loader, semver parsing, provider resolution, catalog snapshots, legacy typed registry.
- Key files: `crates/core/extension/service/src/contract.rs`, `crates/core/extension/service/src/interface.rs`, `crates/core/extension/service/src/registry.rs`.

**`crates/core/foundation`:**
- Purpose: Shared foundation crates with narrow responsibilities.
- Contains: `capability`, `config`, `debug`, `diagnostics`, `events`, `locale`, `theme`.
- Key files: `crates/core/foundation/config/src/lib.rs`, `crates/core/foundation/capability/src/lib.rs`, `crates/core/foundation/theme/src/lib.rs`, `crates/core/foundation/locale/src/lib.rs`.

**`crates/core/platform/wayland`:**
- Purpose: Wayland surface/compositor abstraction.
- Contains: Layer, edge, shell surface, stub surface, and platform types.
- Key files: `crates/core/platform/wayland/src/lib.rs`.

**`crates/core/runtime/backend`:**
- Purpose: Execute backend Luau provider scripts and bridge them to shell service events.
- Contains: `BackendServiceCommand`, `BackendServiceUpdate`, `BackendServiceEvent`, `spawn_backend_service()`, poll/command dispatch.
- Key files: `crates/core/runtime/backend/src/lib.rs`.

**`crates/core/runtime/host`:**
- Purpose: Model sandbox runtime tiers and generic plugin runtime configuration.
- Contains: `SandboxConfig`, `ExecutionTier`, `PluginRuntime`.
- Key files: `crates/core/runtime/host/src/lib.rs`.

**`crates/core/runtime/scripting`:**
- Purpose: Host frontend/backend Luau contexts and install capability-gated host APIs.
- Contains: `ScriptContext`, `BackendScriptContext`, `HostApiManifest`, interface proxies, `mesh.*` APIs.
- Key files: `crates/core/runtime/scripting/src/context.rs`, `crates/core/runtime/scripting/src/backend.rs`, `crates/core/runtime/scripting/src/host_api.rs`.

**`crates/core/shell`:**
- Purpose: Main application runtime and orchestration layer.
- Contains: `Shell`, component host, frontend catalog, IPC server, layout/input/render runtime, service routing, backend provider selection, diagnostics.
- Key files: `crates/core/shell/src/lib.rs`, `crates/core/shell/src/shell/mod.rs`, `crates/core/shell/src/shell/component.rs`, `crates/core/shell/src/shell/component/catalog.rs`, `crates/core/shell/src/shell/service.rs`.

**`crates/core/ui/component`:**
- Purpose: Parse `.mesh` single-file components into typed ASTs.
- Contains: Parser modules for markup/script/style plus public component/template structs.
- Key files: `crates/core/ui/component/src/lib.rs`, `crates/core/ui/component/src/parser.rs`, `crates/core/ui/component/src/parser/markup.rs`, `crates/core/ui/component/src/parser/script.rs`, `crates/core/ui/component/src/parser/styles.rs`.

**`crates/core/ui/elements`:**
- Purpose: Represent runtime UI trees, layout, style, events, and accessibility primitives.
- Contains: Element definitions, widget tree, layout engine, style resolver, event model.
- Key files: `crates/core/ui/elements/src/lib.rs`, `crates/core/ui/elements/src/tree.rs`, `crates/core/ui/elements/src/layout.rs`, `crates/core/ui/elements/src/style.rs`.

**`crates/core/ui/icon`:**
- Purpose: Icon registry and bundled icon assets.
- Contains: XDG icon support, fallback registry, icon config, material SVG assets.
- Key files: `crates/core/ui/icon/src/lib.rs`, `crates/core/ui/icon/src/registry.rs`, `crates/core/ui/icon/src/config.rs`, `crates/core/ui/icon/assets/material/*.svg`.

**`crates/core/ui/render`:**
- Purpose: Compile frontend modules and paint widget trees to buffers/surfaces.
- Contains: Frontend compiler, render engine, software painter, text/icon/surface bridges, accessibility/debug overlay.
- Key files: `crates/core/ui/render/src/compile.rs`, `crates/core/ui/render/src/render.rs`, `crates/core/ui/render/src/surface/mod.rs`, `crates/core/ui/render/src/surface/bridge/wayland_surface.rs`.

**`crates/tools/cli`:**
- Purpose: Provide the `mesh-shell` command-line binary.
- Contains: Start/list/services/debug/ipc/status/version/help commands.
- Key files: `crates/tools/cli/src/main.rs`.

**`crates/tools/lsp`:**
- Purpose: Provide a language server for `.mesh` components.
- Contains: Diagnostics, hover, analyzer, backend, plugin registry, knowledge tables.
- Key files: `crates/tools/lsp/src/main.rs`, `crates/tools/lsp/src/lib.rs`, `crates/tools/lsp/src/analyzer/mod.rs`, `crates/tools/lsp/src/plugin_registry.rs`.

**`modules/backend`:**
- Purpose: Source backend provider modules used by the local root graph.
- Contains: `pipewire-audio` package manifest and Luau source, `pulseaudio-audio` compatibility `module.json` and Luau source, placeholder/partial backend directories.
- Key files: `modules/backend/pipewire-audio/package.json`, `modules/backend/pipewire-audio/src/main.luau`, `modules/backend/pulseaudio-audio/module.json`, `modules/backend/pulseaudio-audio/src/main.luau`.

**`modules/frontend`:**
- Purpose: Source frontend surface/widget modules used by local discovery and root graph.
- Contains: `navigation-bar` surface module with settings, translations, components, `.mesh` entrypoint.
- Key files: `modules/frontend/navigation-bar/module.json`, `modules/frontend/navigation-bar/src/main.mesh`, `modules/frontend/navigation-bar/src/components/*.mesh`, `modules/frontend/navigation-bar/config/settings.json`.

**`modules/interfaces`:**
- Purpose: Interface contract TOML files for service APIs.
- Contains: Audio state fields, methods, events, types, capabilities.
- Key files: `modules/interfaces/audio.toml`.

**`config`:**
- Purpose: Runtime config and bundled package/catalog data for local runs.
- Contains: Root module graph, shell settings, default settings, themes, icon config, package-shaped catalog manifests.
- Key files: `config/package.json`, `config/shell-settings.json`, `config/settings-default.json`, `config/themes/mesh-default-dark.json`, `config/themes/mesh-default-light.json`, `config/modules/@mesh/*/package.json`.

**`docs`:**
- Purpose: Human-facing architecture and authoring documentation.
- Contains: Module system, extensibility, installation, health, frontend syntax, slots, theming, settings, plugin indexes.
- Key files: `docs/module-system.md`, `docs/extensibility.md`, `docs/frontend/mesh-syntax.md`, `docs/settings/README.md`, `docs/theming/themes.md`, `docs/plugins/README.md`.

**`spec`:**
- Purpose: Deeper lifecycle/security/backend design references.
- Contains: Pluggable backend specification.
- Key files: `spec/pluggable-backend.md`.

## Key File Locations

**Entry Points:**
- `crates/tools/cli/src/main.rs`: Binary entrypoint for `mesh-shell`.
- `crates/core/shell/src/lib.rs`: Public shell crate exports.
- `crates/core/shell/src/shell/mod.rs`: Main shell runtime entrypoint and event loop.
- `crates/tools/lsp/src/main.rs`: LSP binary entrypoint.
- `modules/frontend/navigation-bar/src/main.mesh`: Navigation bar surface entrypoint.
- `modules/backend/pipewire-audio/src/main.luau`: PipeWire audio provider script entrypoint.
- `modules/backend/pulseaudio-audio/src/main.luau`: PulseAudio audio provider script entrypoint.

**Configuration:**
- `Cargo.toml`: Workspace members, workspace package metadata, shared dependencies.
- `config/package.json`: Root installed-module graph loaded by backend provider launch.
- `config/shell-settings.json`: Repo-local shell settings path used by `default_settings_path()` when present.
- `config/settings-default.json`: Default shell settings loaded before user overrides.
- `config/icons.toml`: Icon configuration.
- `config/themes/mesh-default-dark.json`: Dark theme tokens.
- `config/themes/mesh-default-light.json`: Light theme tokens.
- `modules/frontend/navigation-bar/config/settings.json`: Frontend surface settings for navigation bar.
- `flake.nix`: Nix development shell/package setup.

**Core Logic:**
- `crates/core/extension/plugin/src/package.rs`: Target package/module graph model.
- `crates/core/extension/plugin/src/manifest.rs`: Runtime manifest normalization and compatibility loaders.
- `crates/core/extension/service/src/contract.rs`: Interface contract parser.
- `crates/core/extension/service/src/interface.rs`: Interface catalog/registry.
- `crates/core/runtime/scripting/src/context.rs`: Frontend Luau host, interface proxy, `require()` behavior.
- `crates/core/runtime/scripting/src/backend.rs`: Backend Luau host behavior.
- `crates/core/runtime/backend/src/lib.rs`: Backend service orchestration loop.
- `crates/core/shell/src/shell/mod.rs`: Shell lifecycle and backend launch flow.
- `crates/core/shell/src/shell/component.rs`: Frontend component runtime.
- `crates/core/ui/render/src/compile.rs`: Frontend module compiler.
- `crates/core/ui/component/src/parser.rs`: `.mesh` parser entrypoint.

**Module System:**
- `docs/module-system.md`: Authoritative target module/package architecture.
- `config/package.json`: Active installed-module graph.
- `config/modules/@mesh/pipewire-audio/package.json`: Package-shaped bundled/catalog provider manifest.
- `config/modules/@mesh/pulseaudio-audio/package.json`: Package-shaped bundled/catalog provider manifest.
- `config/modules/@mesh/panel/package.json`: Package-shaped bundled/catalog frontend manifest.
- `config/modules/@mesh/quick-settings/package.json`: Package-shaped bundled/catalog frontend manifest.
- `config/modules/@mesh/networkmanager/package.json`: Package-shaped bundled/catalog provider manifest.
- `config/modules/@mesh/upower/package.json`: Package-shaped bundled/catalog provider manifest.
- `config/modules/@mesh/shell-theme/package.json`: Package-shaped bundled/catalog resource module manifest.
- `modules/backend/pipewire-audio/package.json`: Source backend package manifest used by `config/package.json`.
- `modules/backend/pulseaudio-audio/module.json`: Compatibility backend manifest used by `config/package.json`.
- `modules/frontend/navigation-bar/module.json`: Compatibility frontend manifest used by `config/package.json`.

**Testing:**
- Tests are colocated in Rust source files under `#[cfg(test)]`, especially `crates/core/extension/plugin/src/package.rs`, `crates/core/extension/plugin/src/manifest.rs`, `crates/core/extension/service/src/contract.rs`, `crates/core/runtime/scripting/src/context.rs`, `crates/core/runtime/backend/src/lib.rs`, and `crates/core/shell/src/shell/component/tests.rs`.

## Naming Conventions

**Files:**
- Rust source uses `snake_case.rs`: `crates/core/shell/src/shell/surface_layout.rs`, `crates/core/runtime/scripting/src/host_api.rs`.
- Rust crate manifests are `Cargo.toml` at each crate root: `crates/core/shell/Cargo.toml`, `crates/tools/cli/Cargo.toml`.
- Module manifests use `package.json` for the target package model: `modules/backend/pipewire-audio/package.json`, `config/modules/@mesh/panel/package.json`.
- Compatibility module manifests use `module.json`: `modules/frontend/navigation-bar/module.json`, `modules/backend/pulseaudio-audio/module.json`.
- Frontend components use kebab-case `.mesh`: `modules/frontend/navigation-bar/src/components/settings-button.mesh`.
- Backend scripts use `src/main.luau`: `modules/backend/pipewire-audio/src/main.luau`.
- Interface contracts use domain TOML names: `modules/interfaces/audio.toml`.
- Theme files use kebab-case JSON names: `config/themes/mesh-default-dark.json`.

**Directories:**
- Rust crates use domain grouping under `crates/core/{foundation,extension,runtime,shell,ui,platform}`.
- Frontend modules use `modules/frontend/<module-name>/`.
- Backend modules use `modules/backend/<provider-name>/`.
- Frontend local components live under `src/components/` inside a module: `modules/frontend/navigation-bar/src/components/`.
- Package-shaped catalog manifests use npm scope directories: `config/modules/@mesh/<module-name>/`.
- Documentation groups by topic: `docs/frontend/`, `docs/plugins/`, `docs/settings/`, `docs/theming/`.

**Rust Symbols:**
- Public types use PascalCase: `Shell`, `InstalledModuleGraph`, `ModulePackageManifest`, `ScriptContext`.
- Functions and methods use snake_case: `load_installed_module_graph`, `spawn_backend_plugins`, `compile_frontend_plugin`.
- Crates use hyphenated package names and underscore Rust imports: package `mesh-core-shell`, import `mesh_core_shell`.

**Module IDs And Interfaces:**
- Module package IDs use npm scope form: `@mesh/navigation-bar`, `@mesh/pipewire-audio`, `@mesh/pulseaudio-audio`.
- Interface IDs use dotted form: `mesh.audio`, `mesh.network`, `mesh.power`.
- Root layout entrypoints use `<module-id>:<entrypoint-id>`: `@mesh/navigation-bar:main`.
- Frontend service capabilities use `service.<name>.read` and `service.<name>.control`: `service.audio.read`, `service.audio.control`.
- Backend host capabilities use generic host powers: `exec.wpctl`, `exec.pactl`, `exec.aplay`.

## Where to Add New Code

**New Rust Core Capability Or Foundation Utility:**
- Primary code: `crates/core/foundation/<new-crate>/src/lib.rs` or the relevant existing foundation crate such as `crates/core/foundation/capability/src/lib.rs`.
- Workspace registration: `Cargo.toml` and the new crate `Cargo.toml`.
- Tests: Colocated `#[cfg(test)]` module in the crate source file.

**New Module Manifest Behavior:**
- Primary code: `crates/core/extension/plugin/src/package.rs`.
- Runtime compatibility mapping: `crates/core/extension/plugin/src/manifest.rs`.
- Tests: Colocated tests in `crates/core/extension/plugin/src/package.rs` or `crates/core/extension/plugin/src/manifest.rs`.
- Docs: `docs/module-system.md`.

**New Interface Contract Feature:**
- Primary code: `crates/core/extension/service/src/contract.rs` and `crates/core/extension/service/src/interface.rs`.
- Interface data: `modules/interfaces/<domain>.toml` or an interface module package.
- Tests: Colocated tests in `crates/core/extension/service/src/contract.rs`.

**New Backend Provider Module:**
- Implementation: `modules/backend/<provider-name>/package.json` plus `modules/backend/<provider-name>/src/main.luau`.
- Root graph entry: add `@scope/<provider-name>` to `config/package.json` under `mesh.modules`; select it under `mesh.providers` when it should be active.
- Catalog/bundled manifest: add `config/modules/@mesh/<provider-name>/package.json` when the provider should be represented in bundled package metadata.
- Contract: depend on or add an interface contract under `modules/interfaces/` or an interface module package.

**New Frontend Surface Or Widget Module:**
- Implementation: `modules/frontend/<module-name>/package.json` for target package shape, or `modules/frontend/<module-name>/module.json` only when compatibility with existing runtime fields is required.
- Entrypoint: `modules/frontend/<module-name>/src/main.mesh`.
- Local components: `modules/frontend/<module-name>/src/components/*.mesh`.
- Settings: `modules/frontend/<module-name>/config/settings.json`.
- Root graph entry: add to `config/package.json` under `mesh.modules`; select `mesh.layout.entrypoint` for a top-level layout when applicable.

**New `.mesh` Parser Or Renderer Capability:**
- Parser changes: `crates/core/ui/component/src/parser.rs` and submodules under `crates/core/ui/component/src/parser/`.
- Compiler changes: `crates/core/ui/render/src/compile.rs`.
- Runtime/render changes: `crates/core/shell/src/shell/component.rs`, `crates/core/ui/elements/src/`, `crates/core/ui/render/src/`.
- LSP updates: `crates/tools/lsp/src/knowledge/`, `crates/tools/lsp/src/analyzer/`, and `crates/tools/lsp/src/diagnostics.rs`.

**New Shell Runtime Behavior:**
- Lifecycle/event-loop behavior: `crates/core/shell/src/shell/mod.rs`.
- Component-specific behavior: `crates/core/shell/src/shell/component.rs` or submodules under `crates/core/shell/src/shell/component/`.
- Service command mapping: `crates/core/shell/src/shell/service.rs`.
- IPC behavior: `crates/core/shell/src/shell/ipc.rs`.

**New CLI Command:**
- Implementation: `crates/tools/cli/src/main.rs`.
- Shell API support: `crates/core/shell/src/lib.rs` and `crates/core/shell/src/shell/mod.rs` when command needs runtime access.

**Utilities:**
- Shared Rust helpers: place near the owning crate, not in a global utility crate by default.
- Shared frontend helpers: use or add Luau library modules once library resolution is implemented; package shape is documented in `docs/module-system.md`.
- Documentation-only references: `docs/` topic directory matching the feature area.

## Special Directories

**`.planning/`:**
- Purpose: GSD project state, roadmap, phase artifacts, and codebase maps.
- Generated: Yes
- Committed: Project-dependent; codebase docs are written under `.planning/codebase/`.

**`.claude/`:**
- Purpose: Claude/GSD commands, agents, hooks, templates, workflows, local settings.
- Generated: Yes
- Committed: Project-dependent local automation.

**`.agents/`:**
- Purpose: Agent configuration area.
- Generated: Yes
- Committed: Project-dependent.

**`.vscode/`:**
- Purpose: Editor settings.
- Generated: No
- Committed: Yes in this repo.

**`target/`:**
- Purpose: Cargo build output.
- Generated: Yes
- Committed: No

**`modules/frontend/navigation-bar/node_modules/`:**
- Purpose: Local package-manager dependencies for the navigation-bar module.
- Generated: Yes
- Committed: No

**`modules/frontend/navigation-bar/dist/`:**
- Purpose: Built frontend module output if generated by local tooling.
- Generated: Yes
- Committed: No

**`config/modules/`:**
- Purpose: Package-shaped bundled/catalog module manifests under `@mesh` scope.
- Generated: No
- Committed: Yes

**`config/themes/`:**
- Purpose: Theme token JSON files loaded by theme configuration and docs.
- Generated: No
- Committed: Yes

**`crates/core/ui/icon/assets/`:**
- Purpose: Bundled SVG icon assets.
- Generated: No
- Committed: Yes

---

*Structure analysis: 2026-05-06*
