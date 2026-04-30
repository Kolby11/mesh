# MESH Codebase Context

This file is the primary orientation guide for an LLM working on this codebase.
It covers the crate map, plugin layout, key data flows, and common task entry points.

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
- **Frontend plugin**: a complete frontend implementation for a specific shell
  feature or capability. It has a `plugin.json`, entrypoint `.mesh`,
  capabilities, settings, optional exports, and can contain multiple
  components. For example, an audio controls frontend plugin may include
  components for the volume mixer, mute toggle, output selector, and device
  list.

When designing Lua access or intellisense, model this as:

```
MESH core elements -> user components -> frontend plugin
```

For example, `icon` is an element with core-defined fields such as `name`,
`src`, and `size`; `VolumeButton.mesh` is a component that composes `button`,
`icon`, and `text`; `@mesh/audio-controls` is a frontend plugin that packages
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
  └─ mesh-core-shell          ← shell orchestrator, owns the main event loop
       ├─ mesh-core-render ← compiles .mesh frontend plugins and paints WidgetNode trees
       │    ├─ mesh-core-component     ← parser for .mesh single-file components
       │    └─ mesh-core-elements   ← core element model, layout engine, style resolver, WidgetNode
       ├─ mesh-core-backend   ← Luau backend plugin polling and command runtime
       │    └─ mesh-core-scripting     ← Luau host APIs and script state bridge
       ├─ mesh-core-service   ← interface/service registry (InterfaceRegistry)
       ├─ mesh-core-plugin    ← manifest parsing (Manifest, plugin.json / mesh.toml)
       ├─ mesh-core-theme     ← token-based theming (ThemeEngine, Theme)
       ├─ mesh-core-locale    ← localization (LocaleEngine)
       ├─ mesh-core-events    ← typed event bus for inter-plugin communication
       ├─ mesh-core-config    ← shell-wide settings (ShellConfig, ShellSettings)
       ├─ mesh-core-capability← capability/permission model
       ├─ mesh-core-wayland   ← Wayland surface abstractions (ShellSurface, Layer)
       ├─ mesh-core-diagnostics ← DiagnosticsCollector, health reporting
       ├─ mesh-core-debug     ← DebugSnapshot, DebugOverlayState
       └─ mesh-core-runtime   ← (stub) future Luau sandbox host
```

### Key types per crate

| Crate                 | Key types / files                                                                                                                                |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `mesh-core-shell`     | `Shell` in `shell/mod.rs` — plugin host and shell orchestrator; `FrontendSurfaceComponent`, `ShellComponent` trait, `CoreRequest`, `CoreEvent`   |
| `mesh-core-plugin`    | `Manifest`, `PluginType`, `SurfaceLayoutSection` in `manifest.rs`; `PluginInstance` in `lifecycle.rs`                                            |
| `mesh-core-component` | `ComponentFile`, `parser.rs` — parses `<template>`, `<script>`, `<style>`, and `<i18n>` blocks                                                   |
| `mesh-core-render`    | `CompiledFrontendPlugin`, `FrontendCompositionResolver`, `RenderEngine`, `PixelBuffer`, `SharedTextMeasurer`, `LayerSurfaceConfig`               |
| `mesh-core-backend`   | `spawn_backend_service`, `BackendServiceCommand`, `BackendServiceUpdate`                                                                         |
| `mesh-core-scripting` | `ScriptContext`, `BackendScriptContext`, `ScriptState`, `LocaleBoundState`                                                                       |
| `mesh-core-elements`  | `ElementKind`, `ElementTypeDef`, `ElementSnapshot`, `WidgetNode`, `LayoutRect`, `StyleContext`, `StyleResolver`, `VariableStore`, `ElementState` |
| `mesh-core-service`   | `InterfaceRegistry`, `ServiceRegistry`, `InterfaceProvider`, `canonical_interface_name`                                                          |
| `mesh-core-theme`     | `ThemeEngine`, `Theme`, `default_theme()`, `load_theme_from_path()`                                                                              |
| `mesh-core-wayland`   | `ShellSurface` trait, `Layer`, `Edge`, `KeyboardMode`, `StubSurface`                                                                             |
| `mesh-core-config`    | `ShellConfig`, `ShellSettings`, `load_config()`, `load_shell_settings()`                                                                         |
| `mesh-core-events`    | `EventBus`                                                                                                                                       |

---

## Plugin Ecosystem

```
plugins/
  frontend/core/        ← built-in surface and widget plugins
    panel/              ← top panel (surface)
    launcher/           ← app launcher popover (surface, content_measured)
    quick-settings/     ← quick settings drawer (surface)
    notification-center/← notification history (surface)
    volume-slider/      ← audio volume popover (surface)
    navigation-bar/     ← nav bar widget used inside panel
    base-surface/       ← dev sandbox surface (surface)
    base-launcher-widget/
    base-sidebar-widget/
    notification-feed/
    notification-sidebar/

  backend/core/         ← service plugins (scripted backends, declared by plugin.json)
    pipewire-audio/     ← audio via PipeWire
    pulseaudio-audio/   ← audio via PulseAudio
    mpris-media/        ← media via MPRIS
    networkmanager-network/ ← network via NetworkManager
    upower-power/       ← power via UPower
    mock-notifications/ ← fake notifications for dev

    audio-interface/    ← interface contract for audio
    network-interface/  ← interface contract for network
    power-interface/    ← interface contract for power
    media-interface/    ← interface contract for media
    notifications-interface/ ← interface contract for notifications
    brightness-interface/
