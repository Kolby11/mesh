# Architecture

**Analysis Date:** 2026-05-01

## System Overview

```text
┌──────────────────────────────────────────────────────────────────────┐
│                        mesh-tools-cli                                │
│              `crates/tools/cli/src/main.rs`  (mesh-shell binary)     │
└──────────────────────────────┬───────────────────────────────────────┘
                               │
┌──────────────────────────────▼───────────────────────────────────────┐
│                        mesh-core-shell                               │
│             `crates/core/shell/src/shell/mod.rs`                     │
│  Shell struct: plugin host, event loop, surface orchestrator         │
│  Owns: ComponentRuntime[], EventBus, ThemeEngine, LocaleEngine,      │
│        InterfaceRegistry, ServiceRegistry, RenderEngine              │
└─────┬─────────────┬──────────────────┬────────────────┬─────────────┘
      │             │                  │                │
      ▼             ▼                  ▼                ▼
┌──────────┐ ┌───────────┐  ┌────────────────┐  ┌───────────────────┐
│mesh-core │ │mesh-core  │  │mesh-core-render│  │mesh-core-backend  │
│-service  │ │-plugin    │  │`ui/render/`    │  │`runtime/backend/` │
│(interface│ │(manifest  │  │Compiles .mesh  │  │Luau backend poll  │
│registry) │ │parsing)   │  │paints WidgetNode│  │loop + commands    │
└──────────┘ └───────────┘  └───────┬────────┘  └────────┬──────────┘
                                    │                    │
                          ┌─────────▼─────────┐  ┌──────▼──────────┐
                          │mesh-core-elements │  │mesh-core-       │
                          │`ui/elements/`     │  │scripting        │
                          │WidgetNode, layout,│  │`runtime/        │
                          │style, element API │  │scripting/`      │
                          └─────────┬─────────┘  │mlua Luau VM     │
                                    │            └─────────────────┘
                          ┌─────────▼─────────┐
                          │mesh-core-component│
                          │`ui/component/`    │
                          │.mesh file parser  │
                          └───────────────────┘

Foundation crates (no knowledge of layers above):
  mesh-core-theme, mesh-core-locale, mesh-core-config,
  mesh-core-events, mesh-core-capability, mesh-core-diagnostics,
  mesh-core-debug, mesh-core-wayland, mesh-core-icon, mesh-core-runtime(stub)
```

## Component Responsibilities

| Component | Responsibility | File |
|-----------|----------------|------|
| `Shell` | Plugin discovery, surface lifecycle, event loop, IPC | `crates/core/shell/src/shell/mod.rs` |
| `FrontendSurfaceComponent` | One surface plugin instance: tick, paint, input, settings | `crates/core/shell/src/shell/component.rs` |
| `ShellComponent` trait | Contract all shell components must implement | `crates/core/shell/src/shell/types.rs` |
| `CoreRequest` enum | All requests a component can make to the shell | `crates/core/shell/src/shell/types.rs` |
| `CoreEvent` enum | Events the shell broadcasts to all components | `crates/core/shell/src/shell/types.rs` |
| `ServiceEvent` enum | Service payload delivered to frontend components | `crates/core/shell/src/shell/types.rs` |
| `RenderEngine` | Selects Wayland or dev-window bridge, presents `PixelBuffer` | `crates/core/ui/render/src/surface/mod.rs` |
| `FrontendRenderEngine` | Thread-local painter; walks `WidgetNode` tree, draws pixels | `crates/core/ui/render/src/surface/painter.rs` |
| `PixelBuffer` | RGBA software framebuffer | `crates/core/ui/render/src/surface/buffer.rs` |
| `ScriptContext` | Frontend Luau VM instance: state, host API, interface proxies | `crates/core/runtime/scripting/src/context.rs` |
| `BackendScriptContext` | Backend Luau VM: init(), on_poll(), on_command_*() | `crates/core/runtime/scripting/src/backend.rs` |
| `WidgetNode` | Intermediate representation of one UI node in the tree | `crates/core/ui/elements/src/tree.rs` |
| `ElementKind` / `ElementTypeDef` | Core element model (box, row, icon, text, button…) | `crates/core/ui/elements/src/element.rs` |
| `StyleResolver` | Resolves CSS-like styles, applies theme tokens | `crates/core/ui/elements/src/style.rs` |
| `LayoutEngine` | Flexbox-inspired layout computation | `crates/core/ui/elements/src/layout.rs` |
| `ComponentFile` | Parsed `.mesh` single-file component AST | `crates/core/ui/component/src/lib.rs` |
| `Manifest` | Normalized plugin manifest (from `plugin.json` or `mesh.toml`) | `crates/core/extension/plugin/src/manifest.rs` |
| `InterfaceRegistry` | Tracks discovered interface contracts and providers | `crates/core/extension/service/src/interface.rs` |
| `ThemeEngine` | Active theme + token lookup | `crates/core/foundation/theme/src/lib.rs` |
| `LocaleEngine` | Active locale + translation lookup with fallback chain | `crates/core/foundation/locale/src/lib.rs` |
| `EventBus` | Tokio broadcast channels for inter-plugin events | `crates/core/foundation/events/src/lib.rs` |
| `DiagnosticsCollector` | Per-plugin health status, frame metrics | `crates/core/foundation/diagnostics/src/lib.rs` |
| `spawn_backend_service` | Async task: Luau backend poll loop + command dispatch | `crates/core/runtime/backend/src/lib.rs` |
| `mesh-tools-lsp` | Language server for `.mesh` files (completions, hover, diagnostics) | `crates/tools/lsp/src/` |

