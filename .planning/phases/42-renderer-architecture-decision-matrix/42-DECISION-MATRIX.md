# Phase 42 Renderer Decision Matrix

## Decision Paths

- Blitz direct adoption
- Blitz-inspired architecture borrowing
- MESH-owned focused-crate path

## Direct Adoption Hard Blockers

| Blocker | Blitz direct adoption status | Evidence source | Decision effect |
|---------|------------------------------|-----------------|-----------------|
| Wayland shell model fit | TBD | `42-SOURCE-INVENTORY.md` local Wayland presentation row; Blitz source row | Must pass before direct adoption can be selected. |
| browser-engine-level performance overhead | TBD | `42-CONTEXT.md` D-04/D-08; Blitz source row | Must pass before direct adoption can be selected. |

## Scorecard Dimensions

| Dimension | What Phase 42 measures | Weight |
|-----------|------------------------|--------|
| determinism | Keeps shell behavior predictable across layout, paint, input, and presentation. | high |
| retained invalidation | Preserves retained node/render-object/display-list invalidation instead of repainting by default. | high |
| profiling | Can restore render cost, damage, and debug payload visibility after prototype work. | medium |
| diagnostics | Supports non-fatal author/runtime diagnostics without hiding errors in renderer internals. | medium |
| accessibility | Provides a clear retained-node accessibility update boundary. | high |
| Wayland shell fit | Fits MESH's Wayland/layer-shell production surface lifecycle. | high |
| build cost | Keeps CI, Nix, local setup, compile time, and native toolchain burden acceptable. | medium |
| binary/dependency risk | Limits binary size, native dependency, licensing, and platform-support risk. | medium |
| migration effort | Can be introduced in reversible phases without replacing the whole renderer at once. | high |
| capability gain | Improves CSS/layout/text/rendering capability enough to justify cost. | high |

## Scoring Scale

- 0 = unacceptable
- 1 = weak
- 2 = acceptable with constraints
- 3 = strong

## Weighted Path Scorecard

| Path | determinism | retained invalidation | profiling | diagnostics | accessibility | Wayland shell fit | build cost | binary/dependency risk | migration effort | capability gain | Phase 43 selection note |
|------|-------------|-----------------------|-----------|-------------|---------------|-------------------|------------|------------------------|------------------|-----------------|-------------------------|
| Blitz direct adoption | TBD | TBD | TBD | TBD | TBD | TBD | TBD | TBD | TBD | TBD | TBD |
| Blitz-inspired architecture borrowing | TBD | TBD | TBD | TBD | TBD | TBD | TBD | TBD | TBD | TBD | TBD |
| MESH-owned focused-crate path | TBD | TBD | TBD | TBD | TBD | TBD | TBD | TBD | TBD | TBD | TBD |

## Candidate Crate Outcomes

| Candidate | v1.8 outcome | Evidence | MESH boundary | Risk/condition |
|-----------|--------------|----------|---------------|----------------|
| Blitz | TBD | TBD | TBD | TBD |
| Skia/rust-skia | TBD | TBD | TBD | TBD |
| Stylo | TBD | TBD | TBD | TBD |
| Taffy | TBD | TBD | TBD | TBD |
| Parley | TBD | TBD | TBD | TBD |
| AnyRender | TBD | TBD | TBD | TBD |
| Winit | TBD | TBD | TBD | TBD |
| AccessKit | TBD | TBD | TBD | TBD |
| Muda | TBD | TBD | TBD | TBD |
| html5ever | TBD | TBD | TBD | TBD |
| xml5ever | TBD | TBD | TBD | TBD |
