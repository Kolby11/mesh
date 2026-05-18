# Phase 42 - Renderer Architecture Decision Matrix Research

**Date:** 2026-05-18
**Status:** Research complete

## Research Question

Should MESH use Blitz directly, use Blitz as an architectural reference while borrowing selected crates, or build a MESH-owned renderer path from focused crates?

## Current MESH Baseline

MESH already owns the shell runtime, `.mesh` component model, retained widget identity, typed invalidation, retained render objects, retained display data, damage filtering, render profiling, non-fatal diagnostics, Wayland presentation, and shipped navigation/audio surfaces. Phase 42 should not replace those systems. It should produce the decision evidence that Phase 43 will use for isolated prototype work.

Relevant local references:

- `.planning/phases/42-renderer-architecture-decision-matrix/42-CONTEXT.md`
- `.planning/REQUIREMENTS.md`
- `.planning/ROADMAP.md`
- `.planning/research/SUMMARY.md`
- `.planning/spikes/MANIFEST.md`
- `.planning/codebase/ARCHITECTURE.md`
- `.planning/codebase/INTEGRATIONS.md`
- `crates/core/frontend/render/src/display_list.rs`
- `crates/core/frontend/render/src/render_object.rs`
- `crates/core/frontend/render/src/surface/painter.rs`
- `crates/core/frontend/render/src/surface/profiling.rs`
- `crates/core/presentation/src/lib.rs`
- `modules/frontend/navigation-bar/src/main.mesh`
- `modules/frontend/audio-popover/src/main.mesh`

## Source Findings

### Blitz

Blitz is a modular HTML/CSS rendering engine. Its README describes it as pre-alpha and not yet recommended for production apps, but also documents useful boundaries: `blitz-dom` for DOM, style, layout, and events; `blitz-paint` for translating styled/layouted trees into AnyRender commands; `blitz-html` for html5ever/xml5ever parsing; and `blitz-shell` for Winit, AccessKit, and Muda integration.

Source: https://github.com/DioxusLabs/blitz

Planning implication: Blitz should be evaluated as both a direct-adoption candidate and a reference architecture. Direct adoption must clear MESH's hard blockers: Wayland shell model fit and no browser-engine-level performance overhead.

### Taffy

Taffy is a Rust layout engine for custom renderers with DOM-free Flexbox/Grid-style layout, custom measurement, and deterministic UI tree computation.

Source: https://taffylayout.com/docs

Planning implication: Taffy is a likely standalone accept candidate for Phase 43 focused-crate prototypes because it maps to MESH-owned retained nodes without forcing a DOM.

### Parley

Parley provides rich text layout primitives, line breaking/alignment, glyph iteration, style spans, selection/editing utilities, and font handling through the Linebender stack.

Source: https://docs.rs/parley/latest/parley/

Planning implication: Parley is a likely standalone accept candidate for text layout and future selection/editor work, but Phase 42 must still require explicit evidence for retained text cache and selection geometry fit.

### AnyRender and Vello

AnyRender is a Rust 2D drawing abstraction with lightweight type/trait boundaries and backends including Vello, Vello CPU/hybrid, and Skia via `skia-safe`.

Source: https://github.com/DioxusLabs/anyrender

Planning implication: AnyRender is the preferred rendering abstraction to evaluate before Skia fallback because it can preserve MESH-owned retained display-list commands while giving Phase 43 multiple backend choices.

### Skia and rust-skia

Skia is a mature 2D graphics library used by major browser/app platforms. `rust-skia` exposes safe Rust bindings but carries native build and binary/dependency cost; its README calls out prebuilt binary downloads and fallback source builds requiring LLVM, Python, and Ninja.

Sources:

- https://skia.org/docs/
- https://github.com/rust-skia/rust-skia

Planning implication: Skia remains a fallback path. MESH already has an isolated Skia CPU-raster spike marked VALIDATED, but Phase 42 should not choose Skia unless Blitz/AnyRender evidence fails or Skia's capability gain outweighs build cost.

### Stylo

Stylo is Servo/Firefox's browser-grade CSS style engine. Its standalone repository documents many internal crates and sync requirements with Mozilla's upstream code.

Source: https://github.com/servo/stylo

Planning implication: Stylo is worth direct evaluation for style capability, but browser-grade CSS power may carry integration and dependency complexity. Phase 42 should avoid accepting it by association with Blitz alone.

### Winit

Winit is the Rust windowing/input event loop layer. Current docs show 0.30.x as the latest stable release family and 0.31 betas present.

Source: https://docs.rs/crate/winit/latest

Planning implication: Winit is likely useful for throwaway Blitz or native-window harnesses, but MESH's production shell is Wayland/layer-shell oriented. Direct production acceptance needs an explicit lifecycle fit decision.

### AccessKit

AccessKit provides cross-platform accessibility infrastructure for UI toolkits that render their own controls. The core schema has stable node/tree identity and atomic tree updates, with platform adapters including Unix/AT-SPI and a Winit adapter.

Sources:

- https://github.com/AccessKit/accesskit
- https://docs.rs/accesskit

Planning implication: AccessKit is a likely accept candidate because MESH already has retained node identity. Phase 42 should require the decision matrix to identify the retained-node update boundary.

### Muda

Muda is a menu utilities library for desktop applications. Its Linux support is GTK-only and requires GTK for Linux/FreeBSD behavior.

Source: https://docs.rs/crate/muda/latest

Planning implication: Muda should be deferred unless Phase 43 discovers a concrete native menu need. It does not advance the renderer decision for shell surfaces.

### html5ever and xml5ever

html5ever is Servo's HTML5 parser and does not provide its own DOM tree representation. xml5ever is alpha quality and targets error-recovering XML parsing rather than validated XML.

Sources:

- https://github.com/servo/html5ever
- https://docs.rs/crate/xml5ever/latest

Planning implication: Both should be deferred unless the chosen Blitz path needs HTML/XHTML parsing or MESH chooses to support imported markup. `.mesh` authoring does not require them in Phase 42.

## Decision Constraints

- Direct Blitz adoption must fail if Wayland shell fit fails.
- Direct Blitz adoption must fail if it creates browser-engine-level overhead.
- Browser-engine-level overhead includes interaction latency, architecture cost, startup time, compile/build cost, binary size, memory, resource usage, and native dependency burden.
- The same scorecard must compare Blitz direct adoption, Blitz-inspired architecture borrowing, and a MESH-owned focused-crate path.
- Every candidate crate must receive an accept, defer, or reject outcome for v1.8.
- Phase 42 must hand off decision artifacts only. Phase 43 builds throwaway prototypes.

## Recommended Plan Shape

1. Build the source inventory and scorecard schema.
2. Fill candidate crate outcomes and path scores.
3. Produce the final decision package and Phase 43 handoff constraints.

## Validation Architecture

Phase 42 validation is document validation, not runtime validation. The executor should verify:

- `42-DECISION-MATRIX.md` exists.
- The decision matrix contains all three paths: Blitz direct adoption, Blitz-inspired architecture borrowing, and MESH-owned focused-crate path.
- The scorecard contains the required dimensions from REND-02.
- The crate outcome table contains Blitz, Skia/rust-skia, Stylo, Taffy, Parley, AnyRender, Winit, AccessKit, Muda, html5ever, and xml5ever.
- The hard blocker table explicitly names Wayland shell model fit and browser-engine-level performance overhead.
- The Phase 43 handoff keeps both navigation bar and audio popover prototypes in scope and marks them as throwaway harnesses.

## Research Complete

This research is sufficient for Phase 42 planning. Implementation research for Phase 43 should re-check exact crate versions before adding dependencies.