## Pattern Overview

**Overall:** Plugin-first layered architecture with a thin Rust wiring core.

**Key Characteristics:**
- The Rust core is a wiring layer only — it discovers plugins, loads manifests, and routes events. All service-specific logic (audio, network, power, media) lives in Luau backend plugins.
- Frontend plugins are compiled at startup into `WidgetNode` trees. The Luau `<script>` block runs in a real Luau VM (mlua); there is no hand-written interpreter.
- Reactive state is tracked via `ScriptState::dirty`: any bare global assignment in a `<script>` block is automatically synced to the UI after each handler call.
- All communication between backend plugins and frontend plugins flows through typed `ServiceEvent` payloads — never through shared Rust state.
- Surface layout (anchor, layer, size, exclusive zone, keyboard mode) is declared in `plugin.json`, not in Rust match arms.

## Layers

**CLI Layer:**
- Purpose: Parses CLI args, initializes tracing, starts `Shell::run()`
- Location: `crates/tools/cli/src/`
- Contains: `main.rs` with commands (`start`, `list`, `services`, `debug`, `ipc`, `status`, `version`)
- Depends on: `mesh-core-shell`, `mesh-core-plugin`, `mesh-core-config`, `mesh-core-diagnostics`

**Shell Orchestration Layer:**
- Purpose: Plugin discovery, lifecycle management, event loop, surface state, IPC server
- Location: `crates/core/shell/src/shell/`
- Contains: `mod.rs` (main loop ~1536 lines), `component.rs`, `ipc.rs`, `layout.rs`, `service.rs`, `surface_layout.rs`, `types.rs`, `sounds.rs`, `render/`
- Depends on: all other core crates
- Rule: Must never implement service logic (no `wpctl`, no `pactl`, no `if service_name == "audio"` branches)

**Render Layer:**
- Purpose: Compiles `.mesh` plugins, walks `WidgetNode` trees, paints `PixelBuffer`, presents to Wayland or dev window
- Location: `crates/core/ui/render/src/`
- Contains: `lib.rs`, `compile.rs`, `render.rs`, `style.rs`, `tags.rs`, `expr.rs`, `accessibility.rs`, `surface/` (painter, buffer, icon, text, debug_overlay, bridge)
- Depends on: `mesh-core-component`, `mesh-core-elements`, `mesh-core-icon`, `mesh-core-plugin`, `mesh-core-theme`, `mesh-core-wayland`

**Element / IR Layer:**
- Purpose: Core element model (`ElementKind`), `WidgetNode` IR, layout engine, style types, accessibility tree, event dispatcher — shared intermediate representation between rendering and scripting
- Location: `crates/core/ui/elements/src/`
- Contains: `element.rs`, `tree.rs`, `layout.rs`, `style.rs`, `events.rs`, `accessibility.rs`
- Depends on: `mesh-core-component`, `mesh-core-theme`
- Boundary: Does NOT depend on `mesh-core-service`, `mesh-core-scripting`, `mesh-core-wayland`, or `mesh-core-render`

