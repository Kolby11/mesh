---
spike: 001
name: skia-retained-display-list-painter
type: standard
validates: "Given a standalone Rust harness with MESH-like display-list primitives, when rendered through skia-safe into a CPU raster surface, then it produces deterministic pixels and can be built in this repo environment"
verdict: VALIDATED
related: []
tags: [skia, renderer, spike, cpu-raster]
---

# Spike 001: Skia Retained Display-List Painter

## What This Validates

Given a small retained-display-list-like command stream, when the commands are rendered through `skia-safe` into a CPU raster surface, then Skia can produce a deterministic image artifact in the MESH repo environment without touching production renderer code.

## Research

| Approach | Tool/Library | Pros | Cons | Status |
|----------|--------------|------|------|--------|
| Skia Rust binding | `skia-safe` | Complete 2D painter surface with paths, rounded rects, images, text, CPU and GPU options | Large C++ dependency; build and Nix/CI cost must be proven | Chosen |
| Current stack | custom `PixelBuffer` + `tiny-skia` + `resvg` + `cosmic-text` + `swash` | Already integrated and test-covered | Low-level painter is fragmented; repeated raster and primitive performance remain concerns | Baseline |
| Vello/FemtoVG | Rust GPU/vector renderers | Worth comparing later | Higher architectural change or alpha risk for first proof | Deferred |

**Chosen approach:** start with `skia-safe` CPU raster output. This tests the dependency and primitive mapping with the smallest production blast radius.

## How to Run

```bash
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo run --manifest-path .planning/spikes/001-skia-retained-display-list-painter/Cargo.toml
```

Direct `cargo run` also works on systems that expose Skia's native link dependencies (`freetype` and `fontconfig`) to the linker.

Expected artifact:

```text
.planning/spikes/001-skia-retained-display-list-painter/skia-spike-output.png
```

## What to Expect

The program should render a dark shell-like panel with a blue block, green stroked path, and white text. It should print the command count and center pixel.

## Investigation Trail

- Created a standalone Rust harness so dependency/build failures do not touch `mesh-core-render`.
- The command model intentionally mirrors MESH retained display-list primitives at a coarse level: fill rect, rounded rect, stroke path, and text.
- `skia-safe = "0.97"` downloaded and compiled successfully once the dev shell exposed `freetype` and `fontconfig`.
- A feature attempt with `svg` and `textlayout` hit a missing prebuilt binary archive and fell back to a source build that required `clang`; advanced Skia feature selection needs its own Nix/CI pass before production use.
- The first proof intentionally uses Skia's core CPU raster canvas and `SkFont` text path, not GPU, SVG parsing, paragraph layout, or shell presentation integration.

## Results

Validated.

Command:

```bash
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo run --manifest-path .planning/spikes/001-skia-retained-display-list-painter/Cargo.toml
```

Observed output:

```text
rendered 4 commands to .planning/spikes/001-skia-retained-display-list-painter/skia-spike-output.png; center=#ff242b38
```

Artifact:

```text
.planning/spikes/001-skia-retained-display-list-painter/skia-spike-output.png
```

Conclusion: Rust + Skia is feasible for a MESH-style retained display-list painter. The next production-oriented milestone should benchmark a `mesh-core-render` backend adapter against current shipped-surface scenarios, while treating native dependency setup and feature selection as first-class work.
