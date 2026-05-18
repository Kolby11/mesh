# Phase 42 Source Inventory

## Source Rule

No accept, defer, or reject outcome may be recorded without a primary source or local source reference.

## Local MESH Sources

| Local source | Reference | Claim used by matrix | Risk to preserve |
|--------------|-----------|----------------------|------------------|
| current retained display list | `crates/core/frontend/render/src/display_list.rs` | MESH already has retained paint commands, damage rectangles, reuse metrics, batching barriers, and repaint policy data. | A new paint backend must consume or replace this boundary without forcing whole-surface repaint as the default. |
| retained render objects | `crates/core/frontend/render/src/render_object.rs` | MESH tracks stable render-object slots for transform, clip, opacity, geometry, material, text, and accessibility. | Candidate paths must not discard retained identity or accessibility slot deltas. |
| software painter | `crates/core/frontend/render/src/surface/painter.rs` | Current rendering is isolated behind a software painter boundary. | New renderers should be alternate backends, not replacements for `.mesh`, service, module, or shell runtime ownership. |
| render profiling | `crates/core/frontend/render/src/surface/profiling.rs` | Render cost, retained metrics, and debug payloads are existing observability contracts. | Prototype observability may regress only if final migration restores equivalent invalidation, damage, profiling, diagnostics, and debug payloads. |
| Wayland presentation | `crates/core/presentation/src/lib.rs` and `crates/core/presentation/src/wayland_surface/*` | Production presentation prefers Wayland/layer-shell surfaces and supports damage-aware presentation. | Direct Blitz or Winit shell ownership must prove fit with MESH's shell-surface lifecycle. |
| navigation bar surface | `modules/frontend/navigation-bar/src/main.mesh` | The navigation bar is a shipped surface with hover, click, theme, icon, and audio-trigger behavior. | Phase 43 prototypes must compare this surface rather than a synthetic-only demo. |
| audio popover surface | `modules/frontend/audio-popover/src/main.mesh` | The audio popover exercises open-close behavior, slider interaction, and service-shaped UI state. | Phase 43 prototypes must include slider and popover behavior, not just static rendering. |
| Skia spike manifest | `.planning/spikes/MANIFEST.md` | An isolated Skia CPU-raster retained-display-list painter spike is already marked VALIDATED. | The spike proves isolated feasibility only; it does not select Skia for production. |

## External Candidate Sources

| Candidate | Primary source | Claim used by matrix | Risk to re-check before dependency adoption |
|-----------|----------------|----------------------|---------------------------------------------|
| Blitz | https://github.com/DioxusLabs/blitz | Blitz is a modular HTML/CSS renderer with DOM, style/layout/event, paint, HTML parsing, and shell integration crates; its README also describes it as pre-alpha. | Re-check maturity, shell ownership, dependency graph, and whether Wayland/layer-shell production integration avoids browser-engine-level overhead. |
| Taffy | https://taffylayout.com/docs | Taffy provides DOM-free CSS-style layout for custom renderers, including Flexbox/Grid-style algorithms and custom measurement. | Re-check current API, text measurement hooks, and whether MESH can map retained nodes without wholesale layout ownership changes. |
| Parley | https://docs.rs/parley/latest/parley/ | Parley provides rich text layout primitives, shaping, line breaking, bidi reordering, alignment, and styled layout builders. | Re-check selection geometry, cache integration, font database ownership, and fit with MESH text invalidation. |
| AnyRender | https://github.com/DioxusLabs/anyrender | AnyRender is a 2D drawing abstraction intended to target multiple backends, including Vello-style and Skia-backed paths. | Re-check API stability, backend support, CPU fallback story, and whether MESH display-list commands map cleanly. |
| Skia | https://skia.org/docs/ | Skia is an open source 2D graphics library used across major hardware and software platforms. | Re-check Linux/Nix dependencies, binary size, CPU/GPU backend choice, and whether capability gain justifies native dependency cost. |
| rust-skia | https://github.com/rust-skia/rust-skia | rust-skia provides Rust bindings for Skia and documents prebuilt binary downloads plus source builds requiring LLVM, Python 3, and Ninja when binaries are unavailable. | Re-check CI cache behavior, bindgen/build time, platform feature matrix, and impact on contributor setup. |
| Stylo | https://github.com/servo/stylo | Stylo is Servo/Firefox's CSS style engine and exposes browser-grade CSS parsing/resolution machinery. | Re-check upstream sync burden, MPL implications, selector/style integration cost, and whether focused style capability offsets browser-engine complexity. |
| Winit | https://docs.rs/winit/ | Winit is a cross-platform window creation and event loop management library. | Re-check whether it belongs only in throwaway harnesses or can coexist with MESH's Wayland/layer-shell presentation model. |
| AccessKit | https://accesskit.dev/ and https://docs.rs/accesskit/latest/accesskit/ | AccessKit provides accessibility infrastructure for self-rendered UI toolkits, with stable node identity and atomic tree updates. | Re-check Unix/AT-SPI adapter maturity, action routing, and retained-node update granularity. |
| Muda | https://docs.rs/muda | Muda is a desktop menu utility library; Linux support is GTK-only and carries GTK/libxdo setup requirements. | Re-check only if Phase 43 needs native menus; otherwise avoid adding GTK dependency surface. |
| html5ever | https://github.com/servo/html5ever | html5ever is Servo's HTML parser and does not provide its own DOM tree representation. | Re-check only if Blitz HTML parsing or imported markup becomes a v1.8 requirement. |
| xml5ever | https://docs.rs/crate/xml5ever/latest | xml5ever is an XML parser in the html5ever family and is relevant to XHTML/XML parsing paths. | Re-check only if XHTML/XML import is required; `.mesh` authoring does not need it in Phase 42. |
