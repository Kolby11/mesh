# External Integrations

**Analysis Date:** 2026-05-01

## System Service Integrations (via Backend Plugins in Luau)

All system integrations are implemented as Luau backend plugins under
`packages/plugins/backend/core/`. The Rust core never calls system tools
directly — it only wires plugins to the event bus.

**Audio:**
- PipeWire — `packages/plugins/backend/core/pipewire-audio/src/main.luau`
  - Polls via `wpctl status` + `wpctl get-volume`; commands via `wpctl set-volume`, `wpctl set-mute`
  - Interface contract: `packages/plugins/backend/core/audio-interface/interface.toml`
- PulseAudio — `packages/plugins/backend/core/pulseaudio-audio/src/main.luau`
  - Alternative provider for the same `@mesh/audio` interface
  - Uses `pactl` via `mesh.exec_shell`

**Network:**
- NetworkManager — `packages/plugins/backend/core/networkmanager-network/src/main.luau`
  - Polls via `nmcli`; interface contract: `packages/plugins/backend/core/network-interface/interface.toml`

**Power / Battery:**
- UPower — `packages/plugins/backend/core/upower-power/src/main.luau`
  - Polls via `upower`; interface contract: `packages/plugins/backend/core/power-interface/interface.toml`

**Media:**
- MPRIS — `packages/plugins/backend/core/mpris-media/src/main.luau`
  - Media player control via `playerctl` or direct MPRIS D-Bus; interface contract: `packages/plugins/backend/core/media-interface/interface.toml`

**Notifications:**
- Mock — `packages/plugins/backend/core/mock-notifications/src/main.luau`
  - Dev-only fake notification emitter; interface contract: `packages/plugins/backend/core/notifications-interface/interface.toml`

**Brightness:**
- Interface declared: `packages/plugins/backend/core/brightness-interface/interface.toml`
- No live implementation plugin detected

**Theme Backend:**
- `packages/plugins/backend/core/shell-theme/src/main.luau` — emits active theme metadata

## Wayland Protocol Integrations

**Layer Shell:**
- `wlr-layer-shell-v1` — required compositor capability; used by all surface plugins for panel, launcher, quick-settings, etc.
- Client: `smithay-client-toolkit` 0.19; `crates/core/ui/render/src/surface/bridge/wayland_surface.rs`

**Keyboard / Input:**
- `xkbcommon` — keyboard event translation via `smithay-client-toolkit`'s xkbcommon feature

**Dev Fallback:**
- `minifb` (Wayland feature) — software dev window for testing without a compositor; `crates/core/ui/render/src/surface/bridge/dev_window.rs`

## Icon System

**XDG Icon Theme:**
- Resolution: `crates/core/ui/icon/src/lib.rs` — searches `~/.local/share/icons`, `~/.icons`, `/usr/share/icons`, `/usr/share/pixmaps`
- Cache: `static ICON_CACHE: OnceLock<Mutex<HashMap<(String, u32), Option<PathBuf>>>>`
- Icon pack plugins: `packages/plugins/icon-packs/papirus/plugin.json` (Papirus icon pack declared)
- Material icon assets bundled: `crates/core/ui/icon/assets/material/`

**SVG Rendering:**
- `resvg` 0.44 — rasterizes SVG icons at paint time; `crates/core/ui/render/src/surface/icon.rs`

**PNG/JPEG Rendering:**
- `image` 0.24 — decodes raster icons; static image cache in `crates/core/ui/render/src/surface/icon.rs`

## IPC

**Unix Socket:**
- Path: `$XDG_RUNTIME_DIR/mesh-shell.sock` (overridden via `MESH_IPC_SOCKET`)
- Protocol: newline-delimited text commands (e.g. `shell:open_launcher`, `shell:debug_overlay`)
- Server: `crates/core/shell/src/shell/ipc.rs` — Tokio async Unix socket listener
- Client: `mesh-shell ipc <command>` via `crates/core/tools/cli/src/main.rs`

## Font System

**Font Database:**
- `fontdb` 0.23 — builds font database from system font directories
- `cosmic-text` 0.18 — shaped text layout; `crates/core/ui/render/src/surface/text.rs`
- Custom font registration: `register_font_dir()` exported from `mesh-core-render`

## LSP Tooling

**Language Server Protocol:**
- `tower-lsp` 0.20 — LSP server framework for the `.mesh` file language server
- Binary: `mesh-tools-lsp` (`crates/tools/lsp/`)
- Provides: completions, hover, diagnostics for `.mesh` template, script, and style blocks

## Observability

**Logging:**
- `tracing` + `tracing-subscriber` — structured logging with env filter
- Initialized in `crates/tools/cli/src/main.rs` via `tracing_subscriber::fmt().with_env_filter()`

**Debug Overlay:**
- In-process overlay painted over live surfaces: `crates/core/foundation/debug/src/lib.rs`
- Toggled via `CoreRequest::ToggleDebugOverlay` / IPC command `shell:debug_overlay`

**Diagnostics:**
- `DiagnosticsCollector` in `crates/core/foundation/diagnostics/src/lib.rs`
- Tracks per-plugin `HealthStatus` (Healthy / Degraded / Error), metrics (frame time, memory)

## Environment Configuration

**Required env vars:**
- None strictly required at runtime (all have defaults)

**Optional env vars:**
- `MESH_IPC_SOCKET` — custom IPC socket path
- `RUST_LOG` — tracing log level filter

**Config files (loaded at startup):**
- `config/shell-settings.json` — active theme ID, locale; loaded by `load_shell_settings()`
- `config/mesh-default-dark.json`, `config/mesh-default-light.json` — bundled theme token files
- `packages/plugins/frontend/core/<name>/config/settings.json` — per-plugin user overrides
- `packages/plugins/frontend/core/<name>/config/i18n/<locale>.json` — plugin translations

## Webhooks & Callbacks

**Incoming:**
- Unix socket IPC only (see IPC section above)

**Outgoing:**
- None — MESH is a shell consumer, not a web service

---

*Integration audit: 2026-05-01*
