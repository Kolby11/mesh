<!-- refreshed: 2026-05-06 -->
# Architecture

**Analysis Date:** 2026-05-06

## System Overview

```text
┌─────────────────────────────────────────────────────────────┐
│                     MESH Shell Process                       │
│        `crates/tools/cli/src/main.rs` -> `Shell::run()`       │
├──────────────────┬──────────────────┬───────────────────────┤
│  Module Graph    │ Frontend Runtime │   Backend Runtime      │
│ `config/package` │ `.mesh` surfaces │   Luau providers       │
│ `extension/package`│ `ui/render`    │   `runtime/backend`    │
└────────┬─────────┴────────┬─────────┴──────────┬────────────┘
         │                  │                     │
         ▼                  ▼                     ▼
┌─────────────────────────────────────────────────────────────┐
│              Generic Shell Orchestration Layer               │
│ `crates/core/shell/src/shell/mod.rs`                         │
│ config, discovery, interfaces, IPC, events, diagnostics, UI  │
└────────┬──────────────────┬─────────────────────┬───────────┘
         │                  │                     │
         ▼                  ▼                     ▼
┌────────────────┐  ┌────────────────────┐  ┌─────────────────┐
│ Wayland/Render │  │ Interface Registry │  │ Installed Files │
│ `ui/render`    │  │ `extension/service`│  │ `modules/`      │
│ `platform`     │  │ `interfaces/*.toml`│  │ `config/`       │
└────────────────┘  └────────────────────┘  └─────────────────┘
```

## Component Responsibilities

| Component                     | Responsibility                                                                                                                                            | File                                               |
| ----------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| CLI entrypoint                | Starts the shell, lists discovered modules, sends IPC commands, reports status.                                                                           | `crates/tools/cli/src/main.rs`                     |
| Shell orchestrator            | Owns process lifecycle, discovery, frontend mounting, backend spawning, event loop, IPC, theme reload, locale reload, service state routing, diagnostics. | `crates/core/shell/src/shell/mod.rs`               |
| Package/module loader         | Parses root installed-module graph and module `package.json` manifests, validates selected providers and layout entrypoints, indexes contributions.       | `crates/core/extension/plugin/src/package.rs`      |
| Compatibility manifest loader | Normalizes `package.json`, `package.json`, `plugin.json`, and `mesh.toml` into `Manifest` for runtime discovery.                                          | `crates/core/extension/plugin/src/manifest.rs`     |
| Interface contracts           | Loads TOML service contracts into state fields, methods, events, types, and capability metadata.                                                          | `crates/core/extension/service/src/contract.rs`    |
| Interface registry            | Registers discovered contracts/providers and resolves frontend `require()`/`mesh.service.use()` lookups.                                                  | `crates/core/extension/service/src/interface.rs`   |
| Backend runtime               | Runs backend Luau scripts, calls `init()`, polls state, dispatches commands, emits service lifecycle/update events.                                       | `crates/core/runtime/backend/src/lib.rs`           |
| Scripting runtime             | Hosts frontend/backend Luau contexts, installs `mesh.*` host APIs, tracks service field reads, converts proxy method calls into published events.         | `crates/core/runtime/scripting/src/context.rs`     |
| Frontend catalog              | Compiles discovered frontend modules, validates component imports and slot composition, chooses top-level surfaces.                                       | `crates/core/shell/src/shell/component/catalog.rs` |
| Frontend component host       | Mounts compiled `.mesh` components, maintains script state, applies service/theme/locale events, processes input, renders widget trees.                   | `crates/core/shell/src/shell/component.rs`         |
| Component parser              | Parses `.mesh` single-file components into template/script/style/i18n AST structures.                                                                     | `crates/core/ui/component/src/lib.rs`              |
| Renderer                      | Compiles frontend manifests, resolves local component imports, paints widget trees to buffers/surfaces.                                                   | `crates/core/ui/render/src/compile.rs`             |
| Configuration                 | Loads shell TOML config, JSON shell settings, default settings, discovery paths, and legacy per-module override files.                                    | `crates/core/foundation/config/src/lib.rs`         |

## Pattern Overview

**Overall:** Modular shell runtime with package-shaped modules, contract-based service interfaces, and Luau-hosted frontend/backend behavior.

**Key Characteristics:**
- Use `package.json` with a top-level `mesh` section as the target module manifest shape. Root installed-module state lives in `config/package.json`; module manifests live under paths selected by `mesh.modulesDir` such as `modules/backend/pipewire-audio/package.json`.
- Keep Rust core generic. Domain behavior such as audio state parsing belongs in backend modules such as `modules/backend/pipewire-audio/src/main.luau` and `modules/backend/pulseaudio-audio/src/main.luau`.
- Route frontend/backend coupling through interface contracts such as `modules/interfaces/audio.toml`, not through direct frontend imports of backend module IDs.
- Maintain compatibility with `package.json`, `plugin.json`, and `mesh.toml` through `crates/core/extension/plugin/src/manifest.rs` and `ModulePackageManifest::from_legacy_manifest()` in `crates/core/extension/plugin/src/package.rs`.
- Run frontend surfaces as compiled `.mesh` single-file components and backend providers as Luau scripts with capability-gated host APIs.

## Layers

**Package And Module Graph:**
- Purpose: Define which modules are installed, enabled, active for interfaces, and selected as layout entrypoints.
- Location: `config/package.json`, `modules/**/package.json`, `modules/**/package.json`, `crates/core/extension/plugin/src/package.rs`.
- Contains: Root graph (`mesh.schemaVersion`, `mesh.modulesDir`, `mesh.modules`, `mesh.providers`, `mesh.layout`) and module metadata (`mesh.kind`, `mesh.entrypoints`, `mesh.dependencies`, `mesh.implements`, `mesh.contributes`).
- Depends on: JSON parsing, relative path validation, manifest normalization in `crates/core/extension/plugin/src/manifest.rs`.
- Used by: Backend launch selection in `crates/core/shell/src/shell/mod.rs` and architecture/docs/runtime references.

**Discovery And Compatibility:**
- Purpose: Recursively scan module directories and normalize supported manifest files into runtime `Manifest` values.
- Location: `crates/core/shell/src/shell/mod.rs`, `crates/core/extension/plugin/src/manifest.rs`.
- Contains: `Shell::discover_plugins()`, `Shell::scan_plugin_dir()`, `load_manifest()`, legacy JSON/TOML parsers.
- Depends on: Discovery paths from `crates/core/foundation/config/src/lib.rs`.
- Used by: Frontend catalog compilation, interface provider registration, backend launch candidate validation.

**Interface And Service Contracts:**
- Purpose: Define service state fields, mutating methods, events, types, and required/optional capabilities as data.
- Location: `crates/core/extension/service/src/contract.rs`, `crates/core/extension/service/src/interface.rs`, `modules/interfaces/audio.toml`.
- Contains: `InterfaceContract`, `InterfaceRegistry`, provider registration, version matching, canonical interface names.
- Depends on: TOML contracts and manifest `interface`/`provides`/`implements` sections.
- Used by: Frontend script proxy resolution, backend provider validation, service state shape warnings.

**Shell Orchestration:**
- Purpose: Tie together config, module discovery, frontend mounting, backend runtime tasks, IPC, input, rendering, theme/locale reloads, diagnostics, and shutdown.
- Location: `crates/core/shell/src/shell/mod.rs`.
- Contains: `Shell`, backend runtime slots/statuses, latest service state cache, event loop, request draining, IPC socket selection.
- Depends on: All core crates through `crates/core/shell/Cargo.toml`.
- Used by: CLI `mesh-shell start` and tests under `crates/core/shell/src/shell/component/tests.rs`.

**Frontend Runtime:**
- Purpose: Compile, mount, update, and render `.mesh` frontend modules.
- Location: `crates/core/ui/component/src`, `crates/core/ui/render/src`, `crates/core/shell/src/shell/component*`.
- Contains: Parser, compiler, local component import resolver, slot/catalog validation, `FrontendSurfaceComponent`, runtime tree, layout, input, animation, diagnostics, rendering.
- Depends on: Module manifests, `mesh-core-elements`, theme, locale, scripting, service catalog.
- Used by: Top-level surfaces from `modules/frontend/navigation-bar/package.json` and component files under `modules/frontend/navigation-bar/src/`.

**Backend Runtime:**
- Purpose: Execute backend Luau scripts as providers for interface state and methods.
- Location: `crates/core/runtime/backend/src/lib.rs`, `crates/core/runtime/scripting/src/backend.rs`, backend modules under `modules/backend/`.
- Contains: `spawn_backend_service()`, backend command/update events, poll loop, command dispatch, runtime failure handling.
- Depends on: Active provider selection from `InstalledModuleGraph`, script source files, requested capabilities, backend settings.
- Used by: Shell backend spawning and service update broadcasting.

**Foundation And Platform:**
- Purpose: Shared cross-cutting support for config, capability checks, diagnostics, events, theme, locale, icons, Wayland surfaces, and debug overlay data.
- Location: `crates/core/foundation/*`, `crates/core/platform/wayland`, `crates/core/ui/icon`.
- Contains: Small crates with focused APIs reused by shell/runtime/render crates.
- Depends on: Workspace dependencies in `Cargo.toml`.
- Used by: Most runtime layers.

## Data Flow

### Primary Shell Startup Path

1. CLI command dispatch starts `Shell::new()` and `Shell::run()` (`crates/tools/cli/src/main.rs:22`, `crates/tools/cli/src/main.rs:33`).
2. `Shell::new()` loads shell config, settings, theme, locale, discovery paths, diagnostics, render engine, and runtime state (`crates/core/shell/src/shell/mod.rs:183`).
3. `Shell::run()` discovers manifests, loads themes, validates module dependencies, compiles frontend components, creates a Tokio runtime, spawns active backend providers, starts IPC, mounts surfaces, and enters the render/event loop (`crates/core/shell/src/shell/mod.rs:465`).
4. `Shell::discover_plugins()` scans configured discovery paths and registers interface contracts and providers from normalized manifests (`crates/core/shell/src/shell/mod.rs:236`).
5. `load_frontend_components()` builds a `FrontendCatalog` from discovered frontend manifests and registers top-level surfaces (`crates/core/shell/src/shell/mod.rs:697`, `crates/core/shell/src/shell/component/catalog.rs:23`).
6. The event loop processes backend/service/IPC messages, component ticks, requests, rendering, Wayland flushing, and render-engine pumping (`crates/core/shell/src/shell/mod.rs:501`).

### Installed Module Graph Path

1. `config/package.json` declares `@mesh/navigation-bar`, `@mesh/pipewire-audio`, `@mesh/pulseaudio-audio`, active provider `mesh.audio -> @mesh/pipewire-audio`, and layout entrypoint `@mesh/navigation-bar:main`.
2. `spawn_backend_plugins()` loads `config/package.json` through `load_installed_module_graph()` before falling back to legacy backend discovery (`crates/core/shell/src/shell/mod.rs:1571`, `crates/core/extension/plugin/src/package.rs:1617`).
3. `load_installed_module_graph()` resolves `mesh.modulesDir` relative to the root manifest and loads each module manifest from `modules/` (`crates/core/extension/plugin/src/package.rs:1620`).
4. `InstalledModuleGraph::from_parts()` validates root entries against loaded module kinds, indexes enabled frontend requirements, interface declarations, backend providers, resource contributions, settings schemas, and layout contribution (`crates/core/extension/plugin/src/package.rs:1028`).
5. Active provider selections are validated against installed enabled backend modules and their implemented interfaces (`crates/core/extension/plugin/src/package.rs:1128`).
6. `backend_launch_candidates_from_graph()` chooses active providers, validates runtime manifests and required binaries, reads backend Luau entrypoints, and returns launch candidates plus lifecycle status records (`crates/core/shell/src/shell/mod.rs:1775`).

### Frontend Service Consumption Flow

1. A frontend `.mesh` file imports local components and/or calls `require("@mesh/audio@>=1.0")` or `mesh.service.use("mesh.audio")` in Luau (`modules/frontend/navigation-bar/src/main.mesh`, `docs/module-system.md`).
2. `compile_frontend_module()` reads the `.mesh` entrypoint, recursively resolves local component imports, and validates standalone component scope (`crates/core/ui/render/src/compile.rs:48`).
3. `ScriptContext::load_script_with_interface_imports()` installs host APIs, executes Luau, and syncs exported globals into reactive state (`crates/core/runtime/scripting/src/context.rs:268`).
4. `require()` canonicalizes `@mesh/...` or `mesh.*` names, checks read capability, resolves the interface/provider, and creates a service proxy (`crates/core/runtime/scripting/src/context.rs:550`).
5. Proxy state reads are served from the latest service payload table and recorded as tracked fields (`crates/core/runtime/scripting/src/context.rs:923`).
6. Proxy method calls emit events such as `mesh.audio.set_volume` with capability metadata (`crates/core/runtime/scripting/src/context.rs:881`).
7. Shell converts published events to `CoreRequest::ServiceCommand` only when the source has `service.<name>.control` (`crates/core/shell/src/shell/service.rs:113`).

### Backend Provider Flow

1. The installed module graph selects one active backend provider for an interface (`config/package.json`, `crates/core/extension/plugin/src/package.rs:1128`).
2. `spawn_backend_candidate()` creates command/update channels, bridges backend runtime events into shell messages, and stores a backend runtime slot for the interface (`crates/core/shell/src/shell/mod.rs:1642`).
3. `spawn_backend_service()` loads the backend Luau script, runs `init()`, emits `Started`, polls with `on_poll()`, and publishes changed state only when payloads differ (`crates/core/runtime/backend/src/lib.rs:40`).
4. Service updates become `ShellMessage::Service`, then `ServiceEvent::Updated`, then are validated against interface contract state fields and cached by interface (`crates/core/shell/src/shell/mod.rs:1660`, `crates/core/shell/src/shell/mod.rs:796`).
5. Cached service updates replay to components after mount, so frontends can initialize from backend state (`crates/core/shell/src/shell/mod.rs:851`).
6. Service commands from frontends are routed back to the active backend command channel when capability checks pass (`crates/core/shell/src/shell/mod.rs`, `crates/core/runtime/backend/src/lib.rs`).

**State Management:**
- Shell-level mutable state is stored in `Shell` fields in `crates/core/shell/src/shell/mod.rs`.
- Frontend reactive state lives in each `ScriptContext` and component runtime state under `crates/core/shell/src/shell/component.rs`.
- Backend state is exported from Luau globals such as `state` and snapshots emitted by `spawn_backend_service()` in `crates/core/runtime/backend/src/lib.rs`.
- Latest public service state is cached per canonical interface in `latest_service_state` in `crates/core/shell/src/shell/mod.rs`.
- User settings come from JSON/TOML files loaded by `crates/core/foundation/config/src/lib.rs`.

## Key Abstractions

**Module Package:**
- Purpose: Single installable unit for frontend, backend, interface, theme, icon pack, font pack, language pack, or Luau library.
- Examples: `modules/backend/pipewire-audio/package.json`, `config/modules/@mesh/quick-settings/package.json`, `docs/module-system.md`.
- Pattern: Standard npm-compatible top-level metadata with all MESH behavior under `mesh`.

**Installed Module Graph:**
- Purpose: Runtime selection graph for enabled modules, active providers, resource contributions, frontend requirements, and selected layout.
- Examples: `config/package.json`, `crates/core/extension/plugin/src/package.rs`.
- Pattern: `RootPackageManifest` plus loaded `ModulePackageManifest` values create `InstalledModuleGraph`.

**Runtime Manifest:**
- Purpose: Compatibility-normalized structure consumed by older runtime discovery and frontend/backend loading.
- Examples: `crates/core/extension/plugin/src/manifest.rs`, `crates/core/extension/plugin/src/package.rs`.
- Pattern: `ModulePackageManifest::into_runtime_manifest()` maps package modules into `Manifest`; `from_legacy_manifest()` maps legacy manifests into package modules.

**Interface Contract:**
- Purpose: Data contract for service state, methods, events, types, and capabilities.
- Examples: `modules/interfaces/audio.toml`, `crates/core/extension/service/src/contract.rs`.
- Pattern: TOML contract loaded into `InterfaceContract` and registered in `InterfaceRegistry`.

**Frontend Component:**
- Purpose: `.mesh` single-file UI unit with template, Luau script, style, and imports.
- Examples: `modules/frontend/navigation-bar/src/main.mesh`, `modules/frontend/navigation-bar/src/components/volume-button.mesh`.
- Pattern: Parse with `mesh-core-component`, compile with `mesh-core-render`, execute script with `mesh-core-scripting`, render through `mesh-core-elements`.

**Backend Provider:**
- Purpose: Luau service adapter from a system source to a contract payload and command handlers.
- Examples: `modules/backend/pipewire-audio/src/main.luau`, `modules/backend/pulseaudio-audio/src/main.luau`.
- Pattern: `init()`, `on_poll()`, `on_command_<method>()`, exported `state`, capability-gated host APIs.

**Capability Set:**
- Purpose: Permission boundary for frontend service reads/commands and backend host APIs.
- Examples: `crates/core/foundation/capability/src/lib.rs`, `modules/frontend/navigation-bar/package.json`, `modules/backend/pipewire-audio/package.json`.
- Pattern: Manifest-required and optional capabilities are granted into runtime contexts and checked before host API or service command access.

## Entry Points

**Shell CLI:**
- Location: `crates/tools/cli/src/main.rs`
- Triggers: `mesh-shell start`, `mesh-shell list`, `mesh-shell services`, `mesh-shell debug`, `mesh-shell ipc`, `mesh-shell status`.
- Responsibilities: Initialize tracing, dispatch CLI commands, start `Shell::run()`, connect to IPC socket.

**Shell Runtime:**
- Location: `crates/core/shell/src/shell/mod.rs`
- Triggers: `Shell::run()` from CLI or tests.
- Responsibilities: Discover modules, compile UI, spawn backends, run IPC/event/render loop.

**Root Module Graph:**
- Location: `config/package.json`
- Triggers: `spawn_backend_plugins()` loads it from the workspace root.
- Responsibilities: Select enabled modules, active providers, and layout entrypoint.

**Frontend Surface:**
- Location: `modules/frontend/navigation-bar/package.json` and `modules/frontend/navigation-bar/src/main.mesh`
- Triggers: Manifest discovery plus `FrontendCatalog::top_level_surfaces()`.
- Responsibilities: Declare a top-level surface, capabilities, settings schema, layout policy, and `.mesh` entrypoint.

**Backend Provider:**
- Location: `modules/backend/pipewire-audio/package.json` and `modules/backend/pipewire-audio/src/main.luau`
- Triggers: Active provider selection for `mesh.audio` in `config/package.json`.
- Responsibilities: Provide audio state and command handlers using `wpctl`/`aplay`.

**Interface Contract:**
- Location: `modules/interfaces/audio.toml`
- Triggers: Interface file scanning or interface module manifests.
- Responsibilities: Define the `mesh.audio` contract.

## Architectural Constraints

- **Threading:** Shell rendering/input loop is synchronous in `Shell::run()` with a Tokio runtime used for backend tasks and IPC (`crates/core/shell/src/shell/mod.rs:474`). Backend providers run as Tokio tasks in `crates/core/runtime/backend/src/lib.rs`.
- **Global state:** Runtime state is centralized in `Shell` fields in `crates/core/shell/src/shell/mod.rs`; frontend script state is per `ScriptContext`; backend script state is per `BackendScriptContext`. Environment-derived paths include `MESH_HOME`, `MESH_SETTINGS_PATH`, `MESH_IPC_SOCKET`, and XDG paths in `crates/core/extension/plugin/src/package.rs` and `crates/core/foundation/config/src/lib.rs`.
- **Circular imports:** No circular Rust crate imports detected in `Cargo.toml`; component import cycles are tolerated by `compile_frontend_module()` through a `seen_local_paths`/ancestry guard in `crates/core/ui/render/src/compile.rs`.
- **Manifest precedence:** Runtime discovery checks `package.json`, then `package.json`, then `plugin.json`, then `mesh.toml` in `crates/core/extension/plugin/src/manifest.rs`; installed-module loading checks `package.json`, then `package.json`, then `plugin.json` in `crates/core/extension/plugin/src/package.rs`.
- **Module source split:** `config/package.json` points `mesh.modulesDir` to `../modules`; `config/modules/@mesh/*/package.json` holds package-shaped bundled/catalog manifests and is not loaded by the current root graph unless the root graph points at it.
- **Provider multiplicity:** Multiple backend modules can implement an interface, but one active provider per interface is selected by `config/package.json` for runtime launch.

## Anti-Patterns

### Frontend Imports Backend Module IDs

**What happens:** A frontend script targets `@mesh/pipewire-audio` or `@mesh/pulseaudio-audio` directly.
**Why it's wrong:** It bypasses provider selection in `config/package.json` and prevents users from swapping providers for `mesh.audio`.
**Do this instead:** Require the interface contract, for example `require("@mesh/audio@>=1.0")` or `mesh.service.use("mesh.audio")`, as supported by `crates/core/runtime/scripting/src/context.rs`.

### Rust Core Owns Domain Service Logic

**What happens:** Audio, power, network, media, or other domain-specific parsing/control is implemented in core shell/runtime crates.
**Why it's wrong:** The module system expects backend modules to adapt system APIs into interface contracts while Rust remains generic.
**Do this instead:** Put system-specific code in backend Luau modules such as `modules/backend/pipewire-audio/src/main.luau`; add generic host APIs in `crates/core/runtime/scripting/src/host_api.rs` when new capabilities are needed.

### Package Manifests Use Top-Level MESH Fields

**What happens:** New modules use top-level `type`, `id`, MESH dependency objects, capabilities, entrypoints, providers, settings, themes, or binary requirements.
**Why it's wrong:** `package.json` is npm-compatible; standard package metadata stays top-level and MESH-specific behavior lives under `mesh`.
**Do this instead:** Follow `modules/backend/pipewire-audio/package.json` and `docs/module-system.md`: use top-level `name`, `version`, `description`, `private`, and put `kind`, `entrypoints`, `dependencies`, `implements`, `contributes`, and `capabilities` under `mesh`.

### Backend Launch Reads Only `mesh.provides`

**What happens:** Provider-launch logic only enumerates `mesh.provides` even though target package manifests can declare providers with `mesh.implements`.
**Why it's wrong:** `InstalledModuleGraph` indexes both `provides` and `implements`, but `backend_launch_candidates_from_graph()` reads `module.manifest.mesh.provides` for interface discovery and validation in `crates/core/shell/src/shell/mod.rs`.
**Do this instead:** Use the `MeshModuleSection::implementations()` pattern from `crates/core/extension/plugin/src/package.rs` wherever runtime code needs all provider declarations.

## Error Handling

**Strategy:** Convert low-level IO/parse/validation failures into typed errors for package/config/contract loading, then record runtime failures as diagnostics and backend lifecycle status records.

**Patterns:**
- Package and graph failures use `PackageManifestError` in `crates/core/extension/plugin/src/package.rs`.
- Shell startup failures use `ShellRunError` in `crates/core/shell/src/shell/mod.rs`.
- Backend lifecycle failures become `BackendRuntimeStatusEntry` records and diagnostics in `crates/core/shell/src/shell/mod.rs`.
- Backend script errors emit `InitFailed`, `PollFailed`, `Failed`, or `Stopped` from `crates/core/runtime/backend/src/lib.rs`.
- Service state contract mismatches produce warnings and lifecycle diagnostics rather than immediate shell termination in `crates/core/shell/src/shell/mod.rs`.

## Cross-Cutting Concerns

**Logging:** Uses `tracing` across shell, module loading, runtime, render, and CLI crates. CLI initializes `tracing_subscriber` in `crates/tools/cli/src/main.rs`.
**Validation:** Package graph validation lives in `crates/core/extension/plugin/src/package.rs`; dependency graph validation lives in `crates/core/extension/plugin/src/manifest.rs`; interface contract parsing lives in `crates/core/extension/service/src/contract.rs`; service payload checks live in `crates/core/shell/src/shell/mod.rs`.
**Authentication:** Not applicable. This local shell framework uses capabilities and environment/path boundaries rather than user identity auth.
**Capabilities:** Capability declarations live in manifests and are enforced in `crates/core/runtime/scripting/src/context.rs` and `crates/core/shell/src/shell/service.rs`.
**Settings:** Shell defaults and user overrides flow through `crates/core/foundation/config/src/lib.rs`; module-level settings schemas are declared in manifests such as `modules/frontend/navigation-bar/package.json` and `config/modules/@mesh/panel/package.json`.
**Theming/Locale:** Theme and locale engines are shell-owned services exposed to components through state and capability checks in `crates/core/shell/src/shell/mod.rs` and `crates/core/runtime/scripting/src/context.rs`.

---

*Architecture analysis: 2026-05-06*
