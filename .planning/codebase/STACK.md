# Technology Stack

**Analysis Date:** 2026-05-06

## Languages

**Primary:**
- Rust 1.85 minimum, edition 2024 - Core shell, plugin runtime, UI compiler/renderer, CLI, LSP, diagnostics, configuration, and service registry in `Cargo.toml`, `crates/core/**/Cargo.toml`, and `crates/tools/**/Cargo.toml`.
- Luau via `mlua` 0.11.6 - Backend service scripts and frontend component scripts hosted by `mesh-core-scripting`; backend examples live in `modules/backend/pipewire-audio/src/main.luau` and `modules/backend/pulseaudio-audio/src/main.luau`.

**Secondary:**
- MESH single-file UI components (`.mesh`) - Frontend surface/component authoring with `<template>`, `<script lang="luau">`, and `<style>` blocks; shipped surface at `modules/frontend/navigation-bar/src/main.mesh`.
- JSON - Module manifests, root module graph, settings, themes, and package metadata in `config/package.json`, `config/modules/@mesh/*/package.json`, `config/settings-default.json`, `config/shell-settings.json`, and `config/themes/*.json`.
- TOML - Rust manifests, interface contracts, legacy/config formats, icon registry config, and docs examples in `Cargo.toml`, `modules/interfaces/audio.toml`, `config/icons.toml`, and `docs/module-system.md`.
- Nix - Development shell in `flake.nix`.

## Runtime

**Environment:**
- Rust binary runtime on Linux/Wayland; the main shell command is `mesh-shell` from `crates/tools/cli/Cargo.toml`.
- Async runtime: Tokio 1.52.1 with `full` features, used by backend orchestration, event bus channels, LSP server, and Unix IPC in `crates/core/runtime/backend/src/lib.rs`, `crates/core/foundation/events/src/lib.rs`, `crates/tools/lsp/src/main.rs`, and `crates/core/shell/src/shell/ipc.rs`.
- Luau VM: `mlua` 0.11.6 with `luau`, `serialize`, and `send` features in `crates/core/runtime/scripting/Cargo.toml`.
- Wayland client runtime: `wayland-client` 0.31.14, `smithay-client-toolkit` 0.19.2, `minifb` 0.27.0 with `wayland`, and runtime libraries from `flake.nix`.