**Component Parser Layer:**
- Purpose: Parses `.mesh` single-file components into typed AST (`ComponentFile`)
- Location: `crates/core/ui/component/src/`
- Contains: `lib.rs`, `parser.rs`, `parser/` (template.rs, script.rs, styles.rs, markup.rs), `template.rs`, `style.rs`
- Depends on: nothing from the MESH crate tree (pure parsing, no runtime dependencies)

**Scripting Layer:**
- Purpose: Embeds Luau VM via `mlua`; provides `ScriptContext` for frontend scripts and `BackendScriptContext` for backend scripts; implements `mesh.*` host API
- Location: `crates/core/runtime/scripting/src/`
- Contains: `lib.rs`, `context.rs`, `backend.rs`, `host_api.rs`
- Depends on: `mesh-core-component`, `mesh-core-elements`, `mesh-core-theme`, `mesh-core-locale`, `mesh-core-config`, `mesh-core-events`, `mesh-core-capability`, `mesh-core-service`

**Backend Runtime Layer:**
- Purpose: Owns the async Luau backend poll loop: calls `init()`, then polls at the declared interval, dispatches commands, emits `BackendServiceUpdate`
- Location: `crates/core/runtime/backend/src/lib.rs`
- Contains: `spawn_backend_service()`, `BackendServiceCommand`, `BackendServiceUpdate`
- Depends on: `mesh-core-scripting`

**Extension Layer:**
- Purpose: Manifest parsing (`Manifest`, `PluginType`, `PluginInstance`) and interface/service registry
- Location: `crates/core/extension/plugin/src/`, `crates/core/extension/service/src/`
- Contains: `manifest.rs`, `lifecycle.rs`, `lib.rs`; `contract.rs`, `interface.rs`, `registry.rs`

**Foundation Layer:**
- Purpose: Shared primitives with no knowledge of layers above
- Location: `crates/core/foundation/*/src/`
- Contains: `theme`, `locale`, `config`, `events`, `capability`, `diagnostics`, `debug`

## Data Flow

### Shell Startup

1. `main()` in `crates/tools/cli/src/main.rs` calls `Shell::new()` then `shell.run()`
2. `Shell::run()` calls `shell.discover_plugins()` — scans `packages/plugins/`, `/usr/share/mesh`, `~/.local/share/mesh`
3. For each frontend plugin: `compile_frontend_plugin()` in `crates/core/ui/render/src/compile.rs` → loads `plugin.json`, parses `.mesh` via `mesh-core-component`, builds `CompiledFrontendPlugin`
4. `surface_layout_from_manifest()` in `crates/core/shell/src/shell/surface_layout.rs` reads `plugin.json` defaults; user `config/settings.json` overrides applied on top
5. For each backend plugin: `spawn_backend_service()` in `crates/core/runtime/backend/src/lib.rs` launches a Tokio task running the Luau backend loop
6. Shell enters the main Tokio event loop

### Surface Rendering (per frame)

1. `Shell` main loop calls `component.tick()` on each `ComponentRuntime`
2. `FrontendSurfaceComponent::paint()` calls `ScriptContext` — evaluates Luau reactive state → builds `WidgetNode` tree via `build_widget_tree_from_component()`
3. If `size_policy == ContentMeasured` (launcher): `measure_content_size()` uses manifest clamp bounds
4. `paint_frontend_tree()` in `crates/core/ui/render/src/surface/mod.rs` walks the `WidgetNode` tree
5. `FrontendRenderEngine` resolves styles (`StyleResolver`), computes layout (`LayoutEngine`), draws into `PixelBuffer`
6. `RenderEngine::present()` commits `PixelBuffer` to Wayland via `PresentationBridge` → `wayland_surface.rs`

### Service / Backend Event Flow

```
Backend Luau plugin (main.luau)
  → mesh.service.emit({percent=75, muted=false})
  → BackendScriptContext collects payload
  → spawn_backend_service() sends BackendServiceUpdate on mpsc channel
  → Shell receives ServiceEvent::Updated{service="audio", payload={...}}
  → Shell sets state["audio"] on all frontend ScriptContexts
  → Shell calls on_audio_update() (if declared) in each frontend script
  → Frontend script writes reactive globals (icon_name = "audio-volume-high")
  → ScriptState marked dirty
  → Next paint rebuilds WidgetNode tree with updated globals
  → Template {icon_name} renders updated value
```

