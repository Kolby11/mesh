# Phase 43 Research: Comparable Renderer Prototype Proofs

## Research Complete

Phase 43 should be planned as an isolated prototype harness, not a production renderer branch. The useful proof is a same-input comparison between:

- Blitz reference/direct-adoption evidence for `PROTO-01`.
- A MESH-owned focused-crate path using retained MESH-shaped fixtures for `PROTO-02`.
- A common comparison frame for `PROTO-03`.

## Phase Constraints That Matter

- Both paths must cover navigation bar and audio popover surfaces.
- Both paths must cover hover, click, slider movement/release, and open-close behavior.
- The Blitz path may use HTML/CSS-equivalent inputs instead of `.mesh` ingestion.
- The focused path should preserve stable node identity, layout/style/text/icon data, and display-list-like paint commands.
- Prototype artifacts must stay throwaway and must not modify `mesh-core-render` or `mesh-core-presentation`.
- A Blitz failure is acceptable only when the attempted harness, API boundary, error or mismatch, and reproduction steps are recorded.

## External Source Findings

| Candidate | Current source-backed finding | Planning implication |
|-----------|-------------------------------|----------------------|
| Blitz | The Blitz README describes it as pre-alpha, notes many bugs and missing features, and says it is not yet recommended for building apps. It also documents high-level crates, HTML/markdown rendering, Dioxus-native rendering, and examples such as `browser`, `readme`, and `wgpu_texture`. | Treat Blitz as a reference/blocker-evidence path. Plan for either a renderable HTML/CSS fixture or a reproducible blocker, not production adoption. |
| Blitz architecture | Blitz is modular: `blitz-dom` owns DOM/style/layout/event handling, `blitz-paint` translates to AnyRender, `blitz-html` parses HTML, and `blitz-shell` integrates Winit, AccessKit, and Muda. | The prototype should record which exact boundary was attempted: high-level `blitz`, lower-level `blitz-dom`/`blitz-paint`, or shell/window path. |
| Taffy | Taffy 0.10.1 supports Flexbox, Grid, and Block layout. Its high-level `TaffyTree` API computes layout from `Style`, children, and optional measurement context. Its docs recommend the low-level API for embedding into an existing UI tree. | Use Taffy for focused layout proof and keep MESH node IDs authoritative outside Taffy's storage. The first prototype can use `TaffyTree`; if identity mapping becomes awkward, document the low-level API as Phase 44 work. |
| Parley | Parley 0.9.0 provides rich text layout. `FontContext` and `LayoutContext` are shared resources; `Layout` supports shaping, line breaking, bidi reordering, and alignment. | Use Parley to produce measurable text runs or text bounding output for status/title/percent labels. A full text renderer is unnecessary for Phase 43. |
| AnyRender | AnyRender 0.8.0 is a 2D drawing abstraction. Its `PaintScene` trait accepts drawing commands; `ImageRenderer` renders to RGBA buffers; `WindowRenderer` renders to surfaces/windows. Backends include Vello and CPU Vello. | Use AnyRender concepts for the focused display-list boundary. If backend rendering is too expensive, a recorded scene/command stream still gives comparable paint evidence. |
| AccessKit | AccessKit 0.24.0 exposes stable `NodeId`, `TreeId`, `Tree`, `Node`, and `TreeUpdate` types. A complete UI is represented as an accessibility tree. | Focused prototype should emit an accessibility boundary report that maps retained MESH node IDs to AccessKit node IDs and roles for the two surfaces. |
| Winit | Winit 0.30.13 provides cross-platform window creation and event loop management, but does not draw; rendering must be provided separately. | Winit is acceptable for throwaway Blitz/focused harnesses, but the plan should not require production Wayland/layer-shell replacement. |

## Local Source Findings

| Source | Finding | Planning implication |
|--------|---------|----------------------|
| `modules/frontend/navigation-bar/src/main.mesh` | Required fixture content includes `status-primary`, `status-secondary`, `control-cluster`, `VolumeButton`, `ThemeButton`, `SettingsButton`, and `AudioPopover hidden={audio_surface_hidden}`. | Shared fixture must include nav baseline and volume-trigger/open path, with status text and three control buttons. |
| `modules/frontend/audio-popover/src/main.mesh` | Required fixture content includes `audio-title`, `audio-status`, `audio-percent`, `audio-slider`, `onVolumeChange`, `onVolumeRelease`, mute/up/down buttons, and `.mesh-surface-exiting` opacity. | Shared fixture must include visible popover, slider change/release, mute action, and close/exiting state. |
| `crates/core/frontend/render/src/render_object.rs` | Retained render object dirty slots include transform, clip, opacity, geometry, material, text, and accessibility. | Focused prototype evidence should keep these categories visible, even if only as structured JSON. |
| `crates/core/frontend/render/src/display_list.rs` | Display-list data is keyed by retained `NodeId` and primitive slot, with metrics for retained/rebuilt entries, damage, filtered commands, and batch barriers. | Focused prototype should emit display-list-like commands keyed by stable node ID and slot. It does not need full production metrics. |
| `crates/core/presentation/src/lib.rs` | Production presentation selects Wayland layer-shell when available and has a dev-window fallback. Damage-aware present remains owned by MESH. | Do not plan Winit/Blitz as production shell ownership. Windowed render proof is throwaway only. |

