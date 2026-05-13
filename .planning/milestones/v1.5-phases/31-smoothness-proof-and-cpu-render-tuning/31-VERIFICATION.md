---
phase: 31
title: Smoothness proof and CPU render tuning verification
status: complete
verified: 2026-05-13
requirements: ["PERF-03", "SMTH-01", "SMTH-02", "SMTH-03"]
---

# Phase 31 Verification

## Status

`complete`

Automated CPU-render proof and conservative threshold tuning are complete. Live manual UAT passes `hover`, `pointer_update`, `keyboard_traversal`, and `backend_update`; `surface_open_close` works with a slight remaining delay explicitly deferred by the user for later polish.

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
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell navigation_bar_same_hover_volume_trigger_closes_popover_immediately`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell audio_popover_mute_renders_shell_normalized_state`

## Evidence

- The canonical shipped-surface proof test now emits `PHASE31_PROOF` rows for `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, and `backend_update`.
- Repaint policy threshold selection now uses a two-thirds surface-area full repaint threshold while preserving the three-quarters changed-entry tree rebuild fallback.
- Focused shell policy tests cover zero damage, small single damage, below-threshold extra damage, two-thirds full-surface promotion, and tree rebuild entry thresholds.
- Existing retained display-list tests continue to prove filtered survivor ordering, root/background replay, scrollbar span inclusion, and full-surface fallback metrics.
- Cache capacities were intentionally left unchanged: raster capacity remains `256`, text layout capacity remains `128`.
- Existing icon and text cache tests continue to cover freshness, SVG external-resource bypass, tint/multicolor key separation, non-UTF path identity, opaque/translucent hit reporting, text layout hits, and layout-affecting misses.
- 31-03 keeps pointer-open audio popover activation from stealing focus while preserving keyboard-open focus transfer into the popover.
- 31-03 uses idempotent `set_muted` when available and keeps an optimistic shell-level pending mute state across stale active-backend and inactive-provider updates until the requested state is confirmed.
- `31-REVIEW.md` records the post-implementation review and the stale-provider pending mute guard fixed in `6e0dc0a`.
- Live retest after 31-03 confirmed the immediate slider grab path now passes.
- Live retest after 31-03 still found that same-hover trigger close needs pointer leave/re-enter and that the mute mismatch persists.
- 31-04 makes the same-hover trigger close publish an immediate `HideSurface` request and covers it with a navigation-bar regression.
- 31-04 removes the popover-local mute pending model so popover display follows shell-normalized `mesh.audio.muted`, with shell optimistic stale-update coverage still passing.
- Live retest after 31-04 confirmed backend mute consistency now passes.
- Live retest after 31-04 confirmed surface open/close works; a slight visual delay remains and is deferred to future transition/smoothness polish by user request.

## Scope Boundary

No GPU backend, parallel paint/layout implementation, new benchmark harness, trace persistence, or broad shell UI redesign was added.

## Residual Risk

- `31-UAT.md` records one deferred minor polish item: slight audio popover open/close delay after functional close behavior was fixed.
- `31-01-BENCHMARK.md` still marks acceptance decisions `deferred`; automated counters alone are not accepted as visible smoothness proof.
- The current shipped-surface proof rows still report `full_surface` policy because the canonical scenarios reach full-rebuild paths. Smaller retained damage paths are protected by focused policy tests, but not yet proven as visible end-user smoothness wins.

## Gap Closure Plan

- COMPLETE: `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-03-PLAN.md`
- COMPLETE: `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-04-PLAN.md`
- Scope: same-hover popover trigger close and single-source mute state across popover/navigation UI.
- Final live UAT is complete with user-approved deferral for popover transition delay polish.

## Follow-Up

- Track deferred popover transition delay polish in `.planning/todos/pending/2026-05-13-phase31-audio-popover-transition-delay.md`.
- If visual lag remains after this conservative CPU threshold work, continue with the planned Skia/GPU renderer investigation or later parallel paint/layout work after retained ownership boundaries are proven.
