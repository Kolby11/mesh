---
phase: 31
plan: 01
title: Smoothness tuning benchmark proof
created: 2026-05-13
status: draft
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
| `hover` | paint 3244us, traversal 1877us, full rebuild | text hits 5/misses 0/shaping 0us; raster hits 2/misses 2/bypasses 0 | pending | pending | pending | pending |
| `surface_open_close` | paint 33449us, traversal 31240us, shaping 1251us, full rebuild | text hits 0/misses 6/shaping 1493us; raster hits 0/misses 1/bypasses 0 | pending | pending | pending | pending |
| `pointer_update` | paint 2005us, traversal 1094us, layout 106us, full rebuild | text hits 4/misses 2/shaping 272us; raster hits 1/misses 0/bypasses 0 | pending | pending | pending | pending |
| `keyboard_traversal` | paint 3037us, traversal 1694us, full rebuild | text hits 5/misses 0/shaping 0us; raster hits 4/misses 0/bypasses 0 | pending | pending | pending | pending |
| `backend_update` | paint 31468us, traversal 30011us, shaping 0us, full rebuild | text hits 3/misses 2/shaping 1365us; raster hits 4/misses 0/bypasses 0 | pending | pending | pending | pending |

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
