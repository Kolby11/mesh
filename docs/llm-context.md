# MESH Codebase Context

This file is the primary orientation guide for an LLM working on this codebase.
It covers the crate map, module layout, key data flows, and common task entry points.

---

## CRITICAL: Terminology

Use these terms precisely. A lot of MESH design depends on keeping this
hierarchy clear:

- **Element**: a base UI primitive exposed by MESH core. Examples include
  `box`, `row`, `column`, `button`, `icon`, `input`, `slider`, `switch`,
  `checkbox`, `text`, `image`, and `separator`. Elements have predefined
  runtime behavior: layout participation, style resolution, accessibility
  metadata, event routing, render-derived metrics, and the Lua-facing API that
  scripts and LSP types should expose.
- **Component**: a user-authored reusable `.mesh` unit composed from base
  elements and, optionally, other components. Components own their template,
  Luau state and handlers, styles, schema, translations, and metadata. A
  component is an authoring abstraction, not a built-in core primitive.
- **Frontend module**: a complete frontend implementation for a specific shell
  feature or capability. It has a `module.json`, entrypoint `.mesh`,
  capabilities, settings, optional exports, and can contain multiple
  components. For example, an audio controls frontend module may include
  components for the volume mixer, mute toggle, output selector, and device
  list.

When designing Lua access or intellisense, model this as:

```
MESH core elements -> user components -> frontend module
```

For example, `icon` is an element with core-defined fields such as `name`,
`src`, and `size`; `VolumeButton.mesh` is a component that composes `button`,
`icon`, and `text`; `@mesh/audio-controls` is a frontend module that packages
multiple audio-related components into a complete UI.

The LSP must use the shared `mesh-core-elements` element model for `refs.<name>`
completion, hover, and diagnostics. Do not duplicate the Lua-facing fields in
LSP-only tables; the runtime and tooling should agree on what `IconElement`,
`InputElement`, and base `MeshElement` expose.

---

## Crate Map

Each crate in `crates/` has a single responsibility. The dependency arrow goes
downward — lower crates know nothing about higher ones.

```
mesh-tools-cli
  └─ mesh-core-shell          ← shell orchestrator, owns the main event loop and glues runtime/rendering
       ├─ mesh-core-frontend ← frontend engine in crates/core/frontend/compiler; compiles .mesh modules and builds WidgetNode trees
       │    ├─ mesh-core-component     ← parser for .mesh single-file components
       │    └─ mesh-core-elements   ← core element model, layout engine, style resolver, WidgetNode
       ├─ mesh-core-frontend-host ← frontend component host contract types
       ├─ mesh-core-animation ← easing, transitions, interpolation, and keyframe playback
       │    └─ mesh-core-elements   ← computed style value types used by animation
       ├─ mesh-core-interaction ← widget-tree hit testing, focus traversal, and scroll helpers
       │    └─ mesh-core-elements   ← WidgetNode and computed style data queried by interaction helpers
       ├─ mesh-core-surface-config ← manifest/settings surface layout resolution
       ├─ mesh-core-render ← render engine in crates/core/frontend/render; paints WidgetNode trees into PixelBuffer surfaces
       │    ├─ mesh-core-elements   ← WidgetNode and computed style data consumed by the painter
       │    └─ mesh-core-icon       ← icon pack resolution and glyph assets
       ├─ mesh-core-presentation ← presentation backends in crates/core/presentation; owns windows/layer-shell surfaces and input events
       │    └─ mesh-core-render       ← PixelBuffer handoff for presented frames
       ├─ mesh-core-backend   ← Luau backend module polling and command runtime
       │    └─ mesh-core-scripting     ← Luau host APIs and script state bridge
       ├─ mesh-core-service   ← interface/service registry (InterfaceRegistry)
       ├─ mesh-core-module    ← manifest parsing (canonical module.json plus migration diagnostics for old inputs)
       ├─ mesh-core-theme     ← token-based theming (ThemeEngine, Theme)
       ├─ mesh-core-locale    ← localization (LocaleEngine)
       ├─ mesh-core-events    ← typed event bus for inter-module communication
       ├─ mesh-core-config    ← shell-wide settings (ShellConfig, ShellSettings)
       ├─ mesh-core-capability← capability/permission model
       ├─ mesh-core-wayland   ← Wayland surface abstractions (ShellSurface, Layer)
       ├─ mesh-core-diagnostics ← DiagnosticsCollector, health reporting
       ├─ mesh-core-debug     ← DebugSnapshot, DebugOverlayState
       └─ mesh-core-runtime   ← sandbox policy metadata in crates/core/runtime/sandbox
```

