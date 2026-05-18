---
phase: 44
slug: selected-renderer-proof-integration
status: passed
verified: 2026-05-18
---

# Phase 44 Verification

## Verdict

PASS. Phase 44 integrates the selected MESH-owned focused proof path behind the existing renderer and shell boundaries while preserving shipped navigation/audio behavior and completing INTG-01 through INTG-04.

## Goal Backward Check

- Retained identity: `FocusedProofSnapshot` carries `NodeId` and `stable_node_id` through nodes, paint, and accessibility evidence.
- Typed invalidation: proof dirty evidence exposes geometry, material, text, and accessibility fields from `RenderObjectDirtySummary`.
- Damage/profiling: shell paint keeps existing invalidation snapshots, present damage, and profiling behavior while storing focused proof evidence.
- Diagnostics: focused proof diagnostics route through non-fatal degraded diagnostics with `focused renderer proof:` prefix.
- Selection: proof text evidence preserves theme-owned selection colors plus anchor/focus geometry.
- Accessibility: `build_accesskit_update` exposes an AccessKit-compatible update boundary derived from retained node IDs.
- Shipped surfaces: navigation and audio real-surface tests pass with focused snapshots present.

## Commands

- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render proof` - passed
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase44` - passed
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase44_navigation` - passed
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test --workspace` - passed

## Code Review Gate

Inline review of the Phase 44 commits found no blocker issues after the workspace-gate corrections in `cdcfaf7`.

## Notes

The first workspace run exposed deterministic pre-existing shell test-contract failures and one transient icon cache ordering failure. The shell contract issues were fixed in `cdcfaf7`; the final workspace run passed.
