# External Integrations

**Analysis Date:** 2026-05-06

## APIs & External Services

**Wayland / Compositor:**
- Wayland compositor - Displays shell surfaces as a Wayland client.
  - SDK/Client: `wayland-client` 0.31.14, `smithay-client-toolkit` 0.19.2, `minifb` 0.27.0 with `wayland`, plus internal abstractions in `crates/core/platform/wayland/src/lib.rs` and rendering code in `crates/core/ui/render/Cargo.toml`.
  - Auth: Local Wayland session environment; no app-level credentials detected.
  - Capability/compatibility: Frontend modules can declare compositor requirements such as `wlr-layer-shell-v1` in `modules/frontend/navigation-bar/package.json`.

**Desktop Shell IPC:**
- Local Unix socket IPC - CLI commands control a running shell instance.
  - SDK/Client: Rust standard library `std::os::unix::net::UnixStream` in `crates/tools/cli/src/main.rs`; Tokio `UnixListener` in `crates/core/shell/src/shell/ipc.rs`.
  - Auth: Local filesystem/socket permissions only; socket path is controlled by `MESH_IPC_SOCKET`, `XDG_RUNTIME_DIR`, or `/tmp/mesh-<uid>.sock` in `crates/core/shell/src/shell/mod.rs`.
  - Commands: `shell:debug_overlay`, `shell:debug_cycle_tab`, `shell:shutdown`, `shell:show_surface:<id>`, `shell:hide_surface:<id>`, and `shell:toggle_surface:<id>` in `crates/core/shell/src/shell/ipc.rs`.

**System Command Host API:**
- Generic `mesh.exec` host API - Backend Luau modules call external programs through `std::process::Command`.
  - SDK/Client: `mesh.exec(program, args)` in `crates/core/runtime/scripting/src/backend.rs`.
  - Auth: Capability-gated by `exec.<binary>` or `exec.command` in `crates/core/runtime/scripting/src/backend.rs`.
  - Current backends: `modules/backend/pipewire-audio/src/main.luau` calls `wpctl` and `aplay`; `modules/backend/pulseaudio-audio/src/main.luau` calls `pactl` and `aplay`.

**Audio Providers:**
- PipeWire / WirePlumber - Default active audio backend for `mesh.audio`.
  - SDK/Client: `wpctl` binary invoked by `modules/backend/pipewire-audio/src/main.luau`.
  - Auth: Required capabilities `exec.wpctl` and `exec.aplay` in `modules/backend/pipewire-audio/package.json` and `config/modules/@mesh/pipewire-audio/package.json`.
  - System packages: `wireplumber` for `wpctl`, `alsa-utils` for `aplay` from `modules/backend/pipewire-audio/package.json`.
  - Provider pin: `config/package.json` maps `mesh.audio` to `@mesh/pipewire-audio`.
- PulseAudio - Alternative installed audio backend for `mesh.audio`.
  - SDK/Client: `pactl` binary invoked by `modules/backend/pulseaudio-audio/src/main.luau`.
  - Auth: Required capabilities `exec.pactl` and `exec.aplay` in `modules/backend/pulseaudio-audio/package.json` and `config/modules/@mesh/pulseaudio-audio/package.json`.
  - System packages: `pulseaudio-utils` / `libpulse` for `pactl`, `alsa-utils` for `aplay`.
  - Provider priority: `@mesh/pulseaudio-audio` priority 50 versus PipeWire priority 100 in module manifests.

**Network / Power Provider Contracts:**
- NetworkManager - Default network provider module metadata exists for `mesh.network`.
  - SDK/Client: Not implemented in repo source; module metadata at `config/modules/@mesh/networkmanager/package.json`.
  - Auth: Not detected in manifest; future system access is expected to be capability-gated by interface/provider capabilities from `docs/module-system.md`.
- UPower - Default power provider module metadata exists for `mesh.power`.
  - SDK/Client: Not implemented in repo source; module metadata at `config/modules/@mesh/upower/package.json`.
  - Auth: Not detected in manifest; future system access is expected to be capability-gated.
- D-Bus - Architectural target for providers such as NetworkManager, UPower, MPRIS, or shared D-Bus helper libraries.
  - SDK/Client: Not implemented as a Rust or Luau host API in current source; documented capability names include `dbus.system` and D-Bus helper library examples in `docs/module-system.md`, `docs/extensibility.md`, and `spec/pluggable-backend.md`.
  - Auth: Planned capability identifiers such as `dbus.session` and `dbus.system`; no current D-Bus client crate detected in `Cargo.toml`.

