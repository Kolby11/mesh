# Blitz Reference Evidence

## Path Summary

The Blitz reference path uses the shared Phase 43 fixture and records an HTML/CSS-equivalent structural evidence file at `.planning/prototypes/phase43/output/blitz-reference.json`.

The default harness remains compileable so the focused-crate path can proceed. Direct Blitz pixel rendering is blocked in this environment by a reproducible compile failure in the current crates.io Blitz alpha.

## Attempted harness

Attempted command:

```bash
cargo check --manifest-path .planning/prototypes/phase43/Cargo.toml --features blitz-reference
```

The `blitz-reference` feature enables:

```toml
blitz = { version = "0.3.0-alpha.4", default-features = false, optional = true }
```

The compileable fallback command remains:

```bash
cargo check --manifest-path .planning/prototypes/phase43/Cargo.toml
cargo run --manifest-path .planning/prototypes/phase43/Cargo.toml --bin blitz_reference
```

## Crate/API boundary

The attempted boundary is the high-level `blitz` crate from crates.io, version `0.3.0-alpha.4`, with default features disabled. That crate still pulls the modular Blitz stack, including `blitz-dom`, `blitz-html`, `blitz-paint`, `blitz-shell`, Stylo, Taffy, Parley, AnyRender/Vello, WGPU, Winit, AccessKit, html5ever, and xml5ever.

The fallback binary records equivalent HTML/CSS scenario evidence from the shared fixture but does not call Blitz APIs after the optional feature compile fails.

## Visual/Layout Fidelity

Structured evidence was produced for all five required scenarios:

- `nav-baseline`
- `nav-audio-trigger-hover`
- `audio-popover-visible`
- `audio-slider-change-release`
- `audio-popover-close`

Pixel output was not produced because the Blitz feature could not compile.

## Interaction Shape

The evidence records all required interaction IDs:

- `hover-volume-trigger`
- `click-volume-trigger`
- `change-audio-slider-0.42-to-0.73`
- `release-audio-slider-0.73`
- `close-audio-popover`

These are fixture-level interaction records, not live Blitz event dispatch.

## Accessibility Boundary

The fallback output maps retained fixture nodes to `blitz-accesskit-node-*` IDs so the comparison can still evaluate whether the Blitz path has an accessibility boundary shape. This is not proof that Blitz shell accessibility dispatch works in MESH.

## Build/Dependency Cost

Enabling the `blitz-reference` feature locked 318 additional packages during the attempted check, including the Blitz modular stack, WGPU/Vello, Winit, Wayland/X11, Stylo, html5ever/xml5ever, and a second AnyRender line. This supports the Phase 42 concern that direct Blitz adoption carries browser-engine-level build and dependency cost.

## Observed error or mismatch

Observed compile error:

```text
error[E0425]: cannot find value `event_loop` in this scope
   --> blitz-0.3.0-alpha.4/src/lib.rs:147:17
    |
147 |         let _ = event_loop;
    |                 ^^^^^^^^^^ not found in this scope
```

This blocks direct render evidence through the high-level Blitz crate in this throwaway harness. The blocker is in the dependency crate, not in MESH prototype code.

## Reproduction

Run:

```bash
cargo check --manifest-path .planning/prototypes/phase43/Cargo.toml --features blitz-reference
```

Expected reproduction result: compile failure in `blitz-0.3.0-alpha.4` with `error[E0425]: cannot find value event_loop in this scope`.

Run the fallback evidence generator:

```bash
cargo run --manifest-path .planning/prototypes/phase43/Cargo.toml --bin blitz_reference
```

Expected fallback result: `.planning/prototypes/phase43/output/blitz-reference.json` is regenerated and contains all five scenario IDs.

## PROTO-01 Result

PROTO-01: blocker evidence produced

