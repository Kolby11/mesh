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
| `@mesh/navigation-bar` | surface | `src/main.mesh` | Main navigation surface |
| `@mesh/audio-popover` | embedded | `src/main.mesh` | Audio control surface |
| `@mesh/quick-settings` | embedded | `src/main.mesh` | Quick settings surface |
| `@mesh/settings` | surface | `src/main.mesh` | Current settings surface |
| `@mesh/debug-inspector` | surface | `src/main.mesh` | Runtime inspection surface |
| `@mesh/composition-editor` | surface | `src/main.mesh` | Replaceable visual editor for author-declared component slots |
| `@mesh/text-selection-proof` | surface | `src/main.mesh` | Text-selection proof surface |
| `@mesh/touch-gesture-proof` | surface | `src/main.mesh` | Touch and gesture proof surface |
| `@mesh/blur-test` | surface | `src/main.mesh` | Isolated `backdrop-filter`/blur debugging surface (disabled by default) |
| `@mesh/opacity-test` | surface | `src/main.mesh` | Opacity control surface without compositor blur (disabled by default) |
| `@mesh/language-popover` | embedded | `src/main.mesh` | Embeddable language chooser |
| `@mesh/theme-selector` | embedded | `src/main.mesh` | Embeddable theme chooser |

## Service providers

| Module | Kind | Entrypoint | Interface/integration |
| --- | --- | --- | --- |
| `@mesh/pipewire-audio` | backend | `src/main.luau` | `mesh.audio` through PipeWire tools |
| `@mesh/pulseaudio-audio` | backend | `src/main.luau` | `mesh.audio` through PulseAudio tools |
| `@mesh/backlight-brightness` | backend | `src/main.luau` | `mesh.brightness` through `brightnessctl` |
| `@mesh/upower-power` | backend | `src/main.luau` | Power state through UPower tooling |
| `@mesh/hyprland-wm` | backend | `src/main.luau` | Hyprland shell/window-manager integration |
| `@mesh/device-info` | backend | `src/main.luau` | Linux device information for system/settings surfaces |

## Interfaces and resource packs

| Module | Kind | Contribution |
| --- | --- | --- |
| `@mesh/audio-interface` | interface | `mesh.audio` contract |
| `@mesh/wm-interface` | interface | Compositor-neutral `mesh.wm` workspace/window contract |
| `@mesh/power-interface` | interface | `mesh.power` battery contract |
| `@mesh/brightness-interface` | interface | `mesh.brightness` backlight contract |
| `@mesh/device-interface` | interface | Base device-information interface for settings/system surfaces |
| `@mesh/shell-ui-interface` | interface | Named UI extension-point contracts a host surface renders foreign module UI into |
| `@mesh/icons-default` | icon-pack | Default semantic icon mappings |
| `@mesh/icons-material-symbols` | icon-pack | Material Symbols font and codepoint mappings |

## Compositions

A composition module (`mesh.kind: "composition"`) binds root component
instances, surface placement, and provider selection into one named, active
desktop layout. See [module-system §5](../spec/01-module-system.md).

| Module | Kind | Contribution |
| --- | --- | --- |
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