### Key types per crate

| Crate                 | Key types / files                                                                                                                                |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `mesh-core-shell`     | `Shell` in `shell/mod.rs` — module host, backend/service orchestrator, and event loop                                                            |
| `mesh-core-module`    | `Manifest`, `ModuleType`, `SurfaceLayoutSection` in `manifest.rs`; `ModuleInstance` in `lifecycle.rs`                                            |
| `mesh-core-component` | `ComponentFile`, `parser.rs` — parses `<template>`, `<script>`, `<style>` blocks                                                                 |
| `mesh-core-frontend`  | `CompiledFrontendModule`, `FrontendCompositionResolver`, `FrontendRenderMode`, `compile_frontend_module()`                                       |
| `mesh-core-frontend-host` | `ShellComponent`, `CoreRequest`, `CoreEvent`, `ServiceEvent`, `ComponentInput`, `ComponentContext`, `ComponentError`, `SurfaceId`         |
| `mesh-core-animation` | `Easing`, `Interpolate`, `AnimatableStyle`, `KeyframeRule`, `ActiveKeyframeAnimation`                                                            |
| `mesh-core-interaction` | `find_node_path_at`, `find_focusable_at`, `collect_focus_traversal`, `annotate_overflow_tree`, `ScrollOffsetState`                             |
| `mesh-core-surface-config` | `SurfaceLayoutSettings`, `SurfaceSizePolicy`, `load_frontend_module_settings()`                                                            |
| `mesh-core-render`    | `PixelBuffer`, `FrontendRenderEngine`, `DebugOverlay`, `SharedTextMeasurer`, `TextRenderer`                                                      |
| `mesh-core-presentation` | `PresentationEngine`, `PresentationError`, `WindowEvent`, `LayerSurfaceConfig`                                                                |
| `mesh-core-backend`   | `spawn_backend_service`, `BackendServiceCommand`, `BackendServiceUpdate`                                                                         |
| `mesh-core-scripting` | `ScriptContext`, `BackendScriptContext`, `ScriptState`, `LocaleBoundState`                                                                       |
| `mesh-core-elements`  | `ElementKind`, `ElementTypeDef`, `ElementSnapshot`, `WidgetNode`, `LayoutRect`, `StyleContext`, `StyleResolver`, `VariableStore`, `ElementState` |
| `mesh-core-service`   | `InterfaceRegistry`, `ServiceRegistry`, `InterfaceProvider`, `canonical_interface_name`                                                          |
| `mesh-core-theme`     | `ThemeEngine`, `Theme`, `default_theme()`, `load_theme_from_path()`                                                                              |
| `mesh-core-wayland`   | `ShellSurface` trait, `Layer`, `Edge`, `KeyboardMode`, `StubSurface`                                                                             |
| `mesh-core-config`    | `ShellConfig`, `ShellSettings`, `load_config()`, `load_shell_settings()`                                                                         |
| `mesh-core-events`    | `EventBus`                                                                                                                                       |

---

## Module Ecosystem

