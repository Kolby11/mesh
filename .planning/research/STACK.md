# Stack Research: v1.8 Rendering Engine Architecture

## Question

What stack additions or replacements should MESH evaluate for the next renderer architecture?

## Findings

### Blitz

Blitz is the closest ecosystem match to the user's direction: a modular Rust HTML/CSS renderer from DioxusLabs. Its public README describes it as a radically modular HTML/CSS rendering engine and identifies a stack built around Stylo for CSS, Taffy for layout, Parley for text, and a renderer backend abstraction that has moved toward AnyRender/Vello-style rendering.

**Fit for MESH:** strong as a reference architecture and prototype target. Risky as a direct base until MESH proves shell-specific lifecycle, retained invalidation, diagnostics, accesskit integration, and Wayland surface behavior can map cleanly.

### Skia / rust-skia

Skia is a mature 2D graphics library used in Chrome, Android, Flutter, and other products. The `rust-skia` project provides `skia-safe` bindings with GPU backend support for Vulkan, Metal, OpenGL, and Direct3D.

**Fit for MESH:** good candidate for low-level raster/GPU rendering if MESH keeps ownership of layout, style, retained scene data, profiling, and shell-specific invalidation. Main risks are build complexity, binary size, GPU context management, and matching existing damage/profiling semantics.

### Stylo

Stylo is Servo/Firefox's browser-grade CSS style engine. It is powerful, but browser-grade style engines carry substantial integration and dependency complexity.

**Fit for MESH:** evaluate through Blitz first. Direct Stylo adoption should be gated behind a proof that it can serve MESH's constrained shell CSS without pulling in full browser-engine obligations.

### Taffy

Taffy is a Rust layout engine for DOM-free UI layout with Flexbox and Grid algorithms. It maps well to retained UI trees and custom renderers.

**Fit for MESH:** strong candidate for layout replacement or coexistence. It should be evaluated independently even if Blitz is not adopted.

### Parley

Parley is Linebender's text layout library. It supports layout, cursor/selection geometry, and editor-oriented text flows.

**Fit for MESH:** strong candidate for richer text, selection, and eventual text input work. It should be benchmarked against current cosmic-text/swash paths and existing selection behavior.

### AnyRender

AnyRender is DioxusLabs' rendering abstraction extracted from Blitz/Dioxus native work.

**Fit for MESH:** useful to evaluate if Blitz's abstraction boundary can target Skia, Vello, or MESH's retained command model. Do not adopt until the abstraction preserves MESH profiling and damage metadata.

### Winit

Winit is a low-level cross-platform window and event-loop library. Its docs stress that it creates windows and delivers input/window events; drawing is supplied by another library.

**Fit for MESH:** useful as a reference or possible shell/windowing layer only if it can coexist with MESH's Wayland-native shell requirements. It is not a renderer.

### AccessKit

AccessKit is a cross-platform accessibility toolkit for UI/toolkit providers.

**Fit for MESH:** strong candidate. Any renderer migration should preserve or improve MESH accessibility tree production and expose accessibility deltas through retained node identity.

### Muda

Muda provides native menu utilities for desktop apps. It supports Windows, macOS, and Linux/GTK with platform-specific integration constraints.

**Fit for MESH:** likely out of renderer-critical path. Evaluate only if v1.8 expands from rendering into desktop shell menu integration.

### html5ever / xml5ever

Servo's html5ever is a browser-grade HTML5 parser. xml5ever provides XML/XHTML-style parsing in the same ecosystem.

**Fit for MESH:** useful if MESH intentionally grows HTML/XHTML input or imports Blitz DOM paths. Not necessary for `.mesh` component rendering unless the milestone decides to support HTML-like authoring.

## Recommendation

Use v1.8 to make a measured architecture decision:

1. Prototype Blitz on one MESH-equivalent shipped surface.
2. In parallel, prototype a MESH-owned retained pipeline slice using Taffy + Parley + Skia or AnyRender.
3. Compare determinism, profiling, invalidation, accessibility, bundle/build cost, and migration complexity.
4. Choose one path before broad migration.

## Sources

- Blitz: https://github.com/DioxusLabs/blitz
- Dioxus native/Blitz context: https://dioxuslabs.com/
- Skia docs: https://skia.org/docs/
- rust-skia: https://github.com/rust-skia/rust-skia
- Taffy docs: https://taffylayout.com/docs
- Parley docs: https://docs.rs/parley/latest/parley/
- Stylo: https://github.com/servo/stylo
- Winit docs: https://rust-windowing.github.io/winit/winit/
- AccessKit: https://www.xskit.dev/
- Muda: https://github.com/tauri-apps/muda
- Servo html5ever: https://github.com/servo/html5ever