```

### Frontend plugin anatomy (`plugin.json`)

Every frontend plugin is a complete feature package. It declares in its
`plugin.json`:
- `type`: `"surface"` | `"widget"` | `"backend"` | `"interface"`
- `entrypoints.main`: path to the `.mesh` single-file component
- `settings.schema.surface.properties`: layout defaults (anchor, layer, width, height, etc.) — **user-editable**
- `surface_layout`: non-user renderer hints (`size_policy`, `prefers_content_children_sizing`, clamp bounds)
- `capabilities.required`: permission gates (`shell.surface`, `theme.read`, etc.)
- `dependencies.plugins`: plugin IDs this plugin depends on

Surface layout defaults live in `plugin.json`, **not** in Rust. `mesh-core-shell` reads them via `surface_layout_from_manifest()` in `shell.rs`.

### `.mesh` single-file component structure

```
<template>   ← XHTML-like markup with core elements, {expressions}, and component tags
<script lang="luau">   ← Luau scripting (state, lifecycle, event handlers)
<style>      ← CSS-like styling with token() references and @container queries
<i18n>       ← bundled translations (optional)
```

Components are reusable authoring units. They should be made from MESH core
elements (`button`, `icon`, `input`, etc.) or other components. Do not call a
full frontend plugin a component; the plugin is the package that owns settings,
capabilities, manifests, and one or more components.

**CRITICAL CODE STYLE**: Component files should be small and focused. Always extract layout sections, list items, and logically distinct UI blocks into their own separate components (e.g., in a `components/` subdirectory). Custom PascalCase component tags must be imported explicitly in the script block, such as `import ItemRow from "./components/item-row.mesh"`. This is especially important for items inside `{#for ...}` loops so they can encapsulate their own event state (like capturing list item IDs) instead of relying on DOM dataset attributes (which are not supported in event handlers).

---

## Key Data Flows

### Shell startup

1. `mesh-tools-cli` → `Shell::run()` in `mesh-core-shell/src/shell.rs`
2. Shell discovers plugins via `plugin_search_paths()` (workspace, `/usr/share/mesh`, `~/.local/share/mesh`)
3. Each plugin dir is loaded from manifest metadata; frontend plugins are compiled via `mesh-core-render`, backend plugins are hosted by `mesh-core-backend`
4. `FrontendSurfaceComponent::new()` is created per surface plugin:
   - reads `plugin.json` manifest → `surface_layout_from_manifest()` for layout defaults
   - reads `config/settings.json` → user overrides applied on top of manifest defaults
5. Shell enters the main event loop (Tokio runtime)

### Surface rendering

1. Wayland compositor calls `paint()` on `FrontendSurfaceComponent`
2. `build_tree()` → `ScriptContext` evaluates Luau state → `WidgetNode` tree
3. If `size_policy == ContentMeasured`: `measure_content_size()` uses manifest clamps
4. `Painter` walks the tree, resolves styles via `StyleResolver`, draws into `PixelBuffer`
5. Buffer committed to Wayland via `LayerShellBackend`

### Settings flow

```
plugin.json settings.schema.surface.properties[field].default
  ↓  (baseline)
surface_layout_from_manifest()
  ↓  + user overrides
config/settings.json  →  load_frontend_plugin_settings()
  ↓
FrontendSurfaceComponent.surface_layout / settings_json
  ↓
ScriptContext state["settings"]  ←  Luau reads {settings.surface.anchor}
```

### Service/interface flow

```
backend plugin (mesh.toml, provides = "mesh.audio")
  → registered in InterfaceRegistry
  → emits events on EventBus
  → Shell sets __mesh_svc_audio Lua table and calls on_change handlers via ScriptContext
  → frontend plugins use require("@mesh/audio") proxy to read state and call commands
```

---

## Common Task Entry Points

| Task                           | Where to start                                                                                                                  |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------- |
| Add a CSS property             | `mesh-core-elements/src/style.rs` (parse), `mesh-core-render/src/surface/painter.rs` (paint)                                    |
| Add a new surface plugin       | Create `packages/plugins/frontend/core/<name>/`, `plugin.json` with `"type": "surface"`, `src/main.mesh`                        |
| Change surface layout behavior | `surface_layout_from_manifest()` in `mesh-core-shell/src/shell.rs`; manifest's `surface_layout` section                         |
| Add a service (backend plugin) | `packages/plugins/backend/core/<name>/`, `plugin.json` + `src/main.luau`, implement the interface contract in the plugin script |
| Add a new CoreRequest action   | `CoreRequest` enum + match arm in `handle_request()` in `mesh-core-shell/src/shell.rs`                                          |
| Add a theme token              | `mesh-core-theme/src/lib.rs`, default theme JSON, then reference with `token(group.name)` in `.mesh`                            |
| Add localization               | Plugin's `<i18n>` block or `config/i18n/<locale>.json`; `LocaleEngine` in `mesh-core-locale`                                    |
| Debug rendering                | `ToggleDebugOverlay` / `CoreRequest::CycleDebugTab`; see `mesh-core-debug/src/lib.rs`                                           |
| Plugin manifest parsing        | `mesh-core-plugin/src/manifest.rs` — `JsonManifest`, `TomlManifest`, `into_manifest()`                                          |
| Fix icons                      | See "Icon System" section below — four specific files need changes                                                              |

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
  → painter (mesh-core-render/src/surface/painter.rs:138) — reads name/src/size attrs
  → resolve_icon_path(name, size) in mesh-core-icon/src/lib.rs — BROKEN (see fix 1)
  → draw_icon_from_path(buffer, path, ...) in mesh-core-render/src/surface/icon.rs — BROKEN for SVG (see fix 2)
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

### Fix 2 — SVG rasterization (`crates/core/ui/render/src/surface/icon.rs`)

The `"svg"` match arm is an empty TODO. To fix it:
1. Add `resvg = "0.44"` to `crates/core/ui/render/Cargo.toml`
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

### Fix 3 — Remove the purple placeholder (`crates/core/ui/render/src/style.rs`)

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

### Fix 4 — Add decoded image caching (`crates/core/ui/render/src/surface/icon.rs`)

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

<!-- Icon from a file path (absolute or plugin-relative) -->
<icon src="{plugin_dir}/assets/logo.svg"/>

<!-- Icon with CSS sizing (overrides the 18px default) -->
<icon name="battery-full" style="width: 20px; height: 20px;"/>
```

Size attribute is used only for XDG resolution hints; actual rendered size is always the layout box size.

---

## Conventions

### CRITICAL: Core is a wiring layer only

**The shell core must never implement service logic.** This is the single most important architectural rule.

Core's only job is:
- discover plugins
- load manifests
- wire plugins to the event bus
- forward service events to frontend state

Everything else — reading audio volume, querying network status, checking battery, calling system tools like `wpctl` or `pactl` — belongs exclusively in backend plugins, written in Luau using the exec host API.

Backend plugins should always be implemented in Luau, or in the plugin's
respective scripting language if the runtime grows beyond Luau. Do not move
service-specific parsing, polling, command shaping, or fallback behavior into
Rust just because the current host API is missing a helper.

**If you find Rust code in `mesh-core-shell` that calls system tools, spawns polling loops for a specific service, or has `if service_name == "audio"` style branches, that is a bug, not a pattern to follow.**

The exec host API in `mesh-core-scripting` is what enables backend Luau plugins to call system commands. When it does not exist yet for a given capability, the right fix is to implement a generic host API primitive — not to move the logic into core.

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
-- packages/plugins/backend/core/pipewire-audio/src/main.luau
local volume = exec("wpctl", {"get-volume", "@DEFAULT_AUDIO_SINK@"})
service.emit("audio", { volume = parse_volume(volume) })
```

Core wires the plugin. The plugin does the work.

### CRITICAL: Frontend and backend plugins are standalone — each owns its own state

**Frontend plugins must never read derived state that was injected by core.** Each plugin computes its own display state from the raw service payload inside its `<script>` block.

Backend plugins emit raw data (volume percent, mute flag, etc.). Frontend plugins transform that into display-ready state (icon names, labels, formatted strings) inside their own scripts.

The mechanism: when a service update arrives, core calls `on_<service>_update()` on the frontend script if it declares that handler. The handler reads from `audio.*` (the raw payload) and writes to local script variables, which the template then binds to.

Example of what is WRONG:
```rust
// mesh-core-shell/src/shell/service.rs — core computing display state
let icon_name = audio_icon_name(percent, muted); // core should not know about this
obj.insert("icon_name", icon_name);
```

Example of what is RIGHT:
```lua
-- packages/plugins/frontend/core/volume-slider/src/main.mesh <script>
local audio = require("@mesh/audio@>=1.0")

icon_name = "audio-volume-muted"
audio_label = "0%"

audio.on_change(function()
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
end)

function onVolumeUp()   audio.volume_up()   end
function onVolumeDown() audio.volume_down() end
```

The template binds `{icon_name}` — a reactive global, not a service field.

**If you find core injecting computed display fields (icon names, formatted labels, derived booleans) into service payloads, that is a bug.**

### Reactive state

Any bare global assignment in the `<script>` block is automatically reactive — no registration needed. Globals are synced to `ScriptState` after each handler call and exposed to templates via `{key}`. `local` variables are private to the script.

```lua
icon_name = "audio-volume-muted"  -- reactive, visible in template as {icon_name}
local helper = function() end     -- private, not synced
```

### Interface proxies

`require("@mesh/audio@>=1.0")` returns a proxy for the named backend service. Use it as a Lua local:

```lua
local audio = require("@mesh/audio@>=1.0")

-- Read state fields (populated when backend emits)
local p = audio.percent   -- number
local m = audio.muted     -- boolean

-- Register a change handler (called on every backend update)
audio.on_change(function()
    icon_name = audio.muted and "audio-volume-muted" or "audio-volume-high"
end)

-- Call commands (published as events to the backend)
audio.volume_up()
audio.toggle_mute()
```

LSP completions for `audio.` derive state fields and commands by analyzing the backend `main.luau` — no separate type declarations required.

---

- **Everything is a plugin.** The shell core must not hardcode plugin IDs or behavior. Layout defaults, size policies, and content sizing are declared in `plugin.json`, not in Rust match arms.
- **`mesh-core-shell/src/shell.rs` is large** (~4000 lines). When reading it, use `Grep` to find specific functions rather than reading the whole file.
- **Frontend plugins are compiled at startup**, not interpreted at runtime. Hot-reload is supported via file watching (`reload_plugin_settings`, `source_path()` watching).
- **Globals are reactive state.** Any global assigned in `<script>` is synced to `ScriptState` after each call. Templates bind to `{variable_name}`. `local` variables are private.
- **Surface layout is user-configurable.** Any surface can have its anchor, layer, size, keyboard mode overridden via `config/settings.json` inside the plugin directory.
- **`SurfaceSizePolicy::ContentMeasured`** means the surface resizes itself to fit its content. Declared in `plugin.json` as `surface_layout.size_policy = "content_measured"`. Only the launcher uses this currently.
- **Test location:** unit tests live in `#[cfg(test)]` modules at the bottom of each source file.
