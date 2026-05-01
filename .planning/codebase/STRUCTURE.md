# Codebase Structure

**Analysis Date:** 2026-05-01

## Directory Layout

```
mesh/                                   # Workspace root
├── Cargo.toml                          # Workspace definition, shared deps, MSRV
├── Cargo.lock                          # Committed lockfile
├── flake.nix / flake.lock              # Nix dev environment
├── CLAUDE.md                           # Primary AI context (loads docs/llm-context.md)
├── docs/                               # Architecture docs, LLM context, specs
│   └── llm-context.md                  # Crate map, data flows, task entry points
├── config/                             # Shell-level config (shipped defaults)
│   ├── shell-settings.json             # Active theme + locale settings
│   ├── settings-default.json           # Fallback settings
│   ├── mesh-default-dark.json          # Dark theme token file
│   └── themes/                         # Additional theme token files
│       ├── mesh-default-dark.json
│       └── mesh-default-light.json
├── crates/                             # All Rust crates
│   ├── core/                           # Core runtime crates
│   │   ├── shell/                      # mesh-core-shell: orchestrator
│   │   ├── extension/
│   │   │   ├── plugin/                 # mesh-core-plugin: manifest parsing
│   │   │   └── service/                # mesh-core-service: interface registry
│   │   ├── foundation/
│   │   │   ├── capability/             # mesh-core-capability: permission model
│   │   │   ├── config/                 # mesh-core-config: ShellConfig, ShellSettings
│   │   │   ├── debug/                  # mesh-core-debug: DebugSnapshot, overlay state
│   │   │   ├── diagnostics/            # mesh-core-diagnostics: health + metrics
│   │   │   ├── events/                 # mesh-core-events: EventBus (tokio broadcast)
│   │   │   ├── locale/                 # mesh-core-locale: LocaleEngine
│   │   │   └── theme/                  # mesh-core-theme: ThemeEngine, token lookup
│   │   ├── platform/
│   │   │   └── wayland/                # mesh-core-wayland: ShellSurface trait, Layer, Edge
│   │   ├── runtime/
│   │   │   ├── backend/                # mesh-core-backend: spawn_backend_service()
│   │   │   ├── host/                   # mesh-core-runtime: stub future sandbox host
│   │   │   └── scripting/              # mesh-core-scripting: mlua Luau VM bridge
│   │   └── ui/
│   │       ├── component/              # mesh-core-component: .mesh file parser
│   │       ├── elements/               # mesh-core-elements: element model, WidgetNode, layout
│   │       ├── icon/                   # mesh-core-icon: XDG icon resolution + cache
│   │       └── render/                 # mesh-core-render: painter, PixelBuffer, Wayland bridge
│   └── tools/
│       ├── cli/                        # mesh-tools-cli: mesh-shell binary
│       └── lsp/                        # mesh-tools-lsp: .mesh language server
├── packages/                           # Plugin ecosystem
│   └── plugins/
│       ├── frontend/
│       │   ├── core/                   # Built-in surface + widget frontend plugins
│       │   │   ├── panel/              # Top panel surface
│       │   │   ├── launcher/           # App launcher (content_measured surface)
│       │   │   ├── quick-settings/     # Quick settings drawer
│       │   │   ├── notification-center/# Notification history
│       │   │   ├── notification-feed/  # Notification feed widget
│       │   │   ├── notification-sidebar/
│       │   │   ├── volume-slider/      # Audio volume popover
│       │   │   ├── volume-bar/
│       │   │   ├── navigation-bar/     # Nav bar (uses components/ subdirectory)
│       │   │   ├── base-surface/       # Dev sandbox surface
│       │   │   ├── base-launcher-widget/
│       │   │   └── base-sidebar-widget/
│       │   └── examples/               # Example/demo frontend plugins
│       │       ├── agenda-list/
│       │       ├── calendar-card/
│       │       ├── date-strip/
│       │       ├── focus-timer/
│       │       ├── habit-streaks/
│       │       ├── status-rail/
│       │       ├── weather-brief/
│       │       └── workspace-hub/
│       ├── backend/
│       │   └── core/                   # Built-in service backend plugins (Luau)
│       │       ├── pipewire-audio/     # Audio via PipeWire (wpctl)
│       │       ├── pulseaudio-audio/   # Audio via PulseAudio (pactl)
│       │       ├── mpris-media/        # Media via MPRIS (playerctl)
│       │       ├── networkmanager-network/ # Network via nmcli
│       │       ├── upower-power/       # Power/battery via upower
│       │       ├── mock-notifications/ # Dev fake notifications
│       │       ├── shell-theme/        # Theme metadata emitter
│       │       ├── audio-interface/    # Interface contract for audio
│       │       ├── network-interface/  # Interface contract for network
│       │       ├── power-interface/    # Interface contract for power
│       │       ├── media-interface/    # Interface contract for media
│       │       ├── notifications-interface/
│       │       └── brightness-interface/
│       └── icon-packs/
│           └── papirus/                # Papirus icon pack (plugin.json only, no assets)
├── spec/                               # Feature specs and design documents
└── tools/                              # Shell-level tooling scripts
```

