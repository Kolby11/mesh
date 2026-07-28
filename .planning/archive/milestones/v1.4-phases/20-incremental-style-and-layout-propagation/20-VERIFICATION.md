---
status: passed
phase: 20
---

# Verification: Phase 20

Status: passed

## Verified

- `INCR-01`: Retained interaction restyles target previous/current stateful stable nodes instead of applying state-rule propagation across the full tree.
- `INCR-02`: Interaction layout invalidation distinguishes non-layout style changes from layout-affecting style changes by comparing layout-relevant style signatures.
- `INCR-03`: Retained layout rectangles are reused when targeted restyles leave layout-relevant inputs unchanged; full layout remains the fallback when geometry inputs change.
- `INCR-04`: Interaction-state retained routing and keyed restyle behavior are covered by focused tests.

## Commands

- `cargo fmt --check`
- `cargo test -p mesh-core-elements targeted_restyle_recomputes_only_named_stateful_nodes -- --nocapture`
- `nix develop -c cargo test -p mesh-core-shell typed_invalidations -- --nocapture`