```
modules/
  frontend/             ← built-in frontend modules
    navigation-bar/     ← shipped top-edge navigation frontend module
    audio-popover/      ← audio popover frontend module
    text-selection-proof/ ← disabled proof frontend module

  backend/              ← scripted backend provider modules declared by module.json
    pipewire-audio/     ← audio via PipeWire
    pulseaudio-audio/   ← audio via PulseAudio
    networkmanager-network/ ← network via NetworkManager

  interfaces/
    audio/              ← @mesh/audio-interface contract for mesh.audio
    audio.toml          ← legacy source copy kept during migration
```

### Canonical module workflow

New modules use `module.json`. Frontend modules depend on interface contracts
such as `mesh.audio`, never backend provider IDs. Backend providers implement
interfaces with `mesh.implements`; the shipped graph keeps both
`@mesh/pipewire-audio` and `@mesh/pulseaudio-audio` enabled while
`config/module.json` selects `@mesh/pipewire-audio` as the active
`mesh.audio` provider. Contributions cover layout, settings, keybinds, icon
requirements, resource packs, and libraries. Service-specific behavior belongs
in Luau provider modules, not service-specific Rust APIs; Rust routes generic
interface/provider records.

```
@mesh/navigation-bar
  -> require("mesh.audio@>=1.0")
  -> mesh.audio interface contract from @mesh/audio-interface
  -> active provider selected by config/module.json
  -> provider script in modules/backend/<provider>/src/main.luau
```

### Frontend module anatomy (`module.json`)

Every frontend module is a complete feature module. It declares in its
canonical author-facing manifest, `module.json`:
- `mesh.kind`: `"frontend"` for frontend modules
- `mesh.entrypoints.main`: path to the `.mesh` single-file component
- `mesh.contributes.settings.schema.surface.properties`: layout defaults (anchor, layer, width, height, etc.) — **user-editable**
- `mesh.surfaceLayout`: non-user renderer hints (`size_policy`, `prefers_content_children_sizing`, clamp bounds)
- `mesh.capabilities.required`: permission gates (`shell.surface`, `theme.read`, etc.)
- `mesh.dependencies.modules`: module IDs this module depends on

Surface layout defaults live in `module.json`, **not** in Rust. `mesh-core-shell` reads them via `surface_layout_from_manifest()` in `shell.rs`.

### `.mesh` single-file component structure

```
<template>   ← XHTML-like markup with core elements, {expressions}, and component tags
<script lang="luau">   ← Luau scripting (state, lifecycle, event handlers)
<style>      ← CSS-like styling with token() references and @container queries
```

Components are reusable authoring units. They should be made from MESH core
elements (`button`, `icon`, `input`, etc.) or other components. Do not call a
full frontend module a component; the module owns settings,
capabilities, manifests, and one or more components.

**CRITICAL CODE STYLE**: Component files should be small and focused. Always extract layout sections, list items, and logically distinct UI blocks into their own separate components (e.g., in a `components/` subdirectory). Custom PascalCase component definitions must be imported explicitly in the script block with Luau `require(...)`, such as `local ItemRow = require("./components/item-row.mesh")`. Markup instantiates the component, and `bind:this={item_row}` exposes the mounted instance when parent code needs to call public fields/functions. This is especially important for items inside `{#for ...}` loops so they can encapsulate their own event state (like capturing list item IDs) instead of relying on DOM dataset attributes (which are not supported in event handlers).

---

## Key Data Flows

### Shell startup

1. `mesh-tools-cli` → `Shell::run()` in `mesh-core-shell/src/shell.rs`
2. Shell discovers modules via `module_search_paths()` (workspace, `/usr/share/mesh`, `~/.local/share/mesh`)
3. Each module dir is loaded from manifest metadata; frontend modules are compiled via `mesh-core-frontend`, backend modules are hosted by `mesh-core-backend`
4. `FrontendSurfaceComponent::new()` is created per surface module:
   - reads `module.json` manifest → `surface_layout_from_manifest()` for layout defaults
   - reads `config/settings.json` → user overrides applied on top of manifest defaults
