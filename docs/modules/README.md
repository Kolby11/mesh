# Shipped Modules

This index is generated from canonical `module.json` manifests currently under
`modules/`. Directories without a manifest are source fragments or unfinished
modules and are not listed as installable modules.

All shipped defaults are ordinary modules. They receive no hidden privilege
from their `@mesh` scope and may be replaced by compatible third-party modules.

## UI modules

All are `ui` modules. Placement is `surface` when the module declares default
`mesh.surface` placement and `embedded` when other components or profiles mount it.

| Module | Placement | Entrypoint | Purpose |
| --- | --- | --- | --- |
| `@mesh/navigation-bar` | surface | `src/main.mesh` | Top bar with start/center/end `mesh.navigation.item` regions; defaults place the widget modules below |
| `@mesh/workspaces-widget`, `@mesh/window-title-widget`, `@mesh/clock-widget`, `@mesh/brightness-widget`, `@mesh/volume-widget`, `@mesh/theme-widget`, `@mesh/language-widget`, `@mesh/battery-widget`, `@mesh/settings-widget` | embedded | `src/main.mesh` | Independent `mesh.navigation.item` contributions any bar can host; per-instance props come from profile placements |
| `@mesh/audio-popover` | embedded | `src/main.mesh` | Audio control surface |
| `@mesh/quick-settings` | embedded | `src/main.mesh` | Quick settings surface |
| `@mesh/settings` | surface | `src/main.mesh` | Settings surface using the shared glass material family and active theme palette |
| `@mesh/debug-inspector` | surface | `src/main.mesh` | Runtime inspection surface |
| `@mesh/composition-editor` | surface | `src/main.mesh` | Replaceable visual editor for author-declared component slots |
| `@mesh/text-selection-proof` | surface | `src/main.mesh` | Text-selection proof surface |
| `@mesh/touch-gesture-proof` | surface | `src/main.mesh` | Touch and gesture proof surface |
| `@mesh/blur-test` | surface | `src/main.mesh` | Isolated `backdrop-filter`/blur debugging surface (disabled by default) |
| `@mesh/opacity-test` | surface | `src/main.mesh` | Opacity control surface without compositor blur (disabled by default) |
| `@mesh/language-popover` | embedded | `src/main.mesh` | Embeddable language chooser |
| `@mesh/theme-selector` | embedded | `src/main.mesh` | Embeddable theme chooser |
| `@mesh/glass-bar` | surface | `src/main.mesh` | Floating glass islands with clock/calendar, workspaces, media, recording, weather and a live audio/brightness control center |
| `@mesh/glass-widgets` | surface | `src/main.mesh` | Responsive desktop media and weather cards with colour artwork and a three-point forecast |
| `@mesh/keybind-help` | surface | `src/main.mesh` | Keyboard shortcuts window opened by `F1`, listing every binding by surface |
| `@mesh/cheatsheet` | surface | `src/main.mesh` | Dark compositor-shortcut cheatsheet: the window manager's described keybinds in sections and columns of keycaps; reads `mesh.wm-keybinds`; open it with `mesh-shell` IPC `shell:toggle_surface:@mesh/cheatsheet` |
| `@mesh/radial-launcher` | surface | `src/main.mesh` | Radial overlay: a ring of app wedges, a clock hub and media wedges, built from `<shape>` sectors |
| `@mesh/halo-orb` | surface | `src/main.mesh` | Always-visible orb that toggles the radial launcher, with the shell switcher beside it |
| `@mesh/glass-dock` | surface | `src/main.mesh` | Glass shelf with neighboring-icon magnification and a searchable launcher with grid and list views of installed applications |
| `@mesh/glass-notifications` | surface | `src/main.mesh` | Glass notification toasts under the bar; shows itself while a notification is popped up |
| `@mesh/glass-dashboard` | surface | `src/main.mesh` | Overlay with system meters and history, the current track with a live spectrum, notifications and weather |
| `@mesh/glass-wallpapers` | surface | `src/main.mesh` | Overlay wallpaper picker: tilted carousel and colour strip; applies through `mesh.theme.set_backdrop_image`; not in the glass composition |
| `@mesh/wallpaper-carousel` | surface | `src/main.mesh` | Wallpaper selector widget under the bar: endless strip of tall cards that slide, shrink with distance and pan across `mesh.wallpapers` panoramas |
| `@mesh/wallpaper-parallax` | surface | `src/main.mesh` | Wallpaper picker under the bar: endless strip of five slanted cards that slide while their upright `mesh.wallpapers` panoramas pan in parallax; the glass composition's wallpaper selector |
| `@mesh/profile-switcher` | embedded | `src/main.mesh` | Shell switcher trigger; also a `mesh.navigation.item` contribution |
| `@mesh/profile-menu` | embedded | `src/main.mesh` | Popover listing profiles; selecting one switches live via `mesh.packages` |

