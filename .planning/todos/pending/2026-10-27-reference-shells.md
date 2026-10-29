# Reference shells

Backlog: the first item in [`docs/BACKLOG.md`](../../../docs/BACKLOG.md).
Design canvas: <https://claude.ai/artifact/9eYjxQ6gkP957jtM5seiay> — one
artboard per shell plus a shared-component matrix.
References: [`docs/reference/shells/`](../../../docs/reference/shells/).

## Goal

Prove that MESH can host very different desktops from one core. Each shell is a
composition module with a profile. Any theme pack can be applied on top of any
shell, and switching profiles replaces the whole shell live.

## Shells

| Reference | Shell | State | New modules |
| --- | --- | --- | --- |
| `01-floating-widgets` | Orbit | Shipped (`@mesh/orbit`); cards lack data | data for the market, activity and usage cards |
| `02-pink-dashboard` | Bubblegum | — | left/right rails, phone-shaped control centre, weather, calendar, palette, profile, flip clock, to-do |
| `03-hyper-aero` | Hyper Aero | — | segmented top bar, chrono, media deck, sky arc, atmos, telemetry rail |
| `04-radial-launcher` | Halo | Shipped (`@mesh/halo`); no live session check, no tooltip-above-hub or volume wedge | volume strip in the hub, per-app window counts and pin badges |
| `05-retro-desktop` | Tapedeck | — | bevelled top taskbar with window list, desktop icons, tile dock, tapedeck player |
| `06-aero-desktop` | Skyglass | — | glass taskbar (orb, pinned/running apps, tray, clock), start panel, desktop icons |
| `07-dark-dashboard` | Nocturne | — | tabbed full-screen overlay (dashboard, workspaces, wallpapers) |
| `08-material-desktop` | Material | — | desktop edit mode with widget picker, configurable widget grid, top editing toolbar, side rail and bottom dock |
| `2001-desktop` | Dockyard | — | right-edge tile dock, workspace clip, root menu, minimized-window tiles |
| `liquid-glass-settings` | Glass | Shipped (`@mesh/glass`) | — |

Each new shell ships a composition module, a profile in `config/profiles/`,
and a default theme pack when no existing one fits (Bubblegum, Hyper Aero,
Tapedeck, Dockyard need one; Skyglass can start from `frutiger-aero`, Nocturne
from `liquid-glass` or a new dark pack). Shell components stay token-only, as
the shipped glass and orbit modules already are (no literal colours), so any
theme restyles any shell.

## Scope boundary

MESH draws panels, docks, launchers, menus, widgets and overlays. Window
decorations and app chrome in the retro and aero references belong to the
compositor and the apps' toolkit themes; the canvas marks them as dashed
outlines. A shell may ship a matching GTK/Qt theme recommendation, not code.

## Platform gaps

1. **Vector shapes.** `<shape>` draws sectors, arcs, polygons and polylines
   with hit-testing on the painted shape; Halo uses it. Still missing for the
   other shells: curves, gradient fills and a focus ring that follows the
   shape. Ring gauges, sparklines and the sky arc can start on what exists.
2. **Contract fields.** `mesh.weather` lacks sunrise/sunset, daily high/low,
   humidity, wind and UV; `mesh.system` lacks GPU, battery, CPU frequency,
   swap and network history. Extend the contracts and their providers.
3. **New data sources.** Orbit's market and usage cards need HTTP-backed
   providers; Bubblegum's palette needs several wallpaper colours (the
   wallpapers contract carries one); its to-do list uses scoped storage.
4. **Switching from anywhere.** Desk, Glass and Orbit each embed the profile
   switcher in their bar. New shells must too (Halo has no bar), and one
   shell-switch keybind should work in any profile, with the switch
   transition and reduced-motion support (second line of the backlog item).

## Order

1. Shell-switch keybind and switcher in every composition, so each new shell
   can be entered and left live.
2. Nocturne: mostly existing elements; first consumer of the shape element.
3. Vector shape element.
4. Hyper Aero, then Skyglass and Tapedeck (taskbars on `mesh.apps`, which
   already reports running, focused and per-window state).
5. Dockyard, Bubblegum.
6. Halo (done); remaining work is its live check.

## Verification per shell

- Real-surface tests like `real_surfaces/glass.rs`: compile, mount, paint at
  1920×1080 and a laptop size, keyboard reachability.
- A profile-switch test into and out of the shell that checks no orphaned
  surfaces remain.
- Paint the shell under at least two other theme packs.
- A live Wayland session check before deleting the shell's line here.