5. Shell enters the main event loop (Tokio runtime)

### Surface rendering

1. `Shell::render_components()` in `crates/core/shell/src/shell/runtime/render.rs` chooses dirty or visible surfaces.
2. `FrontendSurfaceComponent::render()` updates script/runtime state and `FrontendSurfaceComponent::paint()` builds or reuses the `WidgetNode` tree.
3. The component runtime keeps retained widget identity and dirty summaries in `crates/core/shell/src/shell/component/runtime_tree.rs`.
4. `mesh-core-render` paints the tree into a `PixelBuffer` through `paint_frontend_tree_at_for_module()`.
5. `mesh-core-presentation` commits the buffer through either the dev-window backend or the layer-shell backend and returns normalized input events to the shell.

### Settings flow

```
module.json mesh.contributes.settings.schema.surface.properties[field].default
  ↓  (baseline)
surface_layout_from_manifest()
  ↓  + user overrides
config/settings.json  →  load_frontend_module_settings()
  ↓
FrontendSurfaceComponent.surface_layout / settings_json
  ↓
ScriptContext state["settings"]  ←  Luau reads {settings.surface.anchor}
```

### Service/interface flow

```
backend module (`module.json` with `mesh.implements`)
  → registered in InterfaceRegistry
  → emits events on EventBus
  → Shell sets __mesh_svc_audio Lua table and calls on_change handlers via ScriptContext
  → frontend modules use require("mesh.audio@>=1.0") proxy to read state and call commands
```

---

## Common Task Entry Points

| Task                           | Where to start                                                                                                                   |
| ------------------------------ | -------------------------------------------------------------------------------------------------------------------------------- |
| Add a CSS property             | `crates/core/ui/component/src/style.rs` / parser modules (parse), `crates/core/ui/elements/src/style.rs` (computed style), `crates/core/frontend/render/src/surface/painter.rs` (paint) |
| Add a new frontend module      | Create `modules/frontend/<name>/`, `module.json` with `mesh.kind = "frontend"`, `src/main.mesh`                                  |
| Change surface layout behavior | `surface_layout_from_manifest()` in `mesh-core-shell/src/shell.rs`; manifest's `mesh.surfaceLayout` section                      |
| Add a backend provider module  | Create `modules/backend/<name>/`, `module.json` with `mesh.kind = "backend"` and `mesh.implements`, plus `src/main.luau`         |
| Add a new CoreRequest action   | `CoreRequest` enum in `crates/core/shell/src/shell/types.rs` plus request handling under `crates/core/shell/src/shell/runtime/request.rs` |
| Add a theme token              | `mesh-core-theme/src/lib.rs`, default theme JSON, then reference with `token(group.name)` in `.mesh`                             |
| Debug rendering                | `ToggleDebugOverlay` / `CoreRequest::CycleDebugTab`; see `crates/core/foundation/debug/src/lib.rs` and `crates/core/frontend/render/src/surface/debug_overlay.rs` |
| Module manifest parsing        | `mesh-core-module/src/manifest.rs` — `JsonManifest`, `TomlManifest`, `into_manifest()`                                           |
| Fix icons                      | See "Icon System" section below — four specific files need changes                                                               |

---

## Icon System

**Current state: icons are broken.** The parsing and painting plumbing exists, but icons never actually appear because of four separate bugs. Fix them in order.

### How icons are supposed to work

Template syntax (already parsed correctly — do not change the parser):

```xml
<icon name="audio-volume-high" size="24"/>
<icon src="/absolute/path/to/icon.svg"/>
```

The full pipeline:
```
<icon name="..."> in .mesh template
  → parser (mesh-core-component/src/parser.rs) — already works
  → WidgetNode { tag: "icon", attributes: { name, size } }
  → painter (crates/core/frontend/render/src/surface/painter/widgets.rs) — reads name/src/size attrs
  → resolve_icon_path(name, size) in mesh-core-icon/src/lib.rs — BROKEN (see fix 1)
  → draw_icon_from_path(buffer, path, ...) in crates/core/frontend/render/src/surface/icon.rs — BROKEN for SVG (see fix 2)
```

