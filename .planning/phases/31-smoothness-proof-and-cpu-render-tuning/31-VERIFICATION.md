---
phase: 31
title: Smoothness proof and CPU render tuning verification
status: gaps_found
verified: 2026-05-13
requirements: ["PERF-03", "SMTH-01", "SMTH-02", "SMTH-03"]
---

# Phase 31 Verification

## Status

`gaps_found`

Automated CPU-render proof and conservative threshold tuning are complete. Live manual UAT was run after the original verification pass: `hover` and `keyboard_traversal` passed, while `surface_open_close`, `pointer_update`, and `backend_update` exposed major interaction/state gaps. Those gaps are diagnosed in `31-UAT.md` and have a gap-closure plan in `31-02-PLAN.md`.

## Commands

- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render icon`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render text_cache`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase26_real_surface_baseline_emits_canonical_proof_measurements -- --nocapture`

## Evidence

- The canonical shipped-surface proof test now emits `PHASE31_PROOF` rows for `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, and `backend_update`.
- Repaint policy threshold selection now uses a two-thirds surface-area full repaint threshold while preserving the three-quarters changed-entry tree rebuild fallback.
- Focused shell policy tests cover zero damage, small single damage, below-threshold extra damage, two-thirds full-surface promotion, and tree rebuild entry thresholds.
- Existing retained display-list tests continue to prove filtered survivor ordering, root/background replay, scrollbar span inclusion, and full-surface fallback metrics.
- Cache capacities were intentionally left unchanged: raster capacity remains `256`, text layout capacity remains `128`.
- Existing icon and text cache tests continue to cover freshness, SVG external-resource bypass, tint/multicolor key separation, non-UTF path identity, opaque/translucent hit reporting, text layout hits, and layout-affecting misses.

## Scope Boundary

No GPU backend, parallel paint/layout implementation, new benchmark harness, trace persistence, or broad shell UI redesign was added.

## Residual Risk

- `31-UAT.md` has three major live UAT issues pending gap-closure execution and retest: popover close/focus/transition behavior, slider value synchronization, and audio mute/backend state reconciliation.
- `31-01-BENCHMARK.md` still marks acceptance decisions `deferred`; automated counters alone are not accepted as visible smoothness proof.
- The current shipped-surface proof rows still report `full_surface` policy because the canonical scenarios reach full-rebuild paths. Smaller retained damage paths are protected by focused policy tests, but not yet proven as visible end-user smoothness wins.

## Follow-Up

- Run `$gsd-execute-phase 31 --gaps-only` to execute `31-02-PLAN.md`, then rerun `$gsd-verify-work 31` for the three failed live UAT rows.
- If visual lag remains after this conservative CPU threshold work, continue with the planned Skia/GPU renderer investigation or later parallel paint/layout work after retained ownership boundaries are proven.
