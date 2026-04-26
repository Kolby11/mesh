# MESH Codebase Context

This file is the primary orientation guide for an LLM working on this codebase.
It covers the crate map, plugin layout, key data flows, and common task entry points.

---

## Crate Map

Each crate in `crates/` has a single responsibility. The dependency arrow goes
downward — lower crates know nothing about higher ones.

```
mesh-cli
  └─ mesh-core          ← shell orchestrator, owns the main event loop
       ├─ mesh-component-backend  ← compiles + runs .mesh frontend plugins
       │    ├─ mesh-component     ← parser for .mesh single-file components
       │    └─ mesh-scripting     ← Luau bridge (ScriptContext, ScriptState)
       ├─ mesh-renderer  ← paints WidgetNode trees into pixel buffers
       │    └─ mesh-ui   ← layout engine, style resolver, WidgetNode
       ├─ mesh-service   ← interface/service registry (InterfaceRegistry)
       ├─ mesh-plugin    ← manifest parsing (Manifest, plugin.json / mesh.toml)
       ├─ mesh-theme     ← token-based theming (ThemeEngine, Theme)
       ├─ mesh-locale    ← localization (LocaleEngine)
       ├─ mesh-events    ← typed event bus for inter-plugin communication
       ├─ mesh-config    ← shell-wide settings (ShellConfig, ShellSettings)
       ├─ mesh-capability← capability/permission model
       ├─ mesh-wayland   ← Wayland surface abstractions (ShellSurface, Layer)
       ├─ mesh-diagnostics ← DiagnosticsCollector, health reporting
       ├─ mesh-debug     ← DebugSnapshot, DebugOverlayState
       └─ mesh-runtime   ← (stub) future Luau sandbox host
```

### Key types per crate

| Crate | Key types / files |
|---|---|
| `mesh-core` | `Shell` in `shell.rs` — owns everything; `FrontendSurfaceComponent`, `ShellComponent` trait, `CoreRequest`, `CoreEvent` |
| `mesh-plugin` | `Manifest`, `PluginType`, `SurfaceLayoutSection` in `manifest.rs`; `PluginInstance` in `lifecycle.rs` |
| `mesh-component` | `ComponentFile`, `parser.rs` — parses `<template>`, `<script>`, `<style>`, `<schema>` blocks |
| `mesh-component-backend` | `CompiledFrontendPlugin`, `FrontendCatalog`, `FrontendCompositionResolver` |
| `mesh-scripting` | `ScriptContext`, `ScriptState`, `LocaleBoundState` — the only crate crossing the UI/service boundary |
| `mesh-ui` | `WidgetNode`, `LayoutRect`, `StyleContext`, `StyleResolver`, `VariableStore`, `ElementState` |
| `mesh-renderer` | `Painter`, `PixelBuffer`, `SharedTextMeasurer`, `LayerShellBackend`, `LayerSurfaceConfig` |
| `mesh-service` | `InterfaceRegistry`, `ServiceRegistry`, `InterfaceProvider`, `canonical_interface_name` |
| `mesh-theme` | `ThemeEngine`, `Theme`, `default_theme()`, `load_theme_from_path()` |
| `mesh-wayland` | `ShellSurface` trait, `Layer`, `Edge`, `KeyboardMode`, `StubSurface` |
| `mesh-config` | `ShellConfig`, `ShellSettings`, `load_config()`, `load_shell_settings()` |
| `mesh-events` | `EventBus` |

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

  backend/core/         ← service plugins (Rust, mesh.toml manifests)
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

Every frontend plugin declares in its `plugin.json`:
- `type`: `"surface"` | `"widget"` | `"backend"` | `"interface"`
- `entrypoints.main`: path to the `.mesh` single-file component
- `settings.schema.surface.properties`: layout defaults (anchor, layer, width, height, etc.) — **user-editable**
- `surface_layout`: non-user renderer hints (`size_policy`, `prefers_content_children_sizing`, clamp bounds)
- `capabilities.required`: permission gates (`shell.surface`, `theme.read`, etc.)
- `dependencies.plugins`: plugin IDs this plugin depends on

Surface layout defaults live in `plugin.json`, **not** in Rust. `mesh-core` reads them via `surface_layout_from_manifest()` in `shell.rs`.

### `.mesh` single-file component structure

```
<template>   ← XHTML-like markup with {expressions} and component tags
<script lang="luau">   ← Luau scripting (state, lifecycle, event handlers)
<style>      ← CSS-like styling with token() references and @container queries
<schema>     ← typed settings schema (optional)
<i18n>       ← bundled translations (optional)
<meta>       ← accessibility metadata (optional)
```

---

## Key Data Flows

### Shell startup

1. `mesh-cli` → `Shell::run()` in `mesh-core/src/shell.rs`
2. Shell discovers plugins via `plugin_search_paths()` (workspace, `/usr/share/mesh`, `~/.local/share/mesh`)
3. Each plugin dir is compiled: backend plugins via `PluginInstance`, frontend via `compile_frontend_plugin()`
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
  → Shell applies to ScriptState via apply_service_update()
  → frontend reads state["audio"] in Luau / template bindings