The glass composition follows the liquid-glass showcase's floating islands,
light rims, refracted translucent cards and raised dock lenses. The clock opens
a month calendar; the rightmost status button opens quick
controls; the dock's search button opens the application launcher. The bar's
dashboard and wallpaper buttons toggle those overlays. The dashboard closes with
Escape, its close button or a click outside the panel. The wallpaper carousel
is a widget under the bar, centred horizontally, driven by arrows,
PageUp/PageDown and the wheel; Enter, Space or a click on the centre card
applies the wallpaper and closes it, and Escape closes it. Notification toasts
anchor to the left edge so they sit under the bar's start islands.
The media card grows a
spectrum strip while audio plays. Desktop media and weather cards expose a size
button that toggles resize mode: drag the bottom-right handle or focus it and
use arrow keys (10 px, Shift for 1 px); Enter/Escape finishes, and Reset restores
responsive defaults. Explicit component size inputs keep the painted card, its
layout wrapper and the spectrum width synchronized. Custom dimensions
persist per component instance and remain bounded by the output; the spectrum
follows the media card width. Desktop widgets use the bottom layer, so wallpaper
clients must use the background layer to stay behind them. Their frost uses the
actual compositor backdrop rather than the configured wallpaper image. Media
controls follow service playback state, progress advances while playing, and
track changes swipe in the requested skip direction. Seeking previews the pointer
position locally, commits once on release, and holds the preview until a matching
playback sample arrives (with a bounded timeout); keyboard seeks commit immediately. Dashboard system meters and CPU history bars
keep fixed layout boxes and animate their bottom-anchored vertical scale, so
fill motion does not require layout on each frame. The dashboard's 24 spectrum
bars also keep fixed 44 px boxes and apply bottom-anchored scale directly from
audio samples, retaining the 4 px minimum without queued height transitions. Popups
support keyboard opening, Escape and outside dismissal through core disclosure.
Control labels are available in English and Slovak. Audio and brightness use
capability-gated providers; unavailable controls stay disabled. The full Settings
surface shares the same material family, including translucent panels, rims and
hover states, while preserving the active theme's colors.

Desktop cards use the configurable `top_offset` prop (360px by default), compact
widths and a stacked layout at 960px or below. The dock reserves space above the
shelf for its selected and neighboring lenses to rise. Motion uses theme timing
and easing, with separate entrance animations for islands, cards and popups.

## Service providers

| Module | Kind | Entrypoint | Interface/integration |
| --- | --- | --- | --- |
| `@mesh/pipewire-audio` | backend | `src/main.luau` | `mesh.audio` through native PipeWire events and control tools |
| `@mesh/pulseaudio-audio` | backend | `src/main.luau` | `mesh.audio` through PulseAudio tools |
| `@mesh/backlight-brightness` | backend | `src/main.luau` | `mesh.brightness` through `brightnessctl` |
| `@mesh/upower-power` | backend | `src/main.luau` | Power state through UPower tooling |
| `@mesh/hyprland-wm` | backend | `src/main.luau` | Hyprland shell/window-manager integration |
| `@mesh/device-info` | backend | `src/main.luau` | Linux device information for system/settings surfaces |
| `@mesh/mpris-media` | backend | `src/main.luau` | `mesh.media` through `playerctl`, with cached cover art |
| `@mesh/open-meteo-weather` | backend | `src/main.luau` | `mesh.weather` from Open-Meteo, located by settings or IP |
| `@mesh/networkmanager-network` | backend | `src/main.luau` | `mesh.network` through `nmcli` |
| `@mesh/hyprland-apps` | backend | `src/main.luau` | `mesh.apps`: desktop entries joined with Hyprland clients |
| `@mesh/wf-recorder` | backend | `src/main.luau` | `mesh.recorder` through `wf-recorder` |
| `@mesh/status-notifier-tray` | backend | `src/main.luau` | `mesh.tray`: StatusNotifier watcher/host, item state and controls |
| `@mesh/freedesktop-notifications` | backend | `src/main.luau` | `mesh.notifications`: owns `org.freedesktop.Notifications` through `mesh.dbus_serve` |
| `@mesh/proc-system` | backend | `src/main.luau` | `mesh.system` from `/proc`, hwmon and `df` |
| `@mesh/cava-visualizer` | backend | `src/main.luau` | `mesh.visualizer` spectrum bars from `cava` |
| `@mesh/wallpaper-directory` | backend | `src/main.luau` | `mesh.wallpapers` from one directory, with ImageMagick thumbnails and 16:9 panoramas |
| `@mesh/hyprland-keybinds` | backend | `mesh.wm-keybinds` provider: described `hl.bind` calls from the Hyprland Lua config, grouped by `--##! Section` comments |