## Recommended Prototype Layout

Use an isolated standalone Cargo prototype outside the workspace membership:

- `.planning/prototypes/phase43/Cargo.toml`
- `.planning/prototypes/phase43/README.md`
- `.planning/prototypes/phase43/fixtures/phase43-scenarios.json`
- `.planning/prototypes/phase43/src/lib.rs`
- `.planning/prototypes/phase43/src/bin/blitz_reference.rs`
- `.planning/prototypes/phase43/src/bin/focused_crate.rs`
- `.planning/prototypes/phase43/evidence/blitz-reference.md`
- `.planning/prototypes/phase43/evidence/focused-crate.md`
- `.planning/prototypes/phase43/output/*.json`

The nested `Cargo.toml` should include its own empty `[workspace]` table so `cargo --manifest-path .planning/prototypes/phase43/Cargo.toml ...` does not try to join the repository workspace.

## Fixture Contract

The common fixture should name exactly these scenarios:

| Scenario ID | Surface | Required evidence |
|-------------|---------|-------------------|
| `nav-baseline` | navigation bar | root/nav-shell/control cluster geometry, status text, volume/theme/settings controls |
| `nav-audio-trigger-hover` | navigation bar | hover target is the volume trigger and state change is recorded |
| `audio-popover-visible` | audio popover | title/status/percent labels, icon state, slider, mute/up/down controls |
| `audio-slider-change-release` | audio popover | slider value changes from `0.42` to `0.73`, release event recorded |
| `audio-popover-close` | audio popover | hidden/close or exiting opacity state recorded |

The fixture should contain stable node IDs such as `nav.root`, `nav.status.primary`, `nav.controls.volume`, `audio.root`, `audio.slider`, and `audio.actions.mute`.

## Validation Architecture

Use artifact-driven validation because Phase 43 is proof work, not production integration.

- Quick command: `cargo check --manifest-path .planning/prototypes/phase43/Cargo.toml`
- Structural command: `rg -n "nav-baseline|audio-slider-change-release|audio-popover-close" .planning/prototypes/phase43/fixtures/phase43-scenarios.json`
- Evidence command: `rg -n "visual/layout fidelity|interaction shape|retained identity fit|accessibility boundary|build/dependency cost|blocker evidence|Phase 44 integration readiness" .planning/phases/43-comparable-renderer-prototype-proofs/43-PROTOTYPE-COMPARISON.md`
- Blocker fallback: if Blitz cannot compile/render, `43-02-SUMMARY.md` and `evidence/blitz-reference.md` must include `Attempted harness`, `Crate/API boundary`, `Observed error or mismatch`, and `Reproduction`.

## Planning Recommendations

1. Plan shared fixtures and harness skeleton first.
2. Plan Blitz and focused-crate prototypes in parallel after the fixture exists.
3. Plan the final comparison after both evidence paths finish.
4. Keep all dependencies inside the isolated prototype manifest; do not add them to root `Cargo.toml`.
5. Treat screenshots as optional. Structured JSON layout/paint/interaction/accessibility output is sufficient when pixel rendering would exceed throwaway scope.

## Sources

- Blitz README: https://github.com/DioxusLabs/blitz
- Taffy documentation: https://taffylayout.com/docs
- Taffy docs.rs 0.10.1: https://docs.rs/taffy/latest/taffy/
- Parley docs.rs 0.9.0: https://docs.rs/parley/latest/parley/
- AnyRender README: https://github.com/DioxusLabs/anyrender
- AnyRender docs.rs 0.8.0: https://docs.rs/anyrender/latest/anyrender/
- AccessKit docs.rs 0.24.0: https://docs.rs/accesskit/latest/accesskit/
- Winit docs.rs 0.30.13: https://docs.rs/winit/latest/winit/

