---
phase: 104
slug: retained-taffytree
status: gaps_found
verified: 2026-06-18
---

# Phase 104 Verification

## Status

`gaps_found`

Retained TaffyTree implementation is present and the focused retained-layout proof passes. Full shell test verification is blocked by the current dirty worktree: shipped navigation/module tests fail against local navigation-bar and service/module changes that are already present outside this phase.

## Passed Checks

| Check | Result |
|-------|--------|
| `cargo test --package mesh-core-elements -- retained_layout_parity` | passed, 5/5 |
| `cargo test --package mesh-core-elements -- layout` | passed, 32/32 |
| `nix develop -c cargo build --package mesh-core-shell` | passed |

## Failed / Blocked Checks

| Check | Result |
|-------|--------|
| `cargo build --package mesh-core-shell` outside Nix | blocked: missing system `xkbcommon.pc` |
| `nix develop -c cargo test --package mesh-core-shell` | failed: 278 passed, 54 failed |

## Gap Details

- The full shell suite compiles under Nix, but runtime assertions fail in existing real-surface, service, navigation, and module graph tests.
- A focused shipped-surface layout test still expects old navigation-bar structure/content while the dirty worktree has rewritten `modules/frontend/navigation-bar/src/main.mesh` from `status-cluster`/`control-cluster` to `left-cluster`/`right-cluster` plus new clock/brightness/quick-settings/theme-selector surfaces.
- These failures are not suitable to fix inside Phase 104 without overwriting or redesigning user-visible module work that predates this retained-layout patch.

## Requirement Coverage

| Requirement | Coverage |
|-------------|----------|
| LAYOUT-01 retained `TaffyTree` state | covered |
| LAYOUT-02 STYLE/LAYOUT dirty routing | covered by `compute_incremental` tests |
| LAYOUT-03 `_mesh_key` structural identity | covered by add/remove/reorder parity tests |
| LAYOUT-04 post-order subtree removal | covered |
| LAYOUT-05 retained vs fresh parity | covered for five planned scenarios |

## Recommended Next Step

Run a gap-closure/debug pass for the dirty navigation/module shell failures before marking Phase 104 fully verified.

