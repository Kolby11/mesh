# Technology Stack

**Analysis Date:** 2026-05-01

## Languages

**Primary:**
- Rust (edition 2024, MSRV 1.85) — all core runtime, rendering, shell orchestration, and tooling
- Luau (via `mlua`) — plugin scripting for both frontend components (`<script>` blocks in `.mesh` files) and backend service plugins (`src/main.luau`)

**Secondary:**
- JSON — plugin manifests (`plugin.json`), theme tokens (`config/themes/*.json`), user settings (`config/settings.json`, `config/shell-settings.json`)
- TOML — workspace `Cargo.toml`, interface contracts (`interface.toml`), icon pack mappings (`mapping.toml`), alternative manifest format (`mesh.toml`)
- XHTML-like markup — `.mesh` `<template>` blocks (parsed by `mesh-core-component`)
- CSS-like styling — `.mesh` `<style>` blocks (parsed with `lightningcss`/`cssparser`)

## Runtime

**Environment:**
- Linux (Wayland-only — no X11 support)
- Requires `wlr-layer-shell-v1` compositor protocol

**Package Manager:**
- Cargo (workspace resolver "2")
- Lockfile: `Cargo.lock` present and committed

## Frameworks

**Core:**
- Tokio 1 (full features) — async runtime for the shell event loop, backend plugin polling, IPC server
- mlua 0.11 (luau + serialize + send features) — embeds Luau VM for both frontend and backend scripts; `crates/core/runtime/scripting/`

**Wayland / Rendering:**
- smithay-client-toolkit 0.19 (xkbcommon feature) — Wayland layer-shell client, keyboard handling
- wayland-client 0.31 — low-level Wayland protocol bindings
- minifb 0.27 (wayland feature) — dev-window fallback surface for testing without a full compositor
- resvg 0.44 — SVG rasterization (re-exports `usvg` and `tiny_skia`); `crates/core/ui/render/`
- image 0.24 — PNG/JPEG icon decoding; `crates/core/ui/render/`
- cosmic-text 0.18 — text layout and shaping; `crates/core/ui/render/`
- fontdb 0.23 — font database for cosmic-text

**Parsing:**
- quick-xml 0.39 — XML/XHTML template parser; `crates/core/ui/component/`
- cssparser 0.35 — CSS tokenization; `crates/core/ui/component/`
- lightningcss 1.0.0-alpha.71 — CSS property parsing; `crates/core/ui/component/`

**Serialization:**
- serde 1 (derive) + serde_json 1 — all JSON manifest, settings, service payload handling
- toml 0.8 — TOML manifest and config parsing
- semver 1 — plugin version compatibility checks

**Error Handling / Logging:**
- thiserror 2 — typed error enums across all crates
- tracing 0.1 + tracing-subscriber 0.3 (env-filter) — structured logging; initialized in `mesh-tools-cli`

**Testing:**
- Rust built-in test harness (`#[cfg(test)]` blocks in each crate)
- tempfile 3 — temporary directories in icon resolution tests (`crates/core/ui/icon/`)

**Build/Dev:**
- Nix flake (`flake.nix`) — reproducible dev shell with `cargo`, `rustc`, `rustfmt`, `clippy`, `pkg-config`, `libxkbcommon`, `wayland`, `wayland-protocols`
- rustix 0.38 (event, fs, mm, shm) — low-level Linux syscalls for shared memory buffer management

## Key Dependencies

**Critical:**
- `mlua` (luau mode) — the entire extension runtime depends on this; replacing it would require rewriting all backend and frontend scripting
- `smithay-client-toolkit` — provides layer-shell Wayland surface integration; no non-Wayland fallback in production
- `cosmic-text` — text rendering engine; all text element painting goes through this
- `resvg` — SVG icon rendering; required for XDG icon theme support (SVG scalable tier)

**Infrastructure:**
- `tokio` — all async I/O, backend polling timers, IPC server, channel wiring
- `serde` / `serde_json` — service payloads flow as `serde_json::Value` throughout the stack

## Configuration

**Environment:**
- `MESH_IPC_SOCKET` — overrides the Unix socket path for IPC (default: `$XDG_RUNTIME_DIR/mesh-shell.sock`)
- `RUST_LOG` — controls tracing filter (e.g. `RUST_LOG=mesh_core_shell=debug`)
- `LD_LIBRARY_PATH` — must include `libxkbcommon`, `wayland` (set automatically by Nix dev shell)

**Build:**
- `Cargo.toml` (workspace root) — version, edition, MSRV, and shared dependency versions
- `flake.nix` — Nix dev environment with all runtime libraries

## Platform Requirements

**Development:**
- Linux with Wayland compositor (or minifb dev-window for rendering tests)
- Rust 1.85+
- `pkg-config`, `libxkbcommon-dev`, `wayland` libraries
- Nix flake provides a complete dev shell via `nix develop`

**Production:**
- Linux only (Wayland-native)
- Compositor must support `wlr-layer-shell-v1`
- PipeWire or PulseAudio for audio backend plugins
- NetworkManager for network backend plugin
- UPower for power backend plugin

---

*Stack analysis: 2026-05-01*
