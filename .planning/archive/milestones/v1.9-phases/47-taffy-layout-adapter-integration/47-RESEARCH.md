# Phase 47: Taffy Layout Adapter Integration - Research

## RESEARCH COMPLETE

## Research Question

What needs to be known to plan Phase 47 so Taffy replaces the relevant MESH layout code while preserving retained identity, shipped navigation/audio geometry, and diagnostic visibility?

## Sources Read

- `.planning/phases/47-taffy-layout-adapter-integration/47-CONTEXT.md`
- `.planning/REQUIREMENTS.md`
- `.planning/ROADMAP.md`
- `.planning/STATE.md`
- `.planning/phases/46-renderer-library-dependency-and-adapter-foundation/46-CONTEXT.md`
- `Cargo.toml`
- `crates/core/ui/elements/Cargo.toml`
- `crates/core/frontend/render/Cargo.toml`
- `crates/core/ui/elements/src/layout.rs`
- `crates/core/ui/elements/src/tree.rs`
- `crates/core/ui/elements/src/style/types.rs`
- `crates/core/shell/src/shell/component/rendering.rs`
- `crates/core/frontend/render/src/render_object.rs`
- `docs/renderer-migration.md`
- `docs/renderer-ownership.md`
- Official Taffy 0.10.1 docs on docs.rs: `https://docs.rs/taffy/0.10.1/taffy/`
- Local Taffy 0.10.1 crate examples from Cargo registry: `examples/measure.rs` and `examples/custom_tree_vec.rs`

## Findings

### Taffy Integration Shape

- Taffy 0.10.1 supports CSS-style Flexbox, Grid, and Block layout and exposes `TaffyTree` as a high-level API. Its docs state that high-level use builds a Taffy-owned tree, runs `compute_layout` or `compute_layout_with_measure`, then reads each node's `Layout`.
- Taffy also exposes lower-level tree traits and examples for embedding layout into an existing UI framework. The docs recommend lower-level APIs for systems with their own node/widget tree representation.
- For Phase 47, the safest executable path is to preserve MESH's public `LayoutEngine` API and replace its internals with a Taffy-backed builder/writer. That lets shell rendering, profiling, retained runtime-tree code, and render-object synchronization keep their current call sites while Taffy owns geometry computation.

### Dependency Ownership

- Phase 46 added `taffy = { workspace = true, optional = true }` under `crates/core/frontend/render/Cargo.toml` and a `renderer-taffy` render feature.
- The actual layout engine lives in `crates/core/ui/elements/src/layout.rs`, inside the `mesh-core-elements` crate. That crate currently has no Taffy dependency.
- Strict replacement means Phase 47 should add Taffy to `mesh-core-elements` directly. If Taffy remains only optional in `mesh-core-render`, the code that owns layout cannot use it without moving layout ownership into the renderer crate, which would be the wrong boundary.

### Current Layout Semantics To Preserve Or Deliberately Diagnose

The current custom engine supports:

- `Display::None` zero-layout exclusion.
- Row/column flex-like layout via `ComputedStyle.direction`.
- Fixed, percent, auto, and content dimensions.
- Min/max width and height clamping.
- Padding and margin.
- `gap`.
- `flex_grow`, `flex_shrink`, and `flex_basis`.
- `justify_content`, `align_items`, and `align_self`.
- Text leaf measurement through an injected `TextMeasurer`.
- Intrinsic measurement caching using stable MESH `NodeId` plus subtree signatures.
- Overflow-aware natural main-axis size for clipped overflow containers.
- RTL row mirroring.
- Absolute positioning with inset edges and stretch between opposing insets.

The phase requirements explicitly call out rows, columns, stacks, fixed sizes, gaps, padding, absolute positioning, and container-width cases. Tests should cover those first, then preserve existing layout unit cases where they remain part of public MESH semantics.

### Retained Identity And Dirty Propagation

- MESH `WidgetNode` keeps stable `NodeId`, computed style, layout, children, accessibility metadata, event handlers, and interaction state.
- Shell rendering calls `LayoutEngine::compute_with_intrinsic_cache_and_measurer` from `crates/core/shell/src/shell/component/rendering.rs`, then render-object synchronization detects geometry changes in `crates/core/frontend/render/src/render_object.rs`.
- Taffy must not own or replace MESH `NodeId`. A Taffy tree may use transient Taffy node handles internally, but the adapter must retain a `NodeId -> Taffy node` or traversal map during computation and write final rectangles back into the existing `WidgetNode.layout` fields.

### Diagnostics And LAYT-03 Reconciliation

- The user explicitly selected strict replacement: unsupported cases are gaps to diagnose and close, not hidden old-engine fallbacks.
- Current `LayoutEngine` APIs return `()`, so diagnostics cannot naturally propagate unless the implementation adds an internal result-bearing helper or stores diagnostics in a returned structure from a new method. To keep call sites stable, planning should add a Taffy diagnostics type and a helper such as `compute_taffy_with_diagnostics`, while the existing `compute*` methods can call it and trace non-fatal diagnostics.
- LAYT-03 should be satisfied by visible unsupported-case diagnostics and tests that prove no silent old-layout fallback is present.

## Recommended Plan Decomposition

1. **Dependency and ownership relocation:** Move Taffy into `mesh-core-elements`, add Taffy diagnostics/adapter scaffolding, and update renderer ownership docs so layout promotion is explicit.
2. **Taffy-backed layout replacement:** Replace `LayoutEngine` internals with Taffy mapping and writeback while preserving public entrypoints, text measurement, retained identity, and profiling call sites.
3. **Parity, shipped-surface, and documentation gates:** Add layout unit parity/regression tests, shipped navigation/audio geometry tests, and docs updates for strict replacement and unsupported-case diagnostics.

## Validation Architecture

Automated validation should use the existing Rust test harness:

- Quick layout unit command: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-elements layout`
- Render/shell integration command: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase47`
- Regression commands: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase44_navigation` and `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render proof`
- Compile gates: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo check -p mesh-core-elements` and `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo check -p mesh-core-shell`

The final verification must cover LAYT-01 through LAYT-03 and every context decision D-01 through D-13.

