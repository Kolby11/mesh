---
phase: 49
plan: "01"
subsystem: mesh-core-render
tags: [anyrender, paint-adapter, proof-evidence, feature-gate]
dependency_graph:
  requires: []
  provides: [anyrender-paint-proof-adapter, focused-paint-anyrender-evidence]
  affects: [mesh-core-render]
tech_stack:
  added: [color 0.3, peniko 0.6]
  patterns: [feature-gated adapter module, proof-only display-list encoding, software-painter rollback]
key_files:
  created:
    - crates/core/frontend/render/src/anyrender_adapter.rs
  modified:
    - Cargo.lock
    - crates/core/frontend/render/Cargo.toml
    - crates/core/frontend/render/src/lib.rs
    - crates/core/frontend/render/src/proof.rs
decisions:
  - "Use peniko::kurbo for AnyRender-compatible geometry types instead of adding a direct kurbo dependency."
  - "Keep Slider/Input/Scrollbars as a documented deferred subset returning 0 encoded ops."
  - "Text emits a non-fatal combined parley+anyrender diagnostic unless both feature paths are active."
  - "FocusedPaintEvidence.anyrender_encoded is proof evidence only; software painter remains authoritative."
metrics:
  duration: "~20 minutes"
  completed: "2026-05-21"
  tasks: 3
  files: 5
---

# Phase 49 Plan 01: AnyRender Paint Proof Adapter Summary

Implemented the proof-posture AnyRender adapter behind `renderer-anyrender`. The adapter encodes retained display-list background, border, and icon commands into `anyrender::recording::Scene` and exposes the result through `FocusedPaintEvidence.anyrender_encoded`.

## Files

| File | Change | Lines |
|------|--------|-------|
| `crates/core/frontend/render/src/anyrender_adapter.rs` | Created feature-gated proof adapter and tests | 299 |
| `crates/core/frontend/render/src/proof.rs` | Added `anyrender_encoded` and feature-gated adapter call | 715 |
| `crates/core/frontend/render/src/lib.rs` | Registered `anyrender_adapter` module | 37 |
| `crates/core/frontend/render/Cargo.toml` | Added direct optional `color` and `peniko` deps to `renderer-anyrender` | 41 |
| `Cargo.lock` | Recorded direct render crate deps | n/a |

## Verification

| Command | Result |
|---------|--------|
| `cargo check -p mesh-core-render` | Passed, existing `placement_top` warning |
| `cargo test -p mesh-core-render --features renderer-anyrender anyrender` | 7 passed |
| `cargo test -p mesh-core-render proof` | 9 passed |
| `cargo test -p mesh-core-render --features renderer-anyrender proof` | 9 passed |
| `cargo test -p mesh-core-render --features renderer-anyrender` | 81 passed |
| `cargo test -p mesh-core-shell phase44_navigation` | 2 passed |

## Deviations

- The cargo tree still contains two `kurbo` versions: `resvg` uses `kurbo 0.11.3`, while AnyRender uses `kurbo 0.13.1`. Instead of forcing workspace convergence and risking the existing SVG stack, the adapter imports geometry types through `peniko::kurbo`, which is the `kurbo 0.13.1` line used by AnyRender.
- `RoundedRect::new(rect, radii)` from the plan was adjusted to `RoundedRect::from_rect(rect, radii)` for `kurbo 0.13.1`.
- `Edges::default()` from the plan test helper was adjusted to `Edges::zero()` because `Edges` does not implement `Default`.

## Requirements

- **PAINT-01:** Satisfied for the documented Phase 49 subset: backgrounds, borders, and icons encode to an AnyRender scene; Slider/Input/Scrollbars are documented deferred subset paths and return 0.
- **PAINT-02:** Satisfied for background/border/icon proof evidence through `FocusedPaintEvidence.anyrender_encoded`.
- **PAINT-03:** Satisfied. Default builds compile with `anyrender_encoded == false`; `surface/painter.rs` was not modified.

Text glyph-run encoding remains deferred to a combined Parley + AnyRender path.