## Directory Purposes

**`crates/core/shell/src/shell/`:**
- Purpose: Shell orchestrator — the central coordinator
- Key files:
  - `mod.rs` (~1536 lines) — `Shell` struct, main event loop, plugin discovery, IPC handling
  - `component.rs` — `FrontendSurfaceComponent` (one instance per surface plugin)
  - `types.rs` — `CoreRequest`, `CoreEvent`, `ServiceEvent`, `ShellComponent` trait, `SurfaceId`
  - `surface_layout.rs` — `surface_layout_from_manifest()`, `SurfaceLayoutSettings`, `SurfaceSizePolicy`
  - `service.rs` — service name resolution helpers
  - `ipc.rs` — Unix socket IPC server (Tokio async)
  - `layout.rs` — surface positioning helpers
  - `sounds.rs` — shell event sound playback
  - `render/bridge/` — bridges to rendering layer

**`crates/core/ui/render/src/surface/`:**
- Purpose: All software rendering: painter, pixel buffer, icon decode, text layout, Wayland/dev-window bridge
- Key files:
  - `painter.rs` — `FrontendRenderEngine`: walks `WidgetNode`, calls `LayoutEngine`, draws pixels
  - `buffer.rs` — `PixelBuffer` (RGBA software framebuffer)
  - `icon.rs` — `draw_icon_from_path()`, PNG decode, SVG rasterization via `resvg`
  - `text.rs` — `SharedTextMeasurer`, `cosmic-text` integration, `register_font_dir()`
  - `debug_overlay.rs` — `DebugOverlay` painter
  - `bridge/wayland_surface.rs` — production Wayland layer-shell surface via smithay
  - `bridge/dev_window.rs` — `minifb` dev window fallback

**`crates/core/ui/elements/src/`:**
- Purpose: Shared UI intermediate representation — no rendering, no Wayland, no service knowledge
- Key files:
  - `element.rs` — `ElementKind` enum, `ElementTypeDef`, `ELEMENT_TYPE_DEFS` const array
  - `tree.rs` — `WidgetNode`, `NodeId`, `ElementState`
  - `layout.rs` — `LayoutEngine`, `LayoutRect`, `TextMeasurer` trait
  - `style.rs` — `ComputedStyle`, `StyleResolver`, all CSS property types (`Color`, `Dimension`, `FlexDirection`, etc.)
  - `events.rs` — `EventDispatcher`, `UiEvent`, `InputState`
  - `accessibility.rs` — `AccessibilityTree`, `AccessibilityRole`, `AccessibilityState`

**`crates/core/ui/component/src/`:**
- Purpose: Parse `.mesh` files into typed AST; no runtime dependencies
- Key files:
  - `parser.rs` / `parser/` — top-level `parse_component()`, sub-parsers for template, script, styles, markup
  - `template.rs` — `TemplateBlock`, `TemplateNode`, `TemplateElement`
  - `style.rs` — `StyleBlock` type
  - `lib.rs` — `ComponentFile`, `ComponentImport`, `ComponentImportTarget`

**`crates/core/runtime/scripting/src/`:**
- Purpose: Luau VM bridge (mlua); host APIs; reactive state sync
- Key files:
  - `context.rs` — `ScriptContext`, `ScriptState`, `ScriptError`, `LocaleBoundState`, `PublishedEvent`
  - `backend.rs` — `BackendScriptContext`, `BackendScriptError`
  - `host_api.rs` — `mesh.*` Lua global implementations (`mesh.exec_shell`, `mesh.service.*`, etc.)

**`crates/core/extension/plugin/src/`:**
- Purpose: Manifest loading and lifecycle management
- Key files:
  - `manifest.rs` — `Manifest`, `PluginType`, `SurfaceLayoutSection`, JSON/TOML parsers
  - `lifecycle.rs` — `PluginInstance`, `PluginState`

**`crates/core/extension/service/src/`:**
- Purpose: Interface contract parsing and registry
- Key files:
  - `interface.rs` — `InterfaceRegistry`, `InterfaceProvider`, `InterfaceCatalog`, `canonical_interface_name()`
  - `contract.rs` — `InterfaceContract`, `InterfaceMethod`, `InterfaceEvent`, `load_interface_contract()`
  - `registry.rs` — `ServiceRegistry`, `ServiceEntry`

**`packages/plugins/frontend/core/<name>/`:**
- Purpose: One complete frontend surface or widget plugin
- Contains:
  - `plugin.json` — id, type, capabilities, entrypoints, settings schema, surface_layout
  - `src/main.mesh` — entrypoint `.mesh` component
  - `src/components/` — sub-components extracted per the component encapsulation rule
  - `config/settings.json` — user overrides (optional)
  - `config/i18n/<locale>.json` — translations (optional)

