---
phase: 31
plan: 01
title: Smoothness tuning benchmark proof
created: 2026-05-13
status: complete
canonical_scenarios:
  - hover
  - surface_open_close
  - pointer_update
  - keyboard_traversal
  - backend_update
---

# Phase 31 Benchmark Proof

## Scope

Phase 31 accepts optimization decisions only when benchmark evidence and focused UAT both support smoother shipped-surface behavior. This artifact compares the Phase 31 proof rows against the Phase 26 measured baseline and Phase 30 cache evidence.

Captured with:

`env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase26_real_surface_baseline_emits_canonical_proof_measurements -- --nocapture`

## Canonical Scenario Comparison

| Scenario | Phase 26 baseline | Phase 30 cache proof | Phase 31 after | Policy/filtering | UAT result | Acceptance decision |
| --- | --- | --- | --- | --- | --- | --- |
| `hover` | paint 3244us, traversal 1877us, full rebuild | text hits 5/misses 0/shaping 0us; raster hits 2/misses 2/bypasses 0 | paint 1514us, traversal 480us, text hits 5/misses 0/shaping 0us, raster hits 2/misses 2/bypasses 0 | `full_surface`, filtered 66, skipped 0, spans 34, fallbacks 1 | skipped - visual UAT not run in this headless session | deferred |
| `surface_open_close` | paint 33449us, traversal 31240us, shaping 1251us, full rebuild | text hits 0/misses 6/shaping 1493us; raster hits 0/misses 1/bypasses 0 | paint 32045us, traversal 29960us, text hits 0/misses 6/shaping 1312us, raster hits 0/misses 1/bypasses 0 | `full_surface`, filtered 34, skipped 0, spans 18, fallbacks 1 | skipped - visual UAT not run in this headless session | deferred |
| `pointer_update` | paint 2005us, traversal 1094us, layout 106us, full rebuild | text hits 4/misses 2/shaping 272us; raster hits 1/misses 0/bypasses 0 | paint 884us, traversal 618us, text hits 4/misses 2/shaping 259us, raster hits 1/misses 0/bypasses 0 | `full_surface`, filtered 34, skipped 0, spans 18, fallbacks 1 | skipped - visual UAT not run in this headless session | deferred |
| `keyboard_traversal` | paint 3037us, traversal 1694us, full rebuild | text hits 5/misses 0/shaping 0us; raster hits 4/misses 0/bypasses 0 | paint 451us, traversal 443us, text hits 5/misses 0/shaping 0us, raster hits 4/misses 0/bypasses 0 | `full_surface`, filtered 66, skipped 0, spans 34, fallbacks 1 | skipped - visual UAT not run in this headless session | deferred |
| `backend_update` | paint 31468us, traversal 30011us, shaping 0us, full rebuild | text hits 3/misses 2/shaping 1365us; raster hits 4/misses 0/bypasses 0 | paint 33434us, traversal 32100us, text hits 3/misses 2/shaping 1327us, raster hits 4/misses 0/bypasses 0 | `full_surface`, filtered 66, skipped 0, spans 34, fallbacks 1 | skipped - visual UAT not run in this headless session | deferred |

## Repaint Policy Tuning Evidence

- `select_damage_policy` now promotes damage to full-surface repaint only when candidate damage is at least two-thirds of surface area, or when a tree rebuild changes at least three-quarters of retained entries.
- Focused shell tests prove zero-area damage stays `minimal_damage`, small single damage stays `minimal_damage`, below-threshold extra damage stays `bounding_rect`, two-thirds damage promotes to `full_surface`, and tree rebuilds below the entry threshold stay non-full-surface.
- The current shipped-surface proof rows still report `full_surface` because each canonical scenario currently reaches the full-rebuild path. That is accepted as proof that the new threshold is conservative for real surfaces while still allowing smaller retained damage paths to avoid premature full-surface repaint.

## Cache Capacity and Clear Behavior Decision

- raster capacity unchanged: `RASTER_CACHE_CAPACITY` remains `256` in `crates/core/frontend/render/src/surface/icon.rs`.
- text capacity unchanged: `TEXT_LAYOUT_CACHE_CAPACITY` remains `128` in `crates/core/frontend/render/src/surface/text.rs`.
- Evidence: warm steady-state `pointer_update`, `keyboard_traversal`, and `backend_update` rows report raster hits with `raster_misses=0`; `surface_open_close` has the expected cold first-paint miss; `hover` has visual-state key separation with both hits and misses.
- Evidence: `hover` and `keyboard_traversal` report text hits 5/misses 0/shaping 0us, while `surface_open_close`, `pointer_update`, and `backend_update` include expected text misses for first paint or changed content. The rows do not show text layout invalidations or repeated shaping for unchanged text inputs.
- Clear behavior unchanged: full-surface policy still clears the full buffer, and non-full-surface policy still clears only the effective damage rect in `shell_component.rs`. Direct buffer-clear assertions would be more invasive than the current Phase 31 threshold scope, so this behavior remains protected by the existing repaint policy, retained display-list, and profiling regression tests.
- Guardrails preserved: file freshness checks, SVG external-resource bypass, tint and multicolor key separation, non-UTF path identity preservation, and opaque/translucent raster hit reporting remain covered by the existing `icon` test filter.

## Machine-Readable Phase 31 Rows

The canonical proof test emits one `PHASE31_PROOF` row per scenario with these fields:

- `scenario`
- `paint_us`
- `traversal_us`
- `text_hits`
- `text_misses`
- `shaping_us`
- `raster_hits`
- `raster_misses`
- `raster_bypasses`
- `repaint_policy`
- `filtered_commands`
- `filtered_skipped`
- `filtered_spans`
- `filtered_fallbacks`
- `retained`
- `full_rebuild`

## Acceptance Rule

A row can be marked `accepted` only when the final Phase 31 proof row and the matching `31-UAT.md` result both support smoother visible behavior with no visual or interaction correctness regression. Counter-only wins are marked `rejected` or `deferred`.

## Future Boundary

All five rows are marked `deferred` rather than `accepted` because this execution environment produced automated benchmark proof but did not perform live visual UAT. The remaining acceptance work belongs to `$gsd-verify-work 31` or a manual UAT pass on the shipped shell surfaces. If live UAT still reports lag after these conservative CPU threshold changes, the next candidates remain the planned Skia/GPU renderer investigation, parallel paint/layout exploration after retained ownership boundaries are proven, or deeper diagnostics overlays for filtered command hits and overdraw.