**Module Distribution Sources:**
- MESH registry / HTTPS archive / Git / local path - Installation sources documented for package resolution.
  - SDK/Client: Installer design in `docs/installation.md`; no current network client crate such as `reqwest`, `hyper`, or `ureq` detected in `Cargo.toml`.
  - Auth: Signature/trust-tier design documented in `docs/installation.md` and `spec/pluggable-backend.md`; no implemented signing or credential storage detected in source.
  - Current local graph: `config/package.json` and `modules/**` are loaded from disk by `crates/core/extension/plugin/src/package.rs`.

**External Web APIs:**
- Web APIs are an allowed backend category in the module model.
  - SDK/Client: Not implemented in current source; `net.http` is documented as a capability in `docs/module-system.md`, `docs/extensibility.md`, and `spec/pluggable-backend.md`.
  - Auth: Not detected.

## Data Storage

**Databases:**
- Not detected.
  - Connection: Not applicable.
  - Client: No SQLite, PostgreSQL, MySQL, Redis, or ORM crate detected in `Cargo.toml` or `crates/**/Cargo.toml`.

**File Storage:**
- Local filesystem configuration and modules.
  - Root module graph: `config/package.json` in repo; default user root `~/.mesh/package.json` via `crates/core/extension/plugin/src/package.rs`.
  - Module directories: repo `modules/`, `~/.mesh/modules`, and `/usr/share/mesh/modules` via `crates/core/foundation/config/src/lib.rs`.
  - Shell config: `~/.config/mesh/config.toml` via `crates/core/foundation/config/src/lib.rs`.
  - Settings: `config/settings-default.json`, `config/shell-settings.json`, optional `MESH_SETTINGS_PATH`, and fallback `~/.mesh/settings.json` via `crates/core/foundation/config/src/lib.rs`.
  - Per-module overrides: legacy XDG config path under `mesh/plugins/<scope>/<name>.json` via `crates/core/foundation/config/src/lib.rs`.
  - Themes: `config/themes/*.json` and theme loading paths used by `mesh-core-theme`.
  - Icons/assets: `config/icons.toml`, bundled Material SVG assets under `crates/core/ui/icon/assets/material/`, and XDG/system icon lookup in `crates/core/ui/icon/src/lib.rs`.
  - Installation design storage: `~/.config/mesh/plugins.lock.json`, `~/.cache/mesh/packages/`, `/usr/share/mesh/plugins/`, and `~/.local/share/mesh/plugins/` documented in `docs/installation.md`.

**Caching:**
- In-memory runtime caches/registries.
  - Event channels use in-memory Tokio broadcast senders in `crates/core/foundation/events/src/lib.rs`.
  - Interface/provider registry is in-memory in `crates/core/extension/service/src/lib.rs`.
  - Icon registry uses a process-global `OnceLock<Mutex<IconRegistry>>` in `crates/core/ui/icon/src/lib.rs`.
  - Installer/package cache path `~/.cache/mesh/packages/` is documented in `docs/installation.md` but no implemented downloader/cache manager is detected.

## Authentication & Identity

**Auth Provider:**
- Not detected.
  - Implementation: No OAuth, OIDC, session, JWT, password, or external identity provider integration detected in source manifests or Rust dependencies.

**Capability Model:**
- Custom capability-based module permission system.
  - Implementation: Capability types in `crates/core/foundation/capability/Cargo.toml`; backend exec checks in `crates/core/runtime/scripting/src/backend.rs`; manifest capabilities in `modules/backend/pipewire-audio/package.json`, `modules/backend/pulseaudio-audio/package.json`, and `modules/frontend/navigation-bar/package.json`.
  - Permission examples: `exec.wpctl`, `exec.pactl`, `exec.aplay`, `service.audio.read`, `service.audio.control`, `theme.read`, `locale.read`, and `shell.surface`.
  - Install UX design: Standard/elevated/high capability tiers documented in `docs/extensibility.md` and `spec/pluggable-backend.md`.

**Package Identity:**
- npm-style package names identify modules.
  - Implementation: Top-level `name` in `package.json` manifests, with MESH fields under `mesh`, as documented in `docs/module-system.md`.
  - Examples: `@mesh/local-config` in `config/package.json`; `@mesh/pipewire-audio` in `modules/backend/pipewire-audio/package.json`; `@mesh/panel` in `config/modules/@mesh/panel/package.json`.