**`packages/plugins/backend/core/<name>/`:**
- Purpose: One Luau service backend plugin
- Contains:
  - `plugin.json` — id, type: "backend", provides interface, capabilities
  - `src/main.luau` — Luau script with `init()`, `on_poll()`, `on_command_*()` handlers

**`packages/plugins/backend/core/<name>-interface/`:**
- Purpose: Interface contract declaration (no implementation)
- Contains:
  - `plugin.json` — id, type: "interface"
  - `interface.toml` — methods, events, types, capability names

## Naming Conventions

**Crates:**
- Pattern: `mesh-core-<group>-<name>` (e.g. `mesh-core-ui-render` maps to dir `crates/core/ui/render/`)
- Flat name via `[package] name = "mesh-core-render"` in `Cargo.toml`

**Plugin IDs:**
- Pattern: `@mesh/<name>` (e.g. `@mesh/panel`, `@mesh/pipewire-audio`, `@mesh/audio-interface`)

**Plugin directories:**
- kebab-case matching the plugin name suffix (e.g. `navigation-bar`, `pipewire-audio`)

**`.mesh` files:**
- kebab-case filenames (e.g. `main.mesh`, `battery-button.mesh`, `wifi-item.mesh`)

**Component tags in templates:**
- PascalCase (e.g. `<VolumeButton />`, `<BatteryWidget />`) — must be explicitly imported in `<script>`

**Rust files:**
- snake_case modules; types are PascalCase; functions snake_case

## Where to Add New Code

**New frontend surface plugin:**
- Create `packages/plugins/frontend/core/<name>/`
- Add `plugin.json` with `"type": "surface"` and `surface_layout` section
- Add `src/main.mesh` as entrypoint
- Extract sub-components into `src/components/<component-name>.mesh`
- User settings override: `config/settings.json`
- Translations: `config/i18n/<locale>.json`

**New frontend component (sub-component of a plugin):**
- Add to `packages/plugins/frontend/core/<plugin-name>/src/components/<name>.mesh`
- Import explicitly in the parent `.mesh` file's `<script>` block: `import Name from "./components/<name>.mesh"`

**New backend service plugin:**
- Create `packages/plugins/backend/core/<name>/`
- Add `plugin.json` with `"type": "backend"`, `"provides": [{"interface": "@mesh/<interface>"}]`
- Add `src/main.luau` with `init()`, `on_poll()`, `on_command_*()` functions
- Use `mesh.exec_shell()` for system calls — never add Rust service logic

**New interface contract:**
- Create `packages/plugins/backend/core/<name>-interface/`
- Add `plugin.json` with `"type": "interface"`
- Add `interface.toml` declaring methods, events, types

**New CSS property:**
- Parse: `crates/core/ui/elements/src/style.rs`
- Apply during paint: `crates/core/ui/render/src/surface/painter.rs`

**New core element type:**
- Register: `crates/core/ui/elements/src/element.rs` in `ELEMENT_TYPE_DEFS`
- Paint: `crates/core/ui/render/src/surface/painter.rs`
- LSP knowledge: `crates/tools/lsp/src/knowledge/tags.rs`

**New `CoreRequest` action:**
- Add variant to `CoreRequest` enum: `crates/core/shell/src/shell/types.rs`
- Add match arm in `handle_request()`: `crates/core/shell/src/shell/mod.rs`

**New theme token:**
- Add to `config/themes/mesh-default-dark.json` and `config/themes/mesh-default-light.json`
- Reference in `.mesh` `<style>` blocks via `token(group.name)`

**New host API (`mesh.*` function for Luau):**
- Add to `crates/core/runtime/scripting/src/host_api.rs`
- Add capability gate if needed in `crates/core/foundation/capability/src/lib.rs`

**Utilities / shared helpers:**
- Foundation primitives: appropriate `crates/core/foundation/*/src/lib.rs`
- UI helpers: `crates/core/ui/elements/src/` (if no runtime deps) or `crates/core/ui/render/src/` (if rendering)

## Special Directories

**`crates/core/runtime/host/` (`mesh-core-runtime`):**
- Purpose: Stub / future Luau sandbox host (not yet implemented)
- Generated: No
- Committed: Yes

**`crates/core/ui/icon/assets/material/`:**
- Purpose: Bundled Material icon assets for offline/fallback icon resolution
- Generated: No (checked-in assets)
- Committed: Yes

**`packages/plugins/icon-packs/papirus/`:**
- Purpose: Papirus icon pack plugin declaration (only `plugin.json` — actual icon files are installed system-wide)
- Generated: No

**`target/`:**
- Purpose: Cargo build output
- Generated: Yes
- Committed: No (in `.gitignore`)

**`.claude/`:**
- Purpose: GSD workflow tooling (agents, commands, hooks) for AI-assisted development
- Generated: No
- Committed: Yes (`.claude/agents/`, `.claude/commands/`, `.claude/get-shit-done/`)

---

*Structure analysis: 2026-05-01*
