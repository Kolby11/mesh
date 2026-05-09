---
status: passed
phase: 22
verified: 2026-05-09
---

# Phase 22 Verification: Retained Display List and Damage Tracking

## Result

Status: `passed`

## Requirement Coverage

- `PAINT-01`: Passed. A retained display-list layer is tracked across frames in `mesh-core-render` and updated from current widget/render-object output.
- `PAINT-02`: Passed. Damage tracking computes changed rectangles from previous and next retained display-list bounds for insertion, removal, layout, paint, text, transform-like, clip-like, and full-fallback cases.
- `PAINT-03`: Passed conservatively. The system computes damage opportunities and reports partial-present support separately; current unsupported backends keep full-buffer behavior and report zero skipped paint pixels.
- `PAINT-04`: Passed. Debug profiling surface snapshots expose retained display-list reuse, damage area, full-surface fallback, partial-present support, and skipped paint metrics under `invalidation.paint`.

## Commands

- `cargo fmt --check`
- `cargo test -p mesh-core-render display_list`
- `cargo test -p mesh-core-render`
- `nix develop -c cargo test -p mesh-core-shell profiling_snapshot_exposes_typed_surface_invalidation_counts`
- `nix develop -c cargo test -p mesh-core-shell profiling`

## Residual Risk

The current presentation stack still presents full buffers. Phase 22 intentionally records `partial_present_supported=false` until a backend-specific partial present path exists.