MPRIS follows metadata and playback status, resamples position every two seconds
for external seeks, and downloads artwork separately from playback updates.
Queries and controls target the player selected by the metadata stream; if it
exits, sampling falls back to an available player.
Playback position stays provider-owned after a seek. The desktop slider previews
its drag locally, then reconciles with playback; the seek command does not pin
the shared position to its requested value, because playback may already have
advanced when the next sample arrives.
Queued metadata and cava frames publish the newest sample from each batch;
stale artwork requests are cancelled when the track's art URL changes.

## Interfaces and resource packs

| Module | Kind | Contribution |
| --- | --- | --- |
| `@mesh/audio-interface` | interface | `mesh.audio` contract |
| `@mesh/wm-interface` | interface | Compositor-neutral `mesh.wm` workspace/window contract |
| `@mesh/power-interface` | interface | `mesh.power` battery contract |
| `@mesh/brightness-interface` | interface | `mesh.brightness` backlight contract |
| `@mesh/device-interface` | interface | Base device-information interface for settings/system surfaces |
| `@mesh/media-interface` | interface | `mesh.media` now-playing contract |
| `@mesh/weather-interface` | interface | `mesh.weather` conditions and forecast contract |
| `@mesh/network-interface` | interface | `mesh.network` connectivity and Wi-Fi contract |
| `@mesh/apps-interface` | interface | `mesh.apps` pinned, running and installed application contract |
| `@mesh/tray-interface` | interface | `mesh.tray` item state and activation contract |
| `@mesh/notifications-interface` | interface | `mesh.notifications` desktop notification contract |
| `@mesh/system-interface` | interface | `mesh.system` processor, memory, disk, temperature and network load |
| `@mesh/visualizer-interface` | interface | `mesh.visualizer` audio spectrum contract |
| `@mesh/wallpapers-interface` | interface | `mesh.wallpapers` wallpaper library contract |
| `@mesh/wm-keybinds-interface` | interface | `mesh.wm-keybinds` compositor keybind contract: sections of described binds |
| `@mesh/recorder-interface` | interface | `mesh.recorder` screen-recording contract |
| `@mesh/liquid-glass` | theme | Neutral white glass palette over a deep blue base with system-blue accents; tunes `@mesh/glass-material` |
| `@mesh/frutiger-aero` | theme | Frutiger Aero palette: frosted-white glass, sky and aqua blues, grass-green accents, Noto Sans; light (default) and dark modes; tunes `@mesh/glass-material` |
| `@mesh/vitrum` | theme | Vitrum palette, after the niri glass shell: lit-from-within glass tinted by a violet accent with a mint second hue, rounded parts, Inter; dark (default) and light modes; tunes `@mesh/glass-material` |
| `@mesh/volume-presentation` | library | Luau `require("@mesh/volume-presentation/volume")`: level normalization, scroll steps and the semantic volume icon shared by every audio frontend |
| `@mesh/glass-material` | library | Shared surface gradients, translucent fills, text, accents, rims, hovers, shadows and `--glass-backdrop-{pill,card,dock,panel,flare,lens}` blur/refraction materials; defaults derive from any theme, with Liquid Glass and Frutiger Aero tuning them |
| `@mesh/shell-ui-interface` | interface | Named UI extension-point contracts a host surface renders foreign module UI into |
| `@mesh/icons-default` | icon-pack | Default semantic icon mappings |
| `@mesh/icons-material-symbols` | icon-pack | Material Symbols font and codepoint mappings |

## Compositions

A composition module (`mesh.kind: "composition"`) binds root component
instances, surface placement, and provider selection into one named, active
desktop layout. See [module-system §5](../spec/01-module-system.md).

| Module | Kind | Contribution |
| --- | --- | --- |
| `@mesh/halo` | composition | A bare desktop with one orb that opens the radial launcher |
| `@mesh/glass` | composition | Liquid-glass desktop: glass bar, desktop cards, dock, notification toasts, dashboard and wallpaper carousel; recommends the liquid glass theme |
| `@mesh/desk` | composition | The default desktop: a top navigation bar and a settings window, bound to the PipeWire and backlight providers |

## Module anatomy

An installable module contains one canonical manifest. A UI module exports one
primary/default public component, can declare additional public contributions,
and can keep internal components private. Data-only modules need no script or
component entry. A typical executable module looks like:

```text
module-name/
├── module.json
├── src/
│   └── main.mesh or main.luau
├── config/            optional catalogs/settings data
└── assets/            optional module-owned assets
```

UI modules use `.mesh`; backend providers use Luau;
interfaces are declarative data; and resource packs map semantic names to
assets. A `ui` module's optional `mesh.surface` sets its default placement and
whether direct install mounts it; profiles can mount any `ui` module. See the [platform philosophy](../spec/00-philosophy.md) and
[module-system specification](../spec/01-module-system.md).
