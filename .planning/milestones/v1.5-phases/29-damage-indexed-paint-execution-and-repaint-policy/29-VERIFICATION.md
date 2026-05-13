---
status: passed
phase: 29
verified: 2026-05-12
plans_verified: 2
must_haves_verified: 12
gaps_open: 0
human_verification: not_required
---

# Phase 29: Damage-Indexed Paint Execution and Repaint Policy Verification Report

## Goal Achievement

Phase 29 passes. Partial-damage paints can now use retained command-span metadata to select ordered filtered command input, repaint policy and fallback counters are published through the existing debug payload, and the debug inspector makes those counters visible enough to answer whether retained paint filtering is actually reducing work.

## Observable Truths

- Retained display-list command spans are keyed by retained subtree ownership and preserve final paint-command order.
- Sparse damage can select fewer retained commands than the full command list while preserving survivor order and scrollbar inclusion.
- Broad or ambiguous retained paint state selects explicit `full_surface` fallback and increments fallback accounting.
- Tooltip overlay work remains separate from retained display-list traversal.
- Debug payloads expose repaint policy, filtered spans, filtered commands, skipped commands, and fallback count under `invalidation.paint`.
- The shipped debug-inspector Surfaces view now renders the retained paint filtering counters directly.
- Partial or version-skewed paint payloads render unavailable labels instead of misleading zero-count filtering data.
- Canonical benchmark proof remains in the existing benchmark path and keeps all five scenario IDs: `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, `backend_update`.

## UAT

Phase 29 UAT completed with two passed checkpoints, one skipped manual-measurement checkpoint, and one reported observability issue. The issue was diagnosed and fixed in Plan 29-02.

## Code Review

Code review completed with `status: clean` in `29-REVIEW.md` after the review warnings were fixed and re-reviewed.

## Verification Commands

```text
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render painter_
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell debug_inspector
```

All listed commands passed.

## Requirements Coverage

- `PIPE-03`: Covered by retained command-span selection, sparse-damage filtering tests, and visible filtered-command counters.
- `PIPE-04`: Covered by ordered filtered command selection, tooltip separation, and debug payload proof.
- `CULL-03`: Covered by repaint-policy selection, full-surface fallback accounting, and debug-inspector visibility.

## Gaps Summary

No open gaps.
