# Renderer Migration Roadmap

## Scope

MIGR-01: broad renderer migration is phased and reversible; whole-renderer rewrite is not the first migration step.

This roadmap translates the v1.8 renderer decision, prototype comparison, and focused production proof into future migration steps. It is a maintainer contract for adoption sequencing. It does not replace `mesh-core-render`, `mesh-core-presentation`, `.mesh` authoring syntax, shell surface lifecycle behavior, or the existing observability path by itself.

## Source Evidence

- Phase 42: direct Blitz adoption remains blocked by Wayland shell model fit, browser-engine-level overhead concerns, and later high-level crate compile evidence.
- Phase 43: the MESH-owned focused-crate path advanced because retained identity was explicit across layout, text, paint, interaction, and accessibility evidence.
- Phase 44: focused proof integration preserved NodeId identity, typed invalidation, damage, profiling, diagnostics, selection, and AccessKit-compatible update evidence behind existing ownership.

Current source boundaries also matter:

- `mesh-core-component` parses author-facing `.mesh` source.
- `mesh-core-frontend` compiles and lowers frontend source into widget trees.
- `mesh-core-elements` exposes retained widget, tree, style, and layout APIs.
- `mesh-core-render` paints widget trees into `PixelBuffer`s.
- `mesh-core-presentation` presents `PixelBuffer`s through dev-window or layer-shell backends.
- `mesh-core-shell` connects runtime events, service state, frontend output, rendering, diagnostics, and presentation events.

## Migration Principles

- Reversibility: every renderer migration step must have a local bypass, rollback path, or feature flag before it can land as a production default.
- Current authority first: current parser, frontend compiler, retained runtime tree, render object tree, retained display list, software painter, diagnostics, profiling, damage, and Wayland presentation remain authoritative until a later step explicitly replaces them.
- Adapter before replacement: the Phase 44 focused proof boundary should harden into adapter seams before any module-by-module replacement begins.
- Observability parity: NodeId identity, typed invalidation, damage, profiling, diagnostics, debug payloads, theme-owned selection, and AccessKit-compatible update evidence are promotion gates, not optional debug extras.
- Author contract stability: `.mesh` authors should keep writing MESH UI primitives and service-driven components while renderer internals migrate behind the public authoring surface.

## Phased Roadmap

| Step | Objective | Boundary Changed | Feature flag | CI gates | Rollback path | Author-facing effect |
|------|-----------|------------------|--------------|----------|---------------|----------------------|
| Step 1: adapter seam hardening | Turn Phase 44 proof evidence into a stable internal adapter boundary. | `FocusedProofSnapshot`, focused text/layout/paint evidence, and focused accessibility update construction. | required before default shell use | focused renderer proof tests, phase44 shell tests, selection proof, workspace tests | disable focused adapter and keep current render object/display-list path authoritative | none; proof snapshots remain internal evidence |
| Step 2: layout and text candidate integration | Evaluate production Taffy/Parley-shaped integration behind retained MESH nodes. | Taffy-backed layout beneath retained `WidgetNode`/`NodeId` authority; text shaping remains a future candidate path. | required for all shipped surfaces | render proof tests, shipped navigation/audio regressions, text selection tests, profiling snapshots | revert the Phase 47 layout replacement commit if an in-scope blocker is found; no silent runtime fallback is kept | no syntax change; existing `.mesh` layout/control semantics remain stable |
| Step 3: paint backend abstraction | Introduce an AnyRender/Vello-style paint backend seam without replacing presentation ownership. | Paint command execution below retained display-list ownership. | required per backend | display-list tests, damage tests, profiling/debug payload checks, workspace tests | switch back to software painter | no public API change; visual differences require explicit regression acceptance |
| Step 4: accessibility runtime expansion | Expand AccessKit-compatible retained-node updates toward a fuller runtime. | Accessibility update publication beneath retained node identity. | required per platform/runtime path | AccessKit-compatible update tests, navigation/focus regressions, shipped surface tests | retain current metadata-only accessibility boundary | author-facing accessibility attributes continue to map from `.mesh` metadata |
| Step 5: optional style/parser expansion | Consider Stylo-style resolution or parser-profile expansion only when it preserves MESH's bounded UI profile. | CSS/profile validation and lowering, not arbitrary browser semantics. | required for experimental profile work | compiler diagnostics tests, `.mesh` syntax tests, style/restyle tests | keep current bounded CSS parser/resolver | only documented `.mesh` profile changes become public |
| Step 6: blocked Blitz reconsideration | Revisit direct Blitz only if current blockers are cleared and the shell ownership model fits MESH. | Potentially broad renderer architecture, still behind explicit gates. | not allowed until Blitz compile and shell ownership blockers are cleared | new proof harness, dependency/binary-size report, Wayland ownership proof, full workspace tests | reject direct adoption and continue MESH-owned adapter path | none until a later public contract revision |

