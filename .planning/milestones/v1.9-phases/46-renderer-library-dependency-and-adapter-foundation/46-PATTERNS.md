# Phase 46: Renderer Library Dependency And Adapter Foundation - Patterns

## Files To Modify

| File | Role | Closest Existing Analog | Notes |
|------|------|-------------------------|-------|
| `Cargo.toml` | Workspace dependency source of truth | Existing `[workspace.dependencies]` entries for `serde`, `tokio`, `bitflags`, `slotmap` | Add candidate renderer libraries centrally so feature-gated crate deps stay consistent. |
| `crates/core/frontend/render/Cargo.toml` | Render crate dependency and feature gate owner | Current `cosmic-text`, `fontdb`, `image`, `resvg`, `skia-safe`, `swash`, `zeno` deps | Add disabled-by-default features and optional deps here; do not modify presentation/shell manifests. |
| `crates/core/frontend/render/src/library_adapters.rs` | Internal adapter status/bypass seam | `crates/core/frontend/render/src/proof.rs` | Keep migration evidence internal and testable without changing runtime behavior. |
| `crates/core/frontend/render/src/lib.rs` | Render crate module/export boundary | Existing `pub mod proof;` and `pub use proof::{...}` | Export only internal status helpers needed by later phases/tests. |
| `docs/renderer-migration.md` | Dependency record and promotion gates | Existing Dependency Record Template and Broad Adoption Checklist | Fill Phase 46 dependency record with concrete crate versions, Rust constraints, Nix impact, CI gates, rollback path. |
| `docs/renderer-ownership.md` | Ownership status map | Existing Adapter-Owned Boundaries and Replacement Candidates tables | Mark Phase 46 dependency/adapters as disabled-by-default scaffolding, not author-facing behavior. |

## Established Implementation Patterns

- Render-specific code belongs in `mesh-core-render`, not shell or presentation, unless a later phase proves a boundary needs to move.
- Existing proof evidence in `proof.rs` is adapter-owned and internal. New adapter status code should follow that internal, testable style.
- `Cargo.toml` workspace deps use bare version strings or structured dependency records. Optional feature ownership belongs in the consuming crate manifest.
- Tests are colocated in the Rust module that owns the behavior. A `renderer_library` test filter in `mesh-core-render` is the right focused verification target for the new feature/status seam.
- Docs use concrete grep-verifiable strings. Add exact feature names and crate versions.

## Risk Notes For Planning

- `parley@0.9.0`, `parley@0.8.0`, `vello@0.9.0`, and `vello_encoding@0.9.0` require Rust 1.88. The workspace is Rust 1.85, so plans should pin compatible versions or explicitly defer newer ones.
- Full `vello` pulls `wgpu`; Phase 46 should avoid making that a default/native dependency.
- Optional deps must be tested with `--features renderer-libraries`; otherwise Cargo can hide broken feature combinations.
- Docs need to state no `.mesh` authoring behavior changes in Phase 46.