### Fix 1 — XDG icon resolution (`crates/core/ui/icon/src/lib.rs`)

The current resolver only checks 1 level deep. Real system icons live at:
```
/usr/share/icons/<theme>/<size>x<size>/<category>/<name>.png
/usr/share/icons/<theme>/scalable/<category>/<name>.svg
```

The fixed resolver must:
1. Build a candidate list of base directories (user before system):
   `~/.local/share/icons`, `~/.icons`, `/usr/share/icons`, `/usr/share/pixmaps`
2. For each base directory, iterate theme subdirectories (hicolor first as fallback)
3. Inside each theme, try these paths in order:
   - `<size>x<size>/<category>/<name>.png` — try common categories: `apps`, `devices`, `status`, `actions`, `places`, `mimetypes`
   - `scalable/<category>/<name>.svg` — same categories
   - `<name>.png` / `<name>.svg` directly in the theme root
4. After all themes, also try `<base>/<name>.png` and `<base>/<name>.svg` (pixmaps fallback)
5. Wrap the result in a **`HashMap<(String, u32), Option<PathBuf>>`** cache (a `static` via `std::sync::OnceLock<Mutex<HashMap<...>>>`) — icon resolution is called on every paint frame

Prefer PNG over SVG when both exist at the requested size; prefer SVG (scalable) when no PNG matches the size exactly. Return `None` if nothing is found — do not panic or log a warning on every miss.

### Fix 2 — SVG rasterization (`crates/core/frontend/render/src/surface/icon.rs`)

The `"svg"` match arm is an empty TODO. To fix it:
1. Add `resvg = "0.44"` to `crates/core/frontend/render/Cargo.toml`
2. In the `"svg"` arm:
   ```rust
   "svg" => {
       if let Ok(svg_data) = std::fs::read_to_string(path) {
           let mut opt = resvg::usvg::Options::default();
           opt.resources_dir = path.parent().map(|p| p.to_path_buf());
           if let Ok(tree) = resvg::usvg::Tree::from_str(&svg_data, &opt) {
               let w = dest_w.max(1) as u32;
               let h = dest_h.max(1) as u32;
               if let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(w, h) {
                   let scale_x = w as f32 / tree.size().width();
                   let scale_y = h as f32 / tree.size().height();
                   let transform = resvg::tiny_skia::Transform::from_scale(scale_x, scale_y);
                   resvg::render(&tree, transform, &mut pixmap.as_mut());
                   for py in 0..h {
                       for px in 0..w {
                           let idx = (py * w + px) as usize * 4;
                           let data = pixmap.data();
                           buffer.set_pixel(
                               (dest_x + px as i32) as u32,
                               (dest_y + py as i32) as u32,
                               mesh_core_elements::style::Color {
                                   r: data[idx],
                                   g: data[idx + 1],
                                   b: data[idx + 2],
                                   a: data[idx + 3],
                               },
                           );
                       }
                   }
               }
           }
       }
   }
   ```
   `resvg` re-exports `usvg` and `tiny_skia`, so no extra dependencies are needed beyond `resvg`.

### Fix 3 — Remove the purple placeholder (`crates/core/frontend/compiler/src/style.rs`)

Find the `"icon"` arm in `default_style_for_tag()` (around line 1128). It currently sets `background_color = #7f67be` and `border_radius = 9`. This purple box is a debug placeholder — it masks broken icons by always showing something.

Replace with:
```rust
"icon" => {
    let mut style = ComputedStyle::default();
    style.width = mesh_core_elements::Dimension::Px(18.0);
    style.height = mesh_core_elements::Dimension::Px(18.0);
    style.background_color = mesh_core_elements::Color::TRANSPARENT;
    style
}
```

