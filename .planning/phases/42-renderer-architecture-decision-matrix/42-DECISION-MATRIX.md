# Phase 42 Renderer Decision Matrix

## Decision Paths

- Blitz direct adoption
- Blitz-inspired architecture borrowing
- MESH-owned focused-crate path

## Direct Adoption Hard Blockers

| Blocker | Blitz direct adoption status | Evidence source | Decision effect |
|---------|------------------------------|-----------------|-----------------|
| Wayland shell model fit | unproven blocker risk | `42-SOURCE-INVENTORY.md` local Wayland presentation row; Blitz source row; `crates/core/presentation/src/lib.rs` | Direct adoption is not selected for Phase 43 production path; prototype only if blocker evidence can be gathered. |
| browser-engine-level performance overhead | unproven blocker risk | `42-CONTEXT.md` D-04/D-08; Blitz source row; `crates/core/frontend/render/src/display_list.rs` | Direct adoption is not selected for Phase 43 production path; prototype only if blocker evidence can be gathered. |

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
| Blitz direct adoption | 1 | 1 | 1 | 1 | 2 | 1 | 1 | 1 | 1 | 3 | not selected for Phase 43 production path; prototype only if blocker evidence can be gathered |
| Blitz-inspired architecture borrowing | 3 | 3 | 2 | 2 | 2 | 3 | 2 | 2 | 2 | 3 | selected as reference path for comparable prototype evidence |
| MESH-owned focused-crate path | 3 | 3 | 3 | 3 | 3 | 3 | 2 | 2 | 2 | 2 | selected as focused-crate prototype path |

## Candidate Crate Outcomes

| Candidate | v1.8 outcome | Evidence | MESH boundary | Risk/condition |
|-----------|--------------|----------|---------------|----------------|
| Blitz | defer direct adoption; accept as reference architecture | `42-SOURCE-INVENTORY.md` Blitz row; https://github.com/DioxusLabs/blitz | Compare as a full reference stack without handing production shell ownership to Blitz. | Direct adoption remains blocked until Wayland shell model fit and browser-engine-level overhead are measured. |
| Skia/rust-skia | defer as fallback | `42-SOURCE-INVENTORY.md` Skia and rust-skia rows; `.planning/spikes/MANIFEST.md` | Treat as an alternate paint backend for retained display-list commands. | Use only if AnyRender/Vello-style evidence fails or Skia's capability gain outweighs native build and binary cost. |
| Stylo | defer pending focused style proof | `42-SOURCE-INVENTORY.md` Stylo row; https://github.com/servo/stylo | Evaluate as style resolution machinery, not as permission to import browser-engine scope. | Browser-grade CSS power may carry sync, licensing, and dependency complexity. |
| Taffy | accept for focused layout prototype | `42-SOURCE-INVENTORY.md` Taffy row; https://taffylayout.com/docs | Drive layout from MESH retained nodes and custom measurement. | Re-check exact API and text measurement fit before dependency adoption. |
| Parley | accept for focused text prototype | `42-SOURCE-INVENTORY.md` Parley row; https://docs.rs/parley/latest/parley/ | Replace or adapt current text layout/cache boundaries while retaining MESH invalidation. | Prove selection geometry, bidi, line breaking, and cache behavior on shipped surfaces. |
| AnyRender | accept for preferred paint abstraction prototype | `42-SOURCE-INVENTORY.md` AnyRender row; https://github.com/DioxusLabs/anyrender | Map MESH retained display-list commands into backend-agnostic draw commands. | Re-check API stability, CPU fallback, and backend availability before production use. |
| Winit | accept for throwaway harnesses; defer production shell adoption | `42-SOURCE-INVENTORY.md` Winit row; https://docs.rs/winit/ | Use for Phase 43 harnesses where Blitz or focused crates need a conventional window/event loop. | Production adoption must prove coexistence with Wayland/layer-shell lifecycle. |
| AccessKit | accept for retained-node accessibility boundary | `42-SOURCE-INVENTORY.md` AccessKit row; https://accesskit.dev/ | Map MESH retained node identity to AccessKit node/tree updates. | Prove Unix adapter, action routing, and incremental update semantics. |
| Muda | defer | `42-SOURCE-INVENTORY.md` Muda row; https://docs.rs/muda | No renderer boundary in Phase 42; native menus are out of current scope. | Revisit only if a concrete native menu need appears. |
| html5ever | defer | `42-SOURCE-INVENTORY.md` html5ever row; https://github.com/servo/html5ever | No `.mesh` authoring requirement for HTML import in Phase 42. | Revisit only if Blitz HTML parsing or imported markup becomes part of v1.8 proof scope. |
| xml5ever | defer | `42-SOURCE-INVENTORY.md` xml5ever row; https://docs.rs/crate/xml5ever/latest | No XHTML/XML import requirement in Phase 42. | Revisit only if XHTML/XML parsing becomes necessary for Blitz or markup import. |

## Provisional Path Selection

Phase 43 should prototype Blitz as a reference path and a MESH-owned focused-crate path; production adoption remains undecided until those prototypes produce comparable evidence.

## Final Coverage Checklist

- REND-01: covered
- REND-02: covered
- REND-03: covered

### Context Decision Coverage

- [x] D-01: Blitz is the reference architecture by default while MESH-owned boundaries remain preferred unless direct adoption proves cleaner.
- [x] D-02: Direct adoption may justify redesign only when explicit, measurable, and matrix-backed.
- [x] D-03: Wayland shell model mismatch is a hard blocker for direct Blitz adoption.
- [x] D-04: Browser-engine-level performance overhead is a hard blocker for direct Blitz adoption.
- [x] D-05: If Blitz fails direct adoption gates, useful architecture and crates still remain borrowable.
- [x] D-06: Direct Blitz adoption has exactly the two hard blockers tracked in the blocker table.
- [x] D-07: Capability gain matters after hard blockers pass.
- [x] D-08: Performance overhead includes runtime, startup, compile, binary, memory, resource, and native dependency costs.
- [x] D-09: Prototype observability may temporarily regress only if final migration restores MESH invalidation, damage, profiling, diagnostics, and debug payloads.
- [x] D-10: Build/dependency cost is acceptable only when significant renderer capability is unlocked and measured.
- [x] D-11: Skia/rust-skia is a fallback path if Blitz/AnyRender cannot meet MESH needs.
- [x] D-12: Taffy and Parley are accepted as strong standalone layout/text candidates for prototype work.
- [x] D-13: Stylo is a direct candidate for style resolution, deferred pending focused proof.
- [x] D-14: AnyRender/Vello-style abstraction is accepted as the preferred paint abstraction prototype before Skia fallback.
- [x] D-15: Winit is accepted for throwaway harnesses and AccessKit is accepted for retained-node accessibility.
- [x] D-16: Muda, html5ever, and xml5ever are deferred unless a concrete need appears.
- [x] D-17: Phase 43 compares both navigation bar and audio popover.
- [x] D-18: Phase 43 prototypes are throwaway harnesses, not production-wired paths.
- [x] D-19: Phase 43 compares visual output plus interaction shape: hover, click, slider, and open-close behavior.
- [x] D-20: Phase 42 hands off a decision matrix only; Phase 43 planners infer prototype details from scores and constraints.
- [x] D-21: Two-surface scope is not reduced; navigation bar and audio popover are both required.

### Final Phase 42 Verdict

Phase 42 selects a dual-prototype handoff: Blitz remains the reference/direct-adoption candidate under hard blockers, while the MESH-owned focused-crate path advances with Taffy, Parley, AnyRender, and AccessKit as preferred standalone candidates.