## Monitoring & Observability

**Error Tracking:**
- No external error tracking service detected.

**Logs:**
- `tracing` and `tracing-subscriber` provide application logging.
  - CLI setup: `crates/tools/cli/src/main.rs` initializes `tracing_subscriber::fmt()` with `EnvFilter::try_from_default_env()`.
  - LSP setup: `crates/tools/lsp/src/main.rs` defaults `RUST_LOG` to `mesh_tools_lsp=info`.
  - Backend scripts: `mesh.log`, `mesh.log.info`, `mesh.log.warn`, `mesh.log.error`, and `mesh.log.debug` are injected by `crates/core/runtime/scripting/src/backend.rs`.

**Diagnostics/Health:**
- In-process diagnostics collector and debug overlay.
  - Diagnostics: `crates/core/foundation/diagnostics/src/lib.rs` tracks health, degraded/error state, handler errors, missing icons, and backend lifecycle errors.
  - Debug snapshot: `crates/core/shell/src/shell/mod.rs` builds plugin, interface, backend-runtime, health, and surface snapshots.
  - Health design: `docs/health.md` defines plugin health states, missing dependency records, runtime reports, periodic re-checks, and health channels.

## CI/CD & Deployment

**Hosting:**
- Not detected.
  - This is a local Linux desktop shell binary and module tree, not a hosted web service.

**CI Pipeline:**
- Not detected.
  - No `.github/workflows`, `.gitlab-ci.yml`, CircleCI, or Buildkite files found.

**Distribution/Install Design:**
- System and user module directories.
  - System paths: `/usr/share/mesh/modules` in `crates/core/foundation/config/src/lib.rs`; legacy `/usr/share/mesh/plugins/` remains documented in `docs/installation.md`.
  - User paths: `~/.mesh/modules`; legacy install-design paths such as `~/.local/share/mesh/plugins/`, `~/.local/share/mesh/dev-plugins/`, and `~/.config/mesh/plugins.lock.json` remain documented in `docs/installation.md`.
  - Current binary: `mesh-shell` from `crates/tools/cli/Cargo.toml`.

## Environment Configuration

**Required env vars:**
- None strictly required for local repo defaults.

**Optional env vars:**
- `MESH_HOME` - Override root module/config home in `crates/core/extension/plugin/src/package.rs` and `crates/core/foundation/config/src/lib.rs`.
- `MESH_SETTINGS_PATH` - Override user settings JSON in `crates/core/foundation/config/src/lib.rs`.
- `MESH_SETTINGS_DEFAULTS_PATH` - Override default settings JSON in `crates/core/foundation/config/src/lib.rs`.
- `MESH_IPC_SOCKET` - Override Unix IPC socket path in `crates/core/shell/src/shell/mod.rs`.
- `XDG_CONFIG_HOME` - Override XDG config root for `mesh/config.toml` and legacy module override files in `crates/core/foundation/config/src/lib.rs`.
- `XDG_DATA_HOME` - Override XDG data root helper in `crates/core/foundation/config/src/lib.rs`.
- `XDG_RUNTIME_DIR` - Preferred runtime directory for `mesh.sock` in `crates/core/shell/src/shell/mod.rs`.
- `HOME` - Fallback base for `~/.mesh`, XDG paths, and package manifest discovery.
- `UID` - Fallback Unix socket filename when `XDG_RUNTIME_DIR` is unavailable.
- `RUST_LOG` - Runtime log filtering for CLI and LSP.
- `LD_LIBRARY_PATH` - Set by the Nix dev shell in `flake.nix` for native Wayland libraries.

**Secrets location:**
- Not detected.
  - `.env` files not found during scan.
  - No credential, secret, private key, or package auth config files were read.

## Webhooks & Callbacks

**Incoming:**
- No HTTP webhook endpoints detected.
- Local incoming Unix socket commands only, implemented in `crates/core/shell/src/shell/ipc.rs`.

**Outgoing:**
- No HTTP webhook clients detected.
- Current outgoing integrations are local process executions from backend Luau scripts through `mesh.exec` in `crates/core/runtime/scripting/src/backend.rs`.
- Planned outgoing package sources include registry, HTTPS archive, Git, and local path in `docs/installation.md`; no implemented HTTP/Git fetch client detected.

---

*Integration audit: 2026-05-06*