Size defaults remain (18px) because without a `size` attribute the painter uses `w.max(h)` which needs to be non-zero.

### Fix 4 — Add decoded image caching (`crates/core/frontend/render/src/surface/icon.rs`)

`draw_icon_from_path` currently calls `image::open(path)` on every paint frame. With a 250ms poll interval, this runs ~4 times per second per icon. Add a static cache:

```rust
static IMAGE_CACHE: std::sync::OnceLock<std::sync::Mutex<
    std::collections::HashMap<std::path::PathBuf, image::RgbaImage>
>> = std::sync::OnceLock::new();

fn get_or_load(path: &std::path::Path) -> Option<image::RgbaImage> {
    let cache = IMAGE_CACHE.get_or_init(Default::default);
    let mut guard = cache.lock().unwrap();
    if let Some(img) = guard.get(path) {
        return Some(img.clone());
    }
    let img = image::open(path).ok()?.to_rgba8();
    guard.insert(path.to_path_buf(), img.clone());
    Some(img)
}
```

Use `get_or_load` instead of `image::open` in the PNG/JPEG arm. SVG does not need caching (resvg is fast and the output buffer is per-frame anyway).

### Template usage after fixes

Once all four fixes are in place, icons in `.mesh` files work like this:

```xml
<!-- System icon by name (resolves via XDG) -->
<icon name="audio-volume-high" size="24"/>
<icon name="network-wireless" size="16"/>

<!-- Icon from a file path (absolute or module-relative) -->
<icon src="{module_dir}/assets/logo.svg"/>

<!-- Icon with CSS sizing (overrides the 18px default) -->
<icon name="battery-full" style="width: 20px; height: 20px;"/>
```

Size attribute is used only for XDG resolution hints; actual rendered size is always the layout box size.

---

## Conventions

### CRITICAL: Core is a wiring layer only

**The shell core must never implement service logic.** This is the single most important architectural rule.

Core's only job is:
- discover modules
- load manifests
- wire modules to the event bus
- forward service events to frontend state

Everything else — reading audio volume, querying network status, checking battery, calling system tools like `wpctl` or `pactl` — belongs exclusively in backend modules, written in Luau using the exec host API.

Backend modules should always be implemented in Luau, or in the module's
respective scripting language if the runtime grows beyond Luau. Do not move
service-specific parsing, polling, command shaping, or fallback behavior into
Rust just because the current host API is missing a helper.

**If you find Rust code in `mesh-core-shell` that calls system tools, spawns polling loops for a specific service, or has `if service_name == "audio"` style branches, that is a bug, not a pattern to follow.**

The exec host API in `mesh-core-scripting` is what enables backend Luau modules to call system commands. When it does not exist yet for a given capability, the right fix is to implement a generic host API primitive — not to move the logic into core.

Backend scripts now run in a real Luau VM through `mlua`. Do not add new
hand-written backend parsers or mini-interpreters in Rust.

Frontend scripts run in a real Luau VM through `mlua` with no source preprocessing.
Do not add handwritten execution logic or mini-language semantics in Rust.

Example of what is WRONG:
```rust
// mesh-core-shell/src/shell/audio.rs  ← this file should not exist
if service_name == "audio" {
    runtime.spawn(spawn_audio_backend_service(...)); // core doing service work
}
```

Example of what is RIGHT:
```lua
-- packages/modules/backend/core/pipewire-audio/src/main.luau
local volume = exec("wpctl", {"get-volume", "@DEFAULT_AUDIO_SINK@"})
service.emit("audio", { volume = parse_volume(volume) })
```

Core wires the module. The module does the work.

### CRITICAL: Frontend and backend modules are standalone — each owns its own state

**Frontend modules must never read derived state that was injected by core.** Each module computes its own display state from the raw service payload inside its `<script>` block.

Backend modules emit raw data (volume percent, mute flag, etc.). Frontend modules transform that into display-ready state (icon names, labels, formatted strings) inside their own scripts.