```

---

## Common Task Entry Points

| Task | Where to start |
|---|---|
| Add a CSS property | `mesh-ui/src/style.rs` (parse), `mesh-renderer/src/painter.rs` (paint) |
| Add a new surface plugin | Create `plugins/frontend/core/<name>/`, `plugin.json` with `"type": "surface"`, `src/main.mesh` |
| Change surface layout behavior | `surface_layout_from_manifest()` in `mesh-core/src/shell.rs`; manifest's `surface_layout` section |
| Add a service (backend plugin) | `plugins/backend/core/<name>/`, `mesh.toml` with `[service]`, implement the interface contract |
| Add a new CoreRequest action | `CoreRequest` enum + match arm in `handle_request()` in `mesh-core/src/shell.rs` |
| Add a theme token | `mesh-theme/src/lib.rs`, default theme JSON, then reference with `token(group.name)` in `.mesh` |
| Add localization | Plugin's `<i18n>` block or `config/i18n/<locale>.json`; `LocaleEngine` in `mesh-locale` |
| Debug rendering | `ToggleDebugOverlay` / `CoreRequest::CycleDebugTab`; see `mesh-debug/src/lib.rs` |
| Plugin manifest parsing | `mesh-plugin/src/manifest.rs` — `JsonManifest`, `TomlManifest`, `into_manifest()` |
| Fix icons | See "Icon System" section below — four specific files need changes |

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
  → parser (mesh-component/src/parser.rs) — already works
  → WidgetNode { tag: "icon", attributes: { name, size } }
  → painter (mesh-renderer/src/painter.rs:138) — reads name/src/size attrs
  → resolve_icon_path(name, size) in mesh-icon/src/lib.rs — BROKEN (see fix 1)
  → draw_icon_from_path(buffer, path, ...) in mesh-renderer/src/icon.rs — BROKEN for SVG (see fix 2)
```

### Fix 1 — XDG icon resolution (`crates/mesh-icon/src/lib.rs`)

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

### Fix 2 — SVG rasterization (`crates/mesh-renderer/src/icon.rs`)

The `"svg"` match arm is an empty TODO. To fix it:
1. Add `resvg = "0.44"` to `crates/mesh-renderer/Cargo.toml`
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
                               mesh_ui::style::Color {
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

### Fix 3 — Remove the purple placeholder (`crates/mesh-component-backend/src/lib.rs`)

Find the `"icon"` arm in `default_style_for_tag()` (around line 1128). It currently sets `background_color = #7f67be` and `border_radius = 9`. This purple box is a debug placeholder — it masks broken icons by always showing something.

Replace with:
```rust
"icon" => {
    let mut style = ComputedStyle::default();
    style.width = mesh_ui::Dimension::Px(18.0);
    style.height = mesh_ui::Dimension::Px(18.0);
    style.background_color = mesh_ui::Color::TRANSPARENT;
    style
}
```

Size defaults remain (18px) because without a `size` attribute the painter uses `w.max(h)` which needs to be non-zero.

### Fix 4 — Add decoded image caching (`crates/mesh-renderer/src/icon.rs`)

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

**If you find Rust code in `mesh-core` that calls system tools, spawns polling loops for a specific service, or has `if service_name == "audio"` style branches, that is a bug, not a pattern to follow.**

The exec host API in `mesh-scripting` is what enables backend Luau plugins to call system commands. When it does not exist yet for a given capability, the right fix is to implement the host API — not to move the logic into core.

Example of what is WRONG:
```rust
// mesh-core/src/shell/audio.rs  ← this file should not exist
if service_name == "audio" {
    runtime.spawn(spawn_audio_backend_service(...)); // core doing service work
}
```

Example of what is RIGHT:
```lua
-- plugins/backend/core/pipewire-audio/src/main.luau
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
// mesh-core/src/shell/service.rs — core computing display state
let icon_name = audio_icon_name(percent, muted); // core should not know about this
obj.insert("icon_name", icon_name);
```

Example of what is RIGHT:
```lua
-- plugins/frontend/core/volume-slider/src/main.mesh <script>
local icon_name = "audio-volume-muted"

function on_audio_update()
    if audio.muted or audio.percent == 0 then
        icon_name = "audio-volume-muted"
    elseif audio.percent < 67 then
        icon_name = "audio-volume-medium"
    else
        icon_name = "audio-volume-high"
    end
end
```

The template then binds `{icon_name}` — a local script variable, not a service field.

**If you find core injecting computed display fields (icon names, formatted labels, derived booleans) into service payloads, that is a bug.**

### Reactive service bindings

Frontend scripts declare reactive bindings to service fields with `mesh.service.bind("service.field")`:

```lua
local muted = mesh.service.bind("audio.muted")
local percent = mesh.service.bind("audio.percent")
```

When the `audio` backend emits, core:
1. Updates `state["audio"]` (full payload accessible from templates as `{audio.percent}`)
2. Copies bound fields into local script variables (`muted`, `percent`)
3. Calls `on_audio_update()` if the script declares it

This lets `on_audio_update()` read simple local variables in conditions rather than needing object property access (`audio.muted`), which the stub interpreter does not support.

**Two-way binding** (planned): writing to a bound variable will publish the change back to the backend service via an event channel.

**Interpreter limitation**: `elseif` is not yet supported. Use nested `if/else/end` chains instead.

---

- **Everything is a plugin.** The shell core must not hardcode plugin IDs or behavior. Layout defaults, size policies, and content sizing are declared in `plugin.json`, not in Rust match arms.
- **`mesh-core/src/shell.rs` is large** (~4000 lines). When reading it, use `Grep` to find specific functions rather than reading the whole file.
- **Frontend plugins are compiled at startup**, not interpreted at runtime. Hot-reload is supported via file watching (`reload_plugin_settings`, `source_path()` watching).
- **Luau state is the bridge** between services and UI. Backend plugins emit string updates; `apply_service_update()` parses them into `ScriptState`; templates bind to `{state.field}`.
- **Surface layout is user-configurable.** Any surface can have its anchor, layer, size, keyboard mode overridden via `config/settings.json` inside the plugin directory.
- **`SurfaceSizePolicy::ContentMeasured`** means the surface resizes itself to fit its content. Declared in `plugin.json` as `surface_layout.size_policy = "content_measured"`. Only the launcher uses this currently.
- **Test location:** unit tests live in `#[cfg(test)]` modules at the bottom of each source file.
