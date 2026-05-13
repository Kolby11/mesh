---
phase: 31
title: Smoothness proof and CPU render tuning verification
status: human_needed
verified: 2026-05-13
requirements: ["PERF-03", "SMTH-01", "SMTH-02", "SMTH-03"]
---

# Phase 31 Verification

## Status

`human_needed`

Automated CPU-render proof, conservative threshold tuning, and 31-02/31-03 gap-closure implementation are complete. Live manual UAT passed `hover` and `keyboard_traversal`; tests 2, 3, and 5 have fresh automated fixes and are reset for live retest before final acceptance.

## Commands

- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render icon`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render text_cache`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase26_real_surface_baseline_emits_canonical_proof_measurements -- --nocapture`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell audio_popover`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell navigation_bar`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell surface`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell slider`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell real_surfaces`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell activate_popover`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell set_muted_command_broadcasts_optimistic_audio_state_until_backend_confirms`

## Evidence

- The canonical shipped-surface proof test now emits `PHASE31_PROOF` rows for `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, and `backend_update`.
- Repaint policy threshold selection now uses a two-thirds surface-area full repaint threshold while preserving the three-quarters changed-entry tree rebuild fallback.
- Focused shell policy tests cover zero damage, small single damage, below-threshold extra damage, two-thirds full-surface promotion, and tree rebuild entry thresholds.
- Existing retained display-list tests continue to prove filtered survivor ordering, root/background replay, scrollbar span inclusion, and full-surface fallback metrics.
- Cache capacities were intentionally left unchanged: raster capacity remains `256`, text layout capacity remains `128`.
- Existing icon and text cache tests continue to cover freshness, SVG external-resource bypass, tint/multicolor key separation, non-UTF path identity, opaque/translucent hit reporting, text layout hits, and layout-affecting misses.
- 31-03 keeps pointer-open audio popover activation from stealing focus while preserving keyboard-open focus transfer into the popover.
- 31-03 uses idempotent `set_muted` when available and keeps an optimistic shell-level pending mute state across stale backend updates until the requested state is confirmed.

## Scope Boundary

No GPU backend, parallel paint/layout implementation, new benchmark harness, trace persistence, or broad shell UI redesign was added.

## Residual Risk

- `31-UAT.md` has three rows pending live retest after 31-03: first navigation click after popover open, first slider drag after popover open, and second mute-toggle reconciliation in the navigation bar.
- `31-01-BENCHMARK.md` still marks acceptance decisions `deferred`; automated counters alone are not accepted as visible smoothness proof.
- The current shipped-surface proof rows still report `full_surface` policy because the canonical scenarios reach full-rebuild paths. Smaller retained damage paths are protected by focused policy tests, but not yet proven as visible end-user smoothness wins.

## Gap Closure Plan

- COMPLETE: `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-03-PLAN.md`
- Scope: first input after audio popover activation, first slider grab after show/focus transfer, and idempotent/shared mute pending-state reconciliation.
- Final acceptance still requires live UAT confirmation for tests 2, 3, and 5 after 31-03.

## Follow-Up

- Rerun `$gsd-verify-work 31` for tests 2, 3, and 5.
- If visual lag remains after this conservative CPU threshold work, continue with the planned Skia/GPU renderer investigation or later parallel paint/layout work after retained ownership boundaries are proven.