The mechanism: render hooks and event handlers read from `audio.*` (the raw payload) and write to public script variables, which the template then binds to. Service field reads are tracked so changed fields rerender the affected component automatically.

Example of what is WRONG:
```rust
// mesh-core-shell/src/shell/service.rs — core computing display state
let icon_name = audio_icon_name(percent, muted); // core should not know about this
obj.insert("icon_name", icon_name);
```

Example of what is RIGHT:
```lua
-- packages/modules/frontend/core/volume-slider/src/main.mesh <script>
local audio = require("mesh.audio@>=1.0")

icon_name = "audio-volume-muted"
audio_label = "0%"

function render(self)
    local p = audio.percent or 0
    local m = audio.muted or false
    if m or p == 0 then
        icon_name = "audio-volume-muted"
    elseif p < 67 then
        icon_name = "audio-volume-medium"
    else
        icon_name = "audio-volume-high"
    end
    audio_label = string.format("%d%%", p)
end

function onVolumeUp()   audio.volume_up()   end
function onVolumeDown() audio.volume_down() end
```

The template binds `{icon_name}` — a reactive global, not a service field.

**If you find core injecting computed display fields (icon names, formatted labels, derived booleans) into service payloads, that is a bug.**

### Public/private script members

Any bare non-local assignment in the `<script>` block is a public component
member and is automatically reactive. Templates bind to public members with
`{key}`. `local` variables/functions are private to the script. Runtime hooks
receive `self`; use `self.meta` for current-instance identity/diagnostics and
`self.storage` for shell-backed JSON-like persistence scoped to the current
frontend component or backend provider instance.

```lua
icon_name = "audio-volume-muted"  -- public, visible in template as {icon_name}
local helper = function() end     -- private

function render(self)
    local id = self.meta.instance_id
end
```

### Interface proxies

`require("mesh.audio@>=1.0")` returns a proxy for the named backend service. Use it as a Lua local:

```lua
local audio = require("mesh.audio@>=1.0")

-- Read state fields (populated when backend emits)
local p = audio.percent   -- number
local m = audio.muted     -- boolean

-- Derive display state during render
function render(self)
    icon_name = audio.muted and "audio-volume-muted" or "audio-volume-high"
end

-- Call commands (published as events to the backend)
audio.volume_up()
audio.toggle_mute()
```

Declared interface events are direct named channels on the service proxy. The
old `audio.events.VolumeChanged:subscribe(...)` form remains compatibility
syntax.

```lua
audio.VolumeChanged:on(function(event)
    audio_label = string.format("%d%%", event.level)
end)
```

LSP completions for `audio.` derive state fields and commands by analyzing the backend `main.luau` — no separate type declarations required.

---

- **Everything is a module.** The shell core must not hardcode module IDs or behavior. Layout defaults, size policies, and content sizing are declared in `module.json`, not in Rust match arms.
- **`mesh-core-shell/src/shell.rs` is large** (~4000 lines). When reading it, use `Grep` to find specific functions rather than reading the whole file.
- **Frontend modules are compiled at startup**, not interpreted at runtime. Hot-reload is supported via file watching (`reload_module_settings`, `source_path()` watching).
- **Public/private script members.** Any non-local assigned in `<script>` is a public reactive member. Templates bind to `{variable_name}`. `local` variables/functions are private. Runtime hooks receive `self`; current-instance metadata and persistent storage live at `self.meta` and `self.storage`.
- **Surface layout is user-configurable.** Any surface can have its anchor, layer, size, keyboard mode overridden via `config/settings.json` inside the module directory.
- **`SurfaceSizePolicy::ContentMeasured`** means the surface resizes itself to fit its content. Declared in `module.json` as `mesh.surfaceLayout.size_policy = "content_measured"`. Only the launcher uses this currently.
- **Test location:** unit tests live in `#[cfg(test)]` modules at the bottom of each source file.