## Promotion Gates

### Broad Adoption Checklist

- [ ] Feature flag or equivalent local bypass documented
- [ ] Rollback path documented
- [ ] Linux/Nix impact documented
- [ ] Root workspace dependencies documented
- [ ] Native libraries documented
- [ ] Binary/build risk documented
- [ ] CI gates documented
- [ ] Workspace tests documented
- [ ] Focused renderer proof tests documented
- [ ] Shipped navigation/audio surface regressions documented
- [ ] Selection proof documented
- [ ] Invalidation/damage/profiling evidence documented
- [ ] AccessKit-compatible update evidence documented

MIGR-03: build, CI, feature flags, Linux/Nix dependency implications, and binary-size risk are documented before broad adoption.

### Required Commands

- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render proof`
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase44`
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase44_navigation`
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test --workspace`

### Dependency Record Template

| Field | Required content |
|-------|------------------|
| Linux/Nix impact | New Nix dev-shell packages, Linux runtime assumptions, Wayland/session effects, and environment variable changes. |
| Root workspace dependencies | New Cargo workspace dependencies, feature flags, and crate ownership changes. |
| Native libraries | Native libraries, pkg-config requirements, dynamic linking concerns, and runtime library paths. |
| Binary/build risk | Build-time increase, binary-size risk, dependency fan-out, cache effects, and mitigation. |
| CI gates | Exact commands, jobs, or manual equivalents required before promotion. |
| Rollback path | How to disable, bypass, or revert the new path without breaking shipped surfaces. |

## Phase 46 Renderer Library Dependency Record

| Field | Phase 46 record |
|-------|-----------------|
| Linux/Nix impact | Default builds need no new Nix packages, and full Vello/wgpu native risk is deferred. |
| Root workspace dependencies | `taffy 0.10.1`, `parley 0.7.0`, `accesskit 0.24.0`, `anyrender 0.10.0`, and `vello_encoding 0.5.1`. |
| Feature flags | `renderer-taffy`, `renderer-parley`, `renderer-accesskit`, `renderer-anyrender`, `renderer-vello-encoding`, and `renderer-libraries`. |
| Native libraries | No new default runtime native libraries; full `vello`/`wgpu` deferred to Phase 49. |
| Binary/build risk | Optional dependency fan-out is measured through `cargo tree -p mesh-core-render --features renderer-libraries`. |
| CI gates | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo check -p mesh-core-render`; `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo check -p mesh-core-render --features renderer-libraries`; `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render renderer_library`; `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render --features renderer-libraries renderer_library`; `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render proof`; `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase44`; `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase44_navigation`; and workspace tests where feasible. |
| Rollback path | Leave default features empty and route later behavior through `renderer_library_rollback_authority() == "mesh-software-renderer"`. |

Latest `parley 0.9.0`, `parley 0.8.0`, `vello 0.9.0`, and `vello_encoding 0.9.0` require Rust 1.88 and are not selected for the Rust 1.85 workspace.

## Phase 47 Taffy Layout Replacement Record

Phase 47 promotes Taffy for in-scope layout computation. `mesh-core-elements` owns the Taffy dependency because it owns `LayoutEngine`, retained `WidgetNode` geometry storage, and the text measurement injection point used before rendering.

For Phase 47, unsupported cases produce diagnostics or blocker records rather than silent old-engine fallback. This intentionally narrows the Phase 46 rollback posture for layout only: non-layout renderer-library candidates remain gated, but in-scope MESH layout semantics move to Taffy-backed computation while MESH retains `NodeId`, runtime keys, dirty categories, render-object synchronization, diagnostics, profiling, damage, and presentation ownership.

Final Phase 47 gate commands:

- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-elements phase47_taffy`
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-elements layout`
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase47`
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase44_navigation`
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render proof`
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo check -p mesh-core-shell`

The audio popover transition delay remains deferred to v1.10 and is not part of the Phase 47 Taffy layout replacement scope.

### Observability Promotion Gate

A renderer path cannot become authoritative until it preserves or replaces:

- `NodeId` retained identity,
- typed invalidation categories,
- damage evidence,
- profiling records,
- non-fatal diagnostics,
- debug payloads,
- theme-owned selection behavior,
- AccessKit-compatible retained-node update evidence.

## Deferred And Blocked Paths

- Direct Blitz adoption remains blocked by compile/API evidence and unresolved shell ownership fit.
- A whole-renderer rewrite is not part of the first migration step.
- Full browser compatibility remains out of scope.
- Winit shell ownership is not a production replacement for current Wayland shell ownership.
- Skia remains fallback evidence unless a future paint backend step proves it is needed.