### Settings Flow

```
plugin.json settings.schema.surface.properties[field].default
  ↓ (baseline) surface_layout_from_manifest() in shell/surface_layout.rs
  ↓ + user overrides
config/settings.json → load_frontend_plugin_settings()
  ↓
FrontendSurfaceComponent.surface_layout / settings_json
  ↓
ScriptContext state["settings"] ← Luau reads {settings.surface.anchor}
```

### IPC Command Flow

```
mesh-shell ipc <command>  (crates/tools/cli/src/main.rs)
  → Unix socket → crates/core/shell/src/shell/ipc.rs
  → ShellMessage::Ipc(CoreRequest)
  → Shell::handle_request() match arm
  → May produce CoreRequest to components (ToggleSurface, SetTheme, etc.)
```

## Key Abstractions

**`ShellComponent` trait:**
- Purpose: Contract for all renderable surface/widget instances
- Location: `crates/core/shell/src/shell/types.rs`
- Methods: `mount()`, `tick()`, `paint()`, `handle_input()`, `handle_service_event()`, `handle_core_event()`, `theme_changed()`, `reload_source()`

**`WidgetNode`:**
- Purpose: The intermediate representation of one rendered UI node — tag, attributes, children, computed layout rect, style, element state
- Location: `crates/core/ui/elements/src/tree.rs`
- Pattern: Built fresh each frame from the component AST + current Luau state; not a persistent DOM

**`Manifest`:**
- Purpose: Normalized plugin descriptor regardless of source format (`plugin.json` or `mesh.toml`)
- Location: `crates/core/extension/plugin/src/manifest.rs`
- Key sections: `package`, `capabilities`, `entrypoints`, `settings`, `surface_layout`, `provides`, `dependencies`

**`ComponentFile`:**
- Purpose: Parsed AST of a `.mesh` single-file component
- Location: `crates/core/ui/component/src/lib.rs`
- Fields: `imports`, `template`, `script`, `style`, `i18n`

**`InterfaceRegistry` / `InterfaceContract`:**
- Purpose: Maps interface IDs (`@mesh/audio`) to their contracts and active provider plugins
- Location: `crates/core/extension/service/src/interface.rs`, `contract.rs`
- Pattern: Contract is declared in `interface.toml`; provider is discovered from `plugin.json`'s `provides` field

**`ScriptState`:**
- Purpose: Reactive variable store for a frontend script; any bare global = reactive
- Location: `crates/core/runtime/scripting/src/context.rs`
- Pattern: After each Luau handler call, globals are synced back. `dirty` flag triggers tree rebuild.

## Entry Points

**Shell binary:**
- Location: `crates/tools/cli/src/main.rs`
- Triggers: `cargo run -p mesh-tools-cli --bin mesh-shell -- start`
- Responsibilities: arg parsing, tracing init, `Shell::new()`, `shell.run()`

**LSP binary:**
- Location: `crates/tools/lsp/src/main.rs`
- Triggers: Editor LSP client connects via stdio
- Responsibilities: `.mesh` file completions, hover, diagnostics via `tower-lsp`

**Backend plugin entry:**
- Location: Luau `init()` function in `packages/plugins/backend/core/<name>/src/main.luau`
- Triggers: Called once by `spawn_backend_service()` at shell startup
- Responsibilities: `mesh.service.set_poll_interval()`, register any setup

**Frontend plugin entry:**
- Location: `.mesh` file `<script>` block globals + `on_<service>_update()` handlers
- Triggers: `FrontendSurfaceComponent::mount()` and each service event

## Architectural Constraints

