# Settings pages and shell surfaces

Backlog: the "Settings pages and shell surfaces" item in
[`docs/BACKLOG.md`](../../../docs/BACKLOG.md).
Design canvas: <https://claude.ai/artifact/3bgtVEKxDojXhcD8m33ksY> — five
themes (Nocturne, Paper, Tapedeck, Bubblegum, Liquid Glass), each with a desktop, lock
screen, quick settings, notification centre, launcher, overlays, component kit
and 17 settings pages. The first board maps every page to its backing service.

## Goal

A settings app and shell surface set that covers what a working desktop OS
ships, built only from token-driven components so any theme restyles it.

## Settings pages

| Page | Backed by | Today |
| --- | --- | --- |
| Wi-Fi & Network | `mesh.network` | page exists; add VPN, hotspot, proxy, IP config |
| Bluetooth | `mesh.bluetooth` | page exists; interface and BlueZ provider are new |
| Sound | `mesh.audio` | page exists; add per-app streams, input meter |
| Display | `mesh.brightness`, `mesh.wm` outputs | new page; outputs and modes go on `mesh.wm` |
| Keyboard | `mesh.wm-keybinds` | page exists; add key_priority, hint mode, hint corner |
| Mouse & Touchpad | `mesh.wm` input options | new page and contract |
| Power | `mesh.power`, `mesh.session` | new page; idle timers need the session contract |
| Appearance | core settings, `mesh.wallpapers` | page exists; add accent, wallpaper, scale |
| Notifications | `mesh.notifications` | new page; per-app policy store |
| Language & Time | core locale, `mesh.system` | new page; time zone and NTP provider |
| Accessibility | core settings | new page |
| Apps & Defaults | `mesh.apps` | new page; MIME defaults and autostart |
| Privacy & Security | core capabilities, `mesh.session` | new page; grant matrix is core-owned |
| Shell Layout | core profiles | partly in composition-editor |
| Modules | core packages | Advanced page; add install and update |
| Users | `mesh.session` | new page, read-mostly |
| About | `mesh.device`, `mesh.system` | page exists |

## Shell surfaces

The five desktop profiles compose the floating panel and shelf, menu strip,
taskbar, or side rails. They share quick settings, notification center,
application launcher, power controls and a volume/brightness OSD.
Remaining desktop cards include the Bubblegum calendar and tasks, Paper agenda,
and Tapedeck file shortcuts. Context menus and dialogs need their artboard
treatments. The session lock screen and window switcher still need shell
integration.

## Order

1. Extend the component kit across the remaining board-specific layouts.
2. Pages that need no new service (Accessibility, Notifications policy, Apps).
3. Contracts for Display, Pointer, Bluetooth, Session, then their pages.
4. Lock screen and window switcher.