**Package Manager:**
- Cargo - Rust workspace and dependency manager.
- Lockfile: `Cargo.lock` present.
- npm-compatible `package.json` is used for MESH module metadata, not as an active Node.js package manager workflow. No root `package.json`, `package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, or `bun.lockb` detected.

## Frameworks

**Core:**
- Cargo workspace - Multi-crate workspace defined in `Cargo.toml`.
- Tokio 1.52.1 - Async runtime, timers, MPSC channels, broadcast channels, IPC accept loop, and backend task orchestration in `crates/core/runtime/backend/src/lib.rs` and `crates/core/shell/src/shell/ipc.rs`.
- mlua 0.11.6 - Embedded Luau host APIs for backend and frontend scripts in `crates/core/runtime/scripting/src/backend.rs` and `crates/core/runtime/scripting/src/context.rs`.
- tower-lsp 0.20.0 - Language server framework for `.mesh` authoring in `crates/tools/lsp/Cargo.toml`.
- minifb 0.27.0 + Wayland - Presentation/window pump selected by `mesh-core-render` in `crates/core/ui/render/Cargo.toml`.

**Testing:**
- Rust built-in test harness - Unit and integration-style tests colocated in crate source files and test modules across `crates/core/**/src/*.rs`.
- tempfile 3.27.0 - Temporary filesystem fixtures in crates such as `crates/core/shell/Cargo.toml`, `crates/core/extension/service/Cargo.toml`, `crates/core/ui/render/Cargo.toml`, and `crates/core/ui/icon/Cargo.toml`.

**Build/Dev:**
- rustfmt and clippy - Provided in the Nix dev shell through `flake.nix`.
- Nix flakes - Optional reproducible dev environment in `flake.nix`, including `cargo`, `rustc`, `rustfmt`, `clippy`, `pkg-config`, `libxkbcommon`, `wayland`, and `wayland-protocols`.
- Cargo metadata - Workspace member and binary metadata comes from `Cargo.toml` and `crates/**/Cargo.toml`.

## Key Dependencies

**Critical:**
- `serde` 1.0.228 / `serde_json` 1.0.149 - Manifest parsing, JSON settings, service payloads, diagnostics, and Luau value serialization in `crates/core/extension/plugin/src/package.rs`, `crates/core/extension/plugin/src/manifest.rs`, `crates/core/runtime/backend/src/lib.rs`, and `crates/core/runtime/scripting/src/backend.rs`.
- `toml` 0.8.23 - TOML config, legacy manifests, and interface contracts in `crates/core/foundation/config/Cargo.toml`, `crates/core/extension/plugin/Cargo.toml`, and `crates/core/extension/service/Cargo.toml`.
- `semver` 1.0.28 - Interface version and version requirement parsing in `crates/core/extension/service/Cargo.toml`.
- `thiserror` 2.0.18 - Typed errors throughout core crates, including `crates/core/extension/plugin/src/package.rs`, `crates/core/foundation/config/src/lib.rs`, and `crates/core/foundation/events/src/lib.rs`.
- `tracing` 0.1.44 / `tracing-subscriber` 0.3.23 - Runtime logging and env-filter setup in `crates/tools/cli/src/main.rs`, `crates/tools/lsp/src/main.rs`, and `crates/core/foundation/diagnostics/Cargo.toml`.
- `mlua` 0.11.6 - Luau backend execution, host API injection, service payload conversion, and script command dispatch in `crates/core/runtime/scripting/src/backend.rs`.

**Infrastructure:**
- `quick-xml` 0.39.2 - `.mesh` component markup parsing in `crates/core/ui/component/Cargo.toml`.
- `cssparser` 0.35.0 and `lightningcss` 1.0.0-alpha.71 - CSS-like style parsing and validation for `.mesh` components in `crates/core/ui/component/Cargo.toml`.
- `cosmic-text` 0.18.2 and `fontdb` 0.23.0 - Text shaping and font discovery in `crates/core/ui/render/Cargo.toml`.
- `image` 0.24.9 and `resvg` 0.44.0 - Raster and SVG handling in `crates/core/ui/render/Cargo.toml`.
- `rustix` 0.38.44 - Low-level event, filesystem, memory mapping, and shared-memory support in `crates/core/ui/render/Cargo.toml`.
- `wayland-client` 0.31.14 and `smithay-client-toolkit` 0.19.2 - Wayland client protocol integration in `crates/core/ui/render/Cargo.toml`.
- `icon` 0.2.0 and `dirs` 4.0.0 - Icon registry and XDG-style lookup support in `crates/core/ui/icon/Cargo.toml` and `crates/core/ui/icon/src/lib.rs`.
- `tower-lsp` 0.20.0 - `.mesh` language server transport and protocol support in `crates/tools/lsp/Cargo.toml`.

## Configuration

**Environment:**
- `MESH_HOME` - Overrides the module root used for root `package.json`, modules, themes, and settings in `crates/core/extension/plugin/src/package.rs` and `crates/core/foundation/config/src/lib.rs`.
- `MESH_SETTINGS_PATH` - Overrides the user shell settings path in `crates/core/foundation/config/src/lib.rs`.
- `MESH_SETTINGS_DEFAULTS_PATH` - Overrides bundled default settings path in `crates/core/foundation/config/src/lib.rs`.
- `MESH_IPC_SOCKET` - Overrides the shell Unix IPC socket path in `crates/core/shell/src/shell/mod.rs`.
- `XDG_CONFIG_HOME` / `XDG_DATA_HOME` - Control config/data roots for shell config and legacy module override files in `crates/core/foundation/config/src/lib.rs`.
- `XDG_RUNTIME_DIR` - Preferred runtime directory for `mesh.sock` in `crates/core/shell/src/shell/mod.rs`.
- `HOME` / `UID` - Fallbacks for `~/.mesh`, XDG defaults, and `/tmp/mesh-<uid>.sock` in `crates/core/extension/plugin/src/package.rs`, `crates/core/foundation/config/src/lib.rs`, and `crates/core/shell/src/shell/mod.rs`.
- `RUST_LOG` - Standard tracing env-filter input used by `tracing_subscriber::EnvFilter` in `crates/tools/cli/src/main.rs` and `crates/tools/lsp/src/main.rs`.
- `.env` files: Not detected.

**Build:**
- `Cargo.toml` - Workspace members, shared package metadata, Rust version, edition, and shared dependency versions.
- `Cargo.lock` - Resolved Rust dependency graph.
- `crates/**/Cargo.toml` - Per-crate dependencies, library/bin targets, and dev dependencies.
- `flake.nix` - Optional Nix development shell and native Wayland libraries.
- Rust toolchain pin file: Not detected beyond `rust-version = "1.85"` in `Cargo.toml` and `crates/tools/lsp/Cargo.toml`.
- Node/TypeScript config: Not detected.

**Application Config:**
- `config/package.json` - Local installed-module graph. Use `mesh.schemaVersion`, `mesh.modulesDir`, `mesh.modules`, `mesh.providers`, and `mesh.layout` for active modules and provider pins.
- `config/modules/@mesh/*/package.json` - Package-shaped module manifests for bundled/default module metadata and distribution examples.
- `modules/**/package.json` and `modules/**/package.json` - Runtime module manifests loaded by the shell; new modules should use `package.json`, while the current compatibility loader still accepts legacy `package.json`.
- `config/settings-default.json` - Bundled defaults for theme and i18n.
- `config/shell-settings.json` - Repo-local shell settings loaded before falling back to `~/.mesh/settings.json`.
- `config/icons.toml` - Icon pack/profile fallback order for semantic icon names.
- `config/themes/*.json` - Token themes loaded by the theme engine.

## Module System

**Package Model:**
- Use npm-compatible `package.json` for every new module; all MESH-specific fields live under the top-level `mesh` object as documented in `docs/module-system.md`.
- Use top-level `name`, `version`, `description`, `private`, `license`, and `repository`; do not place MESH-only fields such as kind, capabilities, providers, entrypoints, themes, settings, or binary requirements at the top level.
- Root module graph lives in `config/package.json` as `@mesh/local-config`, with `mesh.modulesDir = "../modules"` and active modules under `mesh.modules`.
- The runtime loader accepts `package.json`, `package.json`, and `plugin.json` in `crates/core/extension/plugin/src/manifest.rs` and `crates/core/extension/plugin/src/package.rs`; write new module docs and examples against `package.json`.

**Module Kinds:**
- `interface` - Contract package declaring interface name/version/file, methods, events, types, and capability metadata; example contract file at `modules/interfaces/audio.toml`.
- `backend` - Adapter for a real system source into an interface; examples include `modules/backend/pipewire-audio/package.json`, `modules/backend/pipewire-audio/src/main.luau`, and `modules/backend/pulseaudio-audio/src/main.luau`.
- `frontend` - `.mesh` UI entrypoints and settings UI; examples include `config/modules/@mesh/panel/package.json`, `config/modules/@mesh/quick-settings/package.json`, and `modules/frontend/navigation-bar/src/main.mesh`.
- `theme`, `icon-pack`, `font-pack`, `language-pack`, and `library` - Contribution packages documented in `docs/module-system.md` and represented by `config/modules/@mesh/shell-theme/package.json`.

**Service/Interface Runtime:**
- `mesh-core-service` hosts registry, contract loading, provider resolution, and interface catalog types in `crates/core/extension/service/src/lib.rs`; service interfaces are declared by packages, not hardcoded Rust traits.
- `mesh-core-backend` owns backend task polling and command dispatch in `crates/core/runtime/backend/src/lib.rs`.
- `mesh-core-scripting` exposes generic host APIs such as `mesh.exec`, `mesh.config`, `mesh.service.emit`, `mesh.service.payload`, and `mesh.service.has_capability` in `crates/core/runtime/scripting/src/backend.rs`.
- Frontends consume interface proxies and service state from the runtime rather than importing backend module IDs directly, as documented in `docs/extensibility.md` and `docs/module-system.md`.

## Platform Requirements

**Development:**
- Rust 1.85+ with Cargo, rustfmt, and clippy per `Cargo.toml` and `flake.nix`.
- Linux environment with Wayland development libraries; `flake.nix` provides `libxkbcommon`, `wayland`, and `wayland-protocols`.
- `pkg-config` for native dependency discovery in the Nix dev shell.
- Optional Nix flakes support for reproducible dev shell setup.

**Production:**
- Linux desktop session running an existing Wayland compositor; MESH is a shell client, not a compositor or window manager, per `README.md`.
- Wayland compositor support for layer-shell-style shell surfaces where available; frontend compatibility examples declare `wlr-layer-shell-v1` in `modules/frontend/navigation-bar/package.json` and `docs/installation.md`.
- Runtime module locations include repo `modules/`, `~/.mesh/modules`, and `/usr/share/mesh/modules` from `crates/core/foundation/config/src/lib.rs`.
- User configuration uses `~/.config/mesh/config.toml`, `~/.mesh/settings.json`, and legacy per-module override files under XDG config paths in `crates/core/foundation/config/src/lib.rs`.
- System-installed modules/assets may live under `/usr/share/mesh/modules`; older `/usr/share/mesh/plugins` paths remain documented as legacy installation design in `docs/installation.md`.

---

*Stack analysis: 2026-05-06*
