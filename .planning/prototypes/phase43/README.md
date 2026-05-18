# Phase 43 Renderer Prototype Harness

## Scope

This throwaway harness compares a Blitz reference path with a MESH-owned focused-crate path against the same shipped-surface slice: navigation bar plus audio popover. It records structural layout, retained node identity, paint command shape, interaction events, and accessibility boundary evidence.

## Commands

Run from the repository root:

```bash
cargo check --manifest-path .planning/prototypes/phase43/Cargo.toml
cargo run --manifest-path .planning/prototypes/phase43/Cargo.toml --bin blitz_reference
cargo run --manifest-path .planning/prototypes/phase43/Cargo.toml --bin focused_crate
```

## Scenarios

The shared fixture is `.planning/prototypes/phase43/fixtures/phase43-scenarios.json`.

- `nav-baseline`
- `nav-audio-trigger-hover`
- `audio-popover-visible`
- `audio-slider-change-release`
- `audio-popover-close`

## Evidence Outputs

- `.planning/prototypes/phase43/output/blitz-reference.json`
- `.planning/prototypes/phase43/output/focused-crate.json`
- `.planning/prototypes/phase43/evidence/blitz-reference.md`
- `.planning/prototypes/phase43/evidence/focused-crate.md`

## Final Artifacts

- `.planning/phases/43-comparable-renderer-prototype-proofs/43-PROTOTYPE-COMPARISON.md`
- `.planning/phases/43-comparable-renderer-prototype-proofs/43-PHASE44-HANDOFF.md`

## Non-Goals

- No production renderer replacement.
- No production Wayland/layer-shell adoption.
- No general `.mesh` to Blitz translator.
- No real backend audio runtime.
- No broad migration plan.
