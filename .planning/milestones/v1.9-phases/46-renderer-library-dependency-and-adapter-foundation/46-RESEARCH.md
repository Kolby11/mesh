# Phase 46: Renderer Library Dependency And Adapter Foundation - Research

## User Constraints

- D-01: Add selected renderer libraries as production Cargo manifest entries behind conservative adoption boundaries.
- D-02: Taffy, Parley, and AccessKit are primary foundation dependencies; AnyRender/Vello-style paint remains experimental until Phase 49.
- D-03: Verify current crate metadata and choose Rust 1.85-compatible, non-yanked, production-appropriate versions. If latest requires a higher Rust version, keep it disabled/deferred and document why.
- D-05: Current renderer behavior remains the default for build and runtime.
- D-06: Use explicit Cargo features for dependency fan-out control.
- D-07: Feature flags must be paired with adapter-level rollback once behavior exists.
- D-08: Keep Phase 46 code in or below `crates/core/frontend/render` unless only root workspace metadata is being updated.
- D-10: Define adapter seam and dependency gates only; real Taffy layout, Parley shaping, paint backend execution, and AccessKit runtime publication belong to Phases 47-50.
- D-11 through D-13: Update dependency/build/Nix/CI risk documentation and verify both disabled and enabled feature paths.

## Current Crate Metadata

Checked with `cargo info` on 2026-05-18.

| Crate | Candidate version | Rust requirement | Notes for Phase 46 |
|-------|-------------------|------------------|--------------------|
| `taffy` | `0.10.1` | 1.71 | Prefer this stable version over latest `0.11.0-experimental-cache-fix.3` for the first production scaffold. Enable via optional feature. |
| `parley` | `0.7.0` | 1.83 | Latest `0.9.0` and `0.8.0` require Rust 1.88, above workspace Rust 1.85. Use `0.7.0` with `default-features = false`, `features = ["std"]`. |
| `accesskit` | `0.24.0` | 1.85 | Matches workspace Rust floor. Add optional and disabled by default. |
| `anyrender` | `0.10.0` | unknown | Optional experimental paint candidate. No default features. Must be checked under enabled feature path. |
| `vello_encoding` | `0.5.1` | 1.85 | Use as Vello-style display-list encoding boundary without pulling full `vello`/`wgpu`. Latest `0.9.0` requires Rust 1.88. |
| `vello` | `0.5.1` or `0.9.0` | 1.85 / 1.88 | Full `vello` pulls `wgpu` by default; latest requires Rust 1.88. Defer full Vello backend to Phase 49. |
| `kurbo` | `0.11.3` | 1.65 | Transitive/compatible with current AnyRender/Linebender stack; no direct Phase 46 dependency needed unless compiler asks for explicit type use. |
| `peniko` | `0.6.1` | 1.85 | Likely transitive through AnyRender/Vello paths; no direct Phase 46 dependency needed unless adapter type aliases require it. |

## Architecture Patterns

- `mesh-core-render` is the correct dependency owner for renderer-specific candidate crates. Its README explicitly keeps render-specific code in `crates/core/frontend/render` unless the change belongs to compiler, component parsing, element contracts, or presentation.
- Root `Cargo.toml` already centralizes workspace dependencies. Add candidate crate versions there, then consume them as optional dependencies from `crates/core/frontend/render/Cargo.toml`.
- The current renderer authority is retained widget nodes -> render objects -> retained display list -> software painter -> `PixelBuffer` -> presentation. Phase 46 must not make a candidate crate authoritative.
- `FocusedProofSnapshot` in `crates/core/frontend/render/src/proof.rs` is the current adapter-owned evidence seam. Phase 46 can add a sibling internal module for feature status and adapter selection without altering proof semantics.
- Existing docs already contain the migration dependency record template and promotion gates. Update those docs instead of creating a new adoption process.

## Recommended Feature Shape

Use disabled-by-default features in `mesh-core-render`:

- `renderer-taffy = ["dep:taffy"]`
- `renderer-parley = ["dep:parley"]`
- `renderer-accesskit = ["dep:accesskit"]`
- `renderer-anyrender = ["dep:anyrender"]`
- `renderer-vello-encoding = ["dep:vello_encoding"]`
- `renderer-libraries = ["renderer-taffy", "renderer-parley", "renderer-accesskit", "renderer-anyrender", "renderer-vello-encoding"]`

This intentionally uses `renderer-vello-encoding`, not full `renderer-vello`, because full Vello backend adoption belongs to Phase 49 and latest Vello requires Rust 1.88.

## Don't Hand-Roll

- Do not add fake crate names or proof strings in place of Cargo dependencies. Optional deps must compile under the enabled feature gate.
- Do not introduce runtime settings, user-facing config, or `.mesh` author syntax for these libraries in Phase 46.
- Do not change `mesh-core-presentation` or Wayland backend ownership.
- Do not switch layout/text/paint behavior to candidate crates in Phase 46.
- Do not broaden Skia or Blitz work in this phase.

## Common Pitfalls

- Pinning latest Parley or Vello would violate the workspace Rust 1.85 floor because `parley@0.9.0`, `parley@0.8.0`, `vello@0.9.0`, and `vello_encoding@0.9.0` require Rust 1.88.
- Adding optional dependencies but never compiling their feature path can leave broken dependency combinations undetected.
- Calling Cargo features a rollback path is insufficient for future behavior changes. Phase 46 should create or document the adapter bypass point that later phases must use.
- Full `vello` introduces `wgpu` fan-out and native/GPU dependency risk; Phase 46 should keep full Vello backend adoption deferred.
- Full workspace tests can be expensive. Use targeted checks per task and run workspace checks where feasible; record environment blockers explicitly.

## Validation Architecture

Phase 46 should validate three layers:

1. Manifest and feature scaffold:
   - `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo check -p mesh-core-render`
   - `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo check -p mesh-core-render --features renderer-libraries`
   - `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo tree -p mesh-core-render --features renderer-libraries`
2. Adapter seam and default behavior:
   - `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render renderer_library`
   - `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render proof`
3. Shipped-surface and migration gates:
   - `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase44`
   - `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase44_navigation`
   - `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test --workspace` where feasible.

Documentation proof should grep for the concrete dependency record and feature names:

`rg -n "renderer-taffy|renderer-parley|renderer-accesskit|renderer-anyrender|renderer-vello-encoding|taffy =|parley =|accesskit =|anyrender =|vello_encoding =|Rust 1.88|rollback path" Cargo.toml crates/core/frontend/render/Cargo.toml docs/renderer-migration.md docs/renderer-ownership.md`
