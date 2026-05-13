---
phase: 30
title: Raster cache hardening verification
status: passed
verified: 2026-05-12
requirements: ["CACHE-01", "CACHE-02", "CACHE-03"]
---

# Phase 30 Verification

## Commands

- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render icon`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render glyph`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render text_cache`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling`
- PASS: benchmark artifact contains `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, and `backend_update`.

## Evidence

- SVG, bitmap, and missing-icon repeat paints report cache hits and no repeat raster time for unchanged visual inputs.
- Tint and multicolor mode are distinct cache keys.
- File freshness changes invalidate file-backed cached variants.
- SVG external resources and untrusted metadata take the conservative bypass path.
- Cached opacity proof reaches display-list barrier decisions for warmed file icons.
- Text measure, render, and selection paths reuse layout entries for unchanged inputs.
- Text changes across text, family, size, weight, line height, width, and alignment miss as expected.
- Shipped-surface profiling reports raster reuse on hover, pointer update, keyboard traversal, and backend update.

## Residual Risk

- Capacity tuning, threshold selection, and visible smoothness acceptance are deliberately deferred to Phase 31.
