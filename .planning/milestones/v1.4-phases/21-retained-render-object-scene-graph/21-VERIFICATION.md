---
status: passed
phase: 21
---

# Verification: Phase 21

Status: passed

## Verified

- `REND-01`: `RenderObjectTree` retains render-facing snapshots keyed by stable widget node IDs.
- `REND-02`: Render-object snapshots separate transform, clip, opacity, geometry, material, text, and accessibility slots.
- `REND-03`: Synchronization reports inserted, removed, reordered, and mutated slot counts.
- `REND-04`: The retained tree remains owned and mutated by the shell paint path, preserving a clear future handoff boundary.

## Commands

- `cargo fmt --check`
- `cargo test -p mesh-core-render render_object_tree_preserves_identity_and_reports_slot_diffs -- --nocapture`
- `nix develop -c cargo test -p mesh-core-shell typed_invalidations -- --nocapture`