- **Threading:** Single Tokio runtime; backend plugins run as independent async tasks via `tokio::spawn`. Frontend rendering is synchronous (software rasterizer) on the shell's main thread. `FrontendRenderEngine` is `thread_local!`.
- **Global state:** `ICON_CACHE` (`OnceLock<Mutex<HashMap>>`) in `crates/core/ui/icon/src/lib.rs`; `IMAGE_CACHE` in `crates/core/ui/render/src/surface/icon.rs`; `FRONTEND_RENDERER` (`thread_local! RefCell<FrontendRenderEngine>`) in `crates/core/ui/render/src/surface/mod.rs`
- **No service logic in core:** `mesh-core-shell` must never spawn polling loops for specific services, call `wpctl`/`pactl`/`nmcli`, or branch on service names. This is the single most critical architectural rule.
- **No display logic in core:** Core must never inject computed display fields (icon names, formatted labels, derived booleans) into service payloads. Frontend plugins compute display state from raw service payloads inside their own `<script>` blocks.
- **Circular imports:** None detected. Dependency graph flows strictly downward (see Crate Map in docs/llm-context.md). `mesh-core-elements` has an explicit boundary comment preventing it from importing `mesh-core-service`, `mesh-core-scripting`, `mesh-core-wayland`, or `mesh-core-render`.
- **Plugin loading order:** Dependency graph is validated before loading via `validate_plugin_dependency_graph()` in `mesh-core-plugin`

## Anti-Patterns

### Service Logic in Rust Core

**What happens:** A service-specific polling loop, command formatter, or system tool call (e.g. `wpctl`, `pactl`) is added to a Rust file under `crates/core/shell/`
**Why it's wrong:** Violates the wiring-only rule; makes the shell non-extensible and couples Rust to specific system tools
**Do this instead:** Add a `mesh.exec_shell()` host API primitive in `crates/core/runtime/scripting/src/host_api.rs` and implement the logic in a Luau backend plugin under `packages/plugins/backend/core/`

### Display State Injection

**What happens:** Core computes `icon_name`, formatted labels, or derived booleans and injects them into the `ServiceEvent` payload before sending to frontend scripts
**Why it's wrong:** Frontend plugins own their display state; injected fields create invisible coupling between core and every frontend
**Do this instead:** Backend emits raw data (`percent`, `muted`). Frontend `<script>` block calls `audio.on_change(function() ... end)` and computes display state locally, writing to reactive globals

### New Hand-Written Interpreters

**What happens:** New string parsing / execution logic is added to Rust to "handle" Luau-like syntax or backend script patterns
**Why it's wrong:** `mlua` already provides a real Luau VM; custom interpreters duplicate it and diverge in behavior
**Do this instead:** Route all script execution through `ScriptContext` / `BackendScriptContext` in `crates/core/runtime/scripting/`

## Error Handling

**Strategy:** `thiserror`-derived typed errors at crate boundaries; `tracing::error!` / `tracing::warn!` for runtime errors that don't propagate (e.g. failed plugin paint). Plugin errors are isolated — one plugin failure does not crash the shell.

**Patterns:**
- `ScriptError` variants in `crates/core/runtime/scripting/src/context.rs` — covers Luau errors, capability denied, interface unavailable, timeout
- `ComponentError` in `crates/core/shell/src/shell/types.rs` — wraps script errors with component context
- `RenderError` in `crates/core/ui/render/src/surface/mod.rs` — Wayland connection and buffer allocation failures
- `ParseError` in `crates/core/ui/component/src/parser.rs` — `.mesh` file parse failures
- Backend plugin failures (failed `init()`, failed poll) are logged and the backend task exits; the shell continues

## Cross-Cutting Concerns

**Logging:** `tracing` macros throughout all crates; level controlled by `RUST_LOG` env var; initialized once in `crates/tools/cli/src/main.rs`
**Theme tokens:** Resolved via `StyleResolver` at paint time; referenced in `.mesh` `<style>` blocks with `token(group.name)` syntax; token groups: `color`, `typography`, `spacing`, `radius`, `elevation`, `border`, `motion`, `shadow`, `shape`, `state`, `base16`
**Capability checks:** Declared in `plugin.json` `capabilities.required`; enforced in `ScriptContext` via `CapabilitySet`; `ScriptError::CapabilityDenied` returned for unauthorized host API calls
**Accessibility:** `AccessibilityTree` built alongside `WidgetNode` tree in `crates/core/ui/elements/src/accessibility.rs`; surface role declared in `plugin.json` `accessibility` section
**Hot reload:** Source file watching in `ComponentRuntime`; `reload_source()` and `reload_plugin_settings()` on `ShellComponent` trait; triggered when modified timestamp changes in the shell event loop

---

*Architecture analysis: 2026-05-01*
